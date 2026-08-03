# 8-4 BuildStorm 多线程 fork 快路径

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
