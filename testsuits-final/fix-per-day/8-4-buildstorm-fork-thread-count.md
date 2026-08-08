# 8-4 BuildStorm 多线程 fork 线程计数快路径

## 问题概述

多线程进程调用 `fork()` 时，旧实现先在进程控制块锁内克隆整个任务列表，再逐个取得
任务控制块锁统计存活线程，并为正常的多线程 fork 打印警告。Cargo 主进程有 15 个
线程，因此每次创建编译子进程都会产生与线程数成正比的锁、引用计数和串口输出成本。

## 背景知识

先打个比方：普通派生进程像“复印一整套办公室资料，再让两家公司各自办公”；
创建线程则像“在同一间办公室里加一个工位”。资料可以共用，但每个工位都得有自己
处理临时事务的桌面，否则两个人同时办事时，纸张和步骤会混在一起。

```text
fork（创建新进程）
  父进程
  ├─ 自己的页表 ──> 自己的地址空间
  └─ fork()
       └─ 子进程
          ├─ COW（写时复制）的页表副本
          └─ 独立地址空间

  结果：父、子最初看到相同内容，写入后各自保留自己的页面；
        它们是两个地址空间，不是同一组线程。

clone（按标志创建任务）+ CLONE_THREAD（创建同一线程组的线程）
  同一个 mm_struct（地址空间描述结构）
  ├─ 父线程：自己的 task_struct（内核任务描述结构）
  │          + 自己的 kernel stack（内核栈）
  └─ 新线程：自己的 task_struct
             + 自己的内核栈

  结果：两个线程共用同一个地址空间，但内核执行现场彼此独立。
```

BuildStorm（并发编译压力负载）会同时编译很多代码。它频繁调用 `fork()`、
`exec`（替换进程映像），还会密集读写文件，所以会同时压到进程管理、文件系统和
内存分配。`Cargo`（Rust 包管理和构建工具）主进程大约有 15 个线程；它每启动一个
编译器子进程，内核都要处理一次多线程进程发起的 `fork()`。

`fork()` 会创建一个全新的进程：子进程拿到新的 PID（进程号）、独立的地址空间，
以及父进程文件表的副本。内存页一开始通常通过 COW 共用，任一方写入时才真正复制，
所以“刚 fork 完看起来一样”和“属于同一个地址空间”不是一回事。

`clone()` 更灵活，具体行为由标志决定。带 `CLONE_THREAD` 时，它创建的是同一线程组
里的线程；这些线程共享地址空间、文件表、信号处理器和进程 PID，也就是属于同一个
线程组。每个线程仍有自己的线程 ID，内核也把它们当作可独立调度的任务。

线程共享地址空间，是为了让用户代码直接访问同一份堆、全局数据和地址映射；所有
用户栈也位于这个共同地址空间里，不过每个线程通常使用不同的用户栈区域。进入内核
后情况不同：每个线程必须有自己的内核栈，才能独立保存系统调用、异常和中断的调用
现场。要是两个线程共用一份内核栈，同时进入内核时就会覆盖彼此的返回地址和调用帧。

旧实现的问题出在线程计数。它先把整个任务列表克隆成一个 `Vec`（动态数组），数组
里放的是 `Arc`（原子引用计数智能指针）；随后再逐个锁住 TCB（任务控制块），检查
对应任务是否仍然存活。线程数是 n 时，这就是 O(n)（成本随线程数线性增长）的扫描，
而且要拿 n 次锁，期间还会反复修改引用计数。

平时这可能不显眼，但 Cargo 不断派生编译器时，线程计数就落在每次 `fork()` 的必经
路径上，很快变成热点。Linux（主流类 Unix 内核）不在这里重新扫描任务列表，而是在
`signal_struct::nr_threads`（Linux 线程组的线程计数字段）中维护一个
atomic counter（原子计数器）：创建线程时加一，线程退出时减一。读取线程数因此是
O(1)（固定成本），只读一次计数，不需要逐任务加锁。

## 如何发现

900 秒 `tg-xtask` 基线日志中有 239 条多线程 fork warning。源码审查命中
`ProcessControlBlock::fork_with_task()` 对 `parent.tasks` 的复制和逐任务
`TaskUserRes::is_some()` 检查。命令与日志：

```sh
rg -n 'fork_with_task|live_threads|TaskUserRes|multithread.*fork' os/src/task
cd /work/tgoskits
timeout 900 cargo build -p tg-xtask
```

```text
.tmp/final-runs/20260804-014659-loongarch64-shell/serial.log
.tmp/final-runs/20260804-020444-loongarch64-shell/serial.log
.tmp/final-runs/20260804-022801-loongarch64-shell/serial.log
.tmp/final-runs/20260804-022549-loongarch64-shell/serial.log
```

前两份是固定窗口构建对照，后两份是 16 线程进程执行 128 次 fork/wait 的五轮微基准。
微基准直接覆盖热点，避免仅凭 Cargo crate 顺序波动下结论。

## 怎么解决

项目已有 `live_threads: AtomicUsize`，并在 `TaskUserRes` 注册成功和
`LiveThreadRetirement::retire()` 时精确增减。fork 快路径直接读取
`live_thread_count()`：

```text
旧：PCB lock -> clone all TCB Arc -> unlock -> lock every TCB -> count
新：live_threads.load(Acquire)
```

该计数仍决定多线程非 `CLONE_VM` fork 后是否只保留子进程主 trap context；文件表、
信号、写时复制地址空间和进程标识符生命周期没有改变。正常 Linux 多线程 fork 是合法
行为，因此删除无条件 warning，而不是限频隐藏。

Linux `signal_struct::nr_threads` 在 `copy_process()` 和 `__exit_signal()` 生命周期
边界维护，`get_nr_threads()` 只做 O(1) 读取。CongCore 没有 Linux 的 tasklist
读-复制-更新机制和完整 signal lock，但已有原子计数与统一退休票据，直接复用比另建
扫描快路径更可靠。

## 对应提交

- 内核：`d0679eb386789824b66f8bc7988e8699f85e9f9c`
  `fork: use maintained live-thread count`。
- 回归：`a5c2bda1912352632f4ee0818b923726a546da65`。
- 顶层集成：`c20643ef3e2e9a888d65d104d494b90d2cc285dd`。
- 文档提交：`b411e17941e0c183c95701c4ecf9469a568e7119`。

## 对比提升

128 次 fork/wait 的五轮中位数 `159927 us -> 139321 us`（-12.9%）。相同 900 秒
构建窗口内，Cargo 行 `119 -> 130`（+9.2%），deps 文件 `334 -> 371`（+11.1%），
warning `239 -> 0`。两次构建都以 timeout 124 结束，所以只证明热点减少，不代表
`tg-xtask` 或 BuildStorm 完成。

以下是 AI 的具体分析，作为存档。

---

## 历史分析背景

下面保留的是当时完整的诊断、实现和验证记录，便于以后追溯结论所依据的日志与数据。
这段长分析也记录了优化边界和未完成项，后续工作可以据此复现实验，而不必重新猜测上下文。

## 1. 结论

本批次修复了 `tg-xtask` 编译现场中一个已量化的多线程 `fork()` 热路径：旧实现
每次 fork 都复制线程组任务列表、逐个获取 TCB 锁统计存活线程，并在正常的多线程
fork 上无条件打印 warning。Cargo 主进程有 15 个线程，部分 native build script
也有 2～4 个线程，因此这个路径同时放大了锁竞争、`Arc` 操作和串口输出。

修改参考 Linux 的 `signal_struct::nr_threads`：在线程创建/退出的生命周期边界维护
计数，fork 只读取现有的 `live_threads` 原子计数，不遍历任务表；正常的多线程 fork
不再打印告警。

结果如下：

- 16 线程进程执行 128 次 fork/wait，5 轮中位耗时由 `159927 us` 降至
  `139321 us`，降低 **12.9%**；
- 同一 900 秒 `cargo build -p tg-xtask` 窗口内，Cargo 输出的编译进度行由 119
  增至 130，`target/debug/deps` 文件数由 334 增至 371；
- `tg-xtask` 窗口中的 fork warning 由 239 条降为 0；
- 双架构静态构建以及 LoongArch fork、exec de-thread、wait race、page cache、mmap、
  truncate 和 open-unlinked 回归均通过。

这是一项有效但有限的优化。`tg-xtask` 在 900 秒窗口内仍未完成，返回 124；本批次
没有运行完整 BuildStorm，也不声称 BuildStorm 已通过。

## 2. 版本与资产

| 资产 | 值 |
| --- | --- |
| 顶层基线 | `1427fea2` |
| `os/` 基线 | `e625f82` |
| `os/` 最终 revision | `d0679eb386789824b66f8bc7988e8699f85e9f9c` |
| 顶层回归提交 | `a5c2bda1912352632f4ee0818b923726a546da65` |
| 顶层内核集成提交 | `c20643ef3e2e9a888d65d104d494b90d2cc285dd` |
| final 测试分支/commit | `final-2026` / `1eac61d3becaa592c8ef12a7535f0ec6bb9e3e36` |
| Linux 参考树 | `fc02acf6ac0ccde0c805c2daa9148683cdd01ba8` |
| 架构 | LoongArch64，12 vCPU，8 GiB |
| QEMU | 11.0.3 |
| 镜像模式 | snapshot |
| 决赛镜像 | `sdcard-la-pub.img`，14 GiB raw ext4 |
| 镜像 SHA-256 | `2ad9d955684297abe9db48d94f1b7fcc488268fc8f481408c55b1ec27f520c6a` |
| Rust 工具链 | `nightly-2026-05-28` |

`run.sh` 在最终运行态回归前重新扫描并确认了镜像校验值。没有更新 final 测试源码，
也没有修改基准镜像。

## 3. 根因

旧的 `ProcessControlBlock::fork_with_task()` 在持有 PCB 锁时把 `parent.tasks` 中的
每个 TCB 克隆成临时 `Vec<Arc<_>>`。释放 PCB 锁后，它再逐个获取 TCB 锁并以
`TaskUserRes::is_some()` 统计线程数：

```text
PCB lock
  -> walk task slots
  -> clone every live TCB Arc
drop PCB lock
  -> lock every cloned TCB
  -> inspect TaskUserRes
  -> warn whenever count != 1
```

这个统计仅用于两个决定：多线程非 `CLONE_VM` fork 后，子 mm 只保留主 trap
context；以及一条诊断告警。项目已经有 `live_threads: AtomicUsize`，并在
`TaskUserRes` 注册与统一退出 retirement 上精确增减，因此重新遍历任务表没有提供
额外语义。

多线程进程调用普通 fork 是正常 Linux 行为。每次打印 warning 不但制造误报，还会
进入全局串口输出路径。在基线微基准的 5 × 128 次 fork 中，恰好产生 640 条 warning。

## 4. Linux 对照

直接阅读了本地 `exampleOs/linux`：

| Linux 机制 | 位置 | 本项目对应 |
| --- | --- | --- |
| signal 结构保存线程数 | `include/linux/sched/signal.h:signal_struct::nr_threads` | `ProcessControlBlock::live_threads` |
| 创建线程时递增 | `kernel/fork.c:copy_process()` | `TaskUserRes` 成功注册后 `register_live_thread()` |
| 线程退出时递减 | `kernel/exit.c:__exit_signal()` | `LiveThreadRetirement::retire()` / fallback drop |
| O(1) 读取线程数 | `get_nr_threads()` | `live_thread_count()` |

Linux 在 `tasklist_lock`/`sighand` 生命周期保护下维护 `nr_threads`，而不是在每次
fork 时扫描并锁住 thread-group 中的全部 task。这里保留项目现有的原子计数和
统一 retirement 协议，没有照搬 Linux 的完整 tasklist/RCU 结构。

## 5. 实现

`os/src/task/process_block.rs` 的 fork 快路径现在：

1. 在读取父进程 fork 快照时调用 `self.live_thread_count()`；
2. 删除 `parent_tasks: Vec<Arc<TaskControlBlock>>` 的构造；
3. 删除对所有 TCB 的逐锁统计；
4. 删除正常多线程 fork 的无条件 warning；
5. 继续使用得到的线程数决定是否清理子 mm 中的非主 trap context。

没有改变 `fork`/`clone` ABI、COW、文件表继承、信号处理或 PID 生命周期。

新增 `fork_thread_group_perf_smoke`：创建 15 个共享 VM/FS/files/sighand 的线程，
主线程等待全部上线后执行 128 次 fork/wait，以 `CLOCK_MONOTONIC` 输出耗时。子进程
必须以状态 0 退出，每次 wait 的 PID 和状态均做断言。该测试同时能暴露线程计数、
fork 子 mm trap-context 清理和 wait/reap 回归。

## 6. 900 秒 tg-xtask A/B

两次运行都从 LoongArch 决赛镜像的 snapshot 状态启动，使用 12 vCPU、8 GiB，
在 `/work/tgoskits` 中离线执行：

```sh
timeout 900 cargo build -p tg-xtask
```

| 内核 | 退出码 | 编译输出行 | deps 文件 | fork warning |
| --- | ---: | ---: | ---: | ---: |
| 基线 `e625f82` | 124 | 119 | 334 | 239 |
| 线程计数快路径 | 124 | 130 | 371 | 0 |
| 相对变化 | — | +9.2% | +11.1% | -100% |

日志：

```text
.tmp/final-runs/20260804-014659-loongarch64-shell/serial.log
.tmp/final-runs/20260804-020444-loongarch64-shell/serial.log
```

输出行和 deps 文件数只是固定时间窗口内的进度指标，不能等同于最终编译耗时；Cargo
的并行调度也会带来波动。因此另外使用下述微基准隔离 fork 热路径，而没有只凭一次
BuildStorm 进度就下结论。

## 7. fork 微基准 A/B

每个数据点都是同一 guest 启动内连续执行 128 次 fork/wait 的耗时：

| 轮次 | 基线/us | 快路径/us |
| ---: | ---: | ---: |
| 1 | 159927 | 144428 |
| 2 | 153955 | 141608 |
| 3 | 159451 | 139321 |
| 4 | 164394 | 135391 |
| 5 | 177096 | 133206 |
| 中位数 | **159927** | **139321** |

中位数降低 `20606 us`，即 **12.9%**。两组共 10 次均返回 0；基线产生 640 条
fork warning，快路径为 0。

日志：

```text
.tmp/final-runs/20260804-022801-loongarch64-shell/serial.log
.tmp/final-runs/20260804-022549-loongarch64-shell/serial.log
```

最终内核又独立复跑一次，得到 `135624 us`、返回 0：

```text
.tmp/final-runs/20260804-023220-loongarch64-shell/serial.log
```

## 8. 正确性与回归

最终 LoongArch 运行态回归输出：

```text
FORK_THREAD_GROUP_PERF iterations=128 threads=16 elapsed_us=135624
FORK_RC=0
EXIT_GROUP_PREPARED_WAIT_PASS
PASS
EXEC_THREAD_RC=0
WAIT_WAKEUP_RACE_PASS iterations=256
WAIT_RACE_RC=0
private_file_page_cache_smoke passed
PAGE_CACHE_RC=0
file_mmap_lazy_fault_smoke passed
MMAP_RC=0
shared_file_truncate_cache_smoke passed
TRUNCATE_RC=0
OPEN_UNLINK_LIFETIME_PASS workers=6 iterations=32
UNLINK_RC=0
```

以下静态检查均返回 0：

```sh
cargo fmt --check                       # os/
cargo fmt --check                       # user/
cargo check --target riscv64gc-unknown-none-elf              # os/
cargo check --target loongarch64-unknown-none-softfloat       # os/
cargo check --target riscv64gc-unknown-none-elf \
  --bin fork_thread_group_perf_smoke                            # user/
cargo check --target loongarch64-unknown-none \
  --bin fork_thread_group_perf_smoke                            # user/
git diff --check
```

构建只有仓库既有 warning，没有新增编译错误。测试结束后恢复了
`user/.cargo/config.toml` 的 RISC-V 默认 target。

## 9. 边界与下一步

- 原子 `live_threads` 是本项目对 Linux `signal_struct::nr_threads` 的最小对应，当前
  没有 Linux 的 tasklist RCU、完整 signal lock 或 per-CPU process counter；
- 计数的 release/acquire 顺序与统一 retirement 已由上一批退出生命周期修复建立，
  本批次没有改变其增减时机；
- 完整 `tg-xtask` 仍未在 900 秒内完成。下一步应根据新的活跃 PC、块 I/O 计数和
  allocator 锁等待证据选择瓶颈，不能因为某种结构“更像 Linux”就直接加入；
- 候选的 frame allocator per-hart page cache 必须先证明全局 frame lock/BTreeSet
  是实际热点，并且需要 OOM drain、连续分配、可用页统计与 IOZone A/B；无证据时
  不实施；
- 在 `tg-xtask` 能稳定完成并通过工具链/最小 Cargo 检查前，不运行或宣称完整
  BuildStorm 通过。

## 10. AI 使用说明

AI 用于检索本地 Linux 参考树、对比项目线程生命周期、生成补丁、驱动 QEMU 聚焦
A/B 和整理报告。所有数值来自上述串口日志和实际命令返回码；未硬编码测试结果、
未伪造计时，也未修改 final judge、测试源码或基准镜像。
