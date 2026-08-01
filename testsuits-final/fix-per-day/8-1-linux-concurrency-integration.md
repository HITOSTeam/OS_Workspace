# 8-1 Linux 并发优化筛选与主线集成

## 1. 结论

本批次审查了 `final-perf-concurrency` 工作树中的并发优化，并以 Linux 的用户
可观察语义、锁边界和对象生命周期作为标准，将通过静态检查与聚焦回归的修改
按职责拆分后集成到主 `os/dev_final`。最终保留三组修改：

| 主线提交 | 范围 | 结论 |
| --- | --- | --- |
| `72ee89b` | frame allocation、COW/lazy fault、RISC-V active-mm TLB shootdown | 保留；耗时页初始化和文件读取移出 mm 全局锁，提交时重验 PTE/VMA。 |
| `9b8059d` | fd close/install/replace 生命周期 | 保留；表锁内只摘除，通知、mount ref 释放和对象析构在锁外完成。 |
| `33fcb6b` | userspace terminal write 串行边界 | 保留；一次 write 不再逐字符交错，阻塞锁在翻译未 pin 的用户页之前取得。 |

以下候选没有进入主线：

- 工作树中的轮询 VirtIO：主线已有 `1ed50a2` 的 IRQ-driven 多请求实现，不能
  再叠加一套 token/完成生命周期，也不应从中断完成退回同步轮询。
- scheduler 全局 runnable 原子快照：两版均在线程 hackbench 中稳定停滞；缩减
  到只统计 RT runnable 后，对普通 workload 没有机制上的广泛收益，烟测还出现
  process-pipe 约 3.2 倍异常回退。
- central buddy + per-hart magazine：首个完成的 5-sample workload 中位数由
  核心组合的 64.17 秒变为 75.60 秒，回退约 17.8%，超过预设 5% 上限；同时
  当前 per-hart cache 没有完整实现 Linux 的 migration/IRQ-safe fast path。

这次筛选坚持两条规则：一是“看起来像 Linux”不等于可合入；二是候选只要在
并发正确性或独立性能门槛中失败，就不靠其他快项抵消风险。

## 2. Linux 对照

参考树为：

```text
exampleOs/linux
fc02acf6ac0ccde0c805c2daa9148683cdd01ba8
```

本地实现复用的是 Linux 的并发原则，不是一比一复制完整子系统：

| 本地问题 | Linux 参考 | 采用的原则 |
| --- | --- | --- |
| 页分配器在全局锁内清零 4 KiB | `mm/page_alloc.c` | free-list 元数据操作和 `prep_new_page()` 类初始化分离。 |
| COW/lazy fault 在 mm 锁内分配、复制或读文件 | `mm/memory.c` 的 `do_cow_fault()`、`finish_fault()` | prepare → lockless work → PTE/VMA recheck and commit。 |
| COW 后向所有核做粗粒度处理 | `arch/riscv/mm/tlbflush.c` 的 `mm_cpumask(mm)` | 只对实际可能使用该 mm 的在线远端 hart 做页粒度 shootdown。 |
| fd 表锁内执行最终 close/drop | `fs/file.c` 的 `file_close_fd_locked()`、`filp_close()` | 锁内摘除 fd，锁外执行可能递归取锁或释放重对象的 close。 |
| 并发 stdout write 字符级交错 | `drivers/tty/tty_io.c` 的 `tty_write_lock()` | 一次 userspace write 是串行单位；`O_NONBLOCK` 竞争返回 `EAGAIN`。 |

## 3. 内存管理：缩短页错误临界区

### 3.1 frame allocator 锁外清零

旧 `frame_alloc()` 在 `FRAME_ALLOCATOR` 自旋锁内构造 `FrameTracker`，而构造会
清零整页并分配 `Arc` 控制块。多个 hart 同时 fault 时，所有 4 KiB 清零都被
串成一条全局通道。

现在锁只保护空闲页号的移除：

```text
FRAME_ALLOCATOR.lock().alloc()
                |
                v  release allocator lock
        clear 4 KiB + build Arc owner
```

页号从 free-list 移除后已由当前调用独占，因此锁外初始化不改变所有权语义。

### 3.2 COW/lazy fault 三阶段提交

`MmRef` 的 COW 和 lazy fault 统一采用：

```text
prepare under mm lock
  snapshot VMA/PTE/backing and pin old frame
                  |
                  v
work without mm lock
  allocate + zero/copy, or file pread
                  |
                  v
commit under mm lock
  revalidate VMA/PTE/source PPN, then install
```

commit 发现竞态时丢弃本轮临时页并重试。前三次走乐观路径；持续竞争后只让
retrying fault 通过 `fault_retry_lock` 串行，但仍执行相同的 prepare/work/recheck，
不会把“竞争次数多”误报成无效地址或 `SIGSEGV`。

lazy fault 还显式传入 `charge_pid`。内核替子 mm 处理
`CLONE_CHILD_SETTID` 一类 fault 时，不再把 child 匿名页记到当前 parent 的
cgroup/memory 账上。

### 3.3 RISC-V active-mm 与块 I/O guard

`AsidContext` 增加 active hart mask，语义对应 Linux `mm_cpumask(mm)`：

- 返回用户态前先发布本 hart 的 active bit，再安装/恢复用户 SATP；
- syscall/trap 中继续使用 user SATP，因此不能在 trap entry 过早清 bit；
- PTE writer 读取 active mask，只向在线远端 hart 发送单页
  `remote_sfence_vma()`；
- 若 hart 在 writer 取快照之后才激活，返回用户态前的本地 flush 覆盖该竞态。

主线 VirtIO 的 `KernelPageTableGuard` 可能跨调度等待 IRQ 完成。为避免任务恢复到
另一个 hart 后恢复旧 user SATP 却没有发布 active bit，guard 额外 pin 对应
`Arc<AsidContext>`，恢复 SATP 前在当前 hart 重新发布它。

LoongArch 保留原有 ASID/TLB 路径；本批次没有把 RISC-V SBI shootdown 机制生搬
到 LoongArch IOCSR IPI。

## 4. fd：锁内摘除，锁外完成 close

`FilesStruct` 引入两个 `must_use` 的所有权载体：

- `DetachedFd`：已经从表中摘除，等待锁外 close notification、mount ref 释放
  和最后的 `Arc<File>` drop；
- `RejectedFd`：安装/替换失败时返还输入 file/mount，防止错误路径在表锁内析构。

close、close_range、exec 的 CLOEXEC、dup/replace、socket/pipe 安装回滚、fork
失败回滚和 idle 批量清理均迁移到这一边界。这样即使 file destructor 触发
socket wakeup、fanotify、mount pin 释放或其他内存分配，也不会递归占用
`FilesLock`。

## 5. TTY：一次 userspace write 为串行单位

`Stdout` 使用 scheduler-aware `KernelMutex<()>`。`syscall_write()` 识别真正的
userspace stdout 后，在翻译用户页之前获取锁，并持有到完整 write 返回：

- 阻塞 fd 等待锁；
- `O_NONBLOCK` 在锁竞争时返回 `EAGAIN`；
- 普通文件、pipe、socket、PTY 和其他伪文件不经过这把锁。

“翻译前取锁”是必要的生命周期边界。当前 `UserBuffer` 内保存的是未 pin 的用户
页切片；若翻译后再等待可睡眠 mutex，调度期间地址空间变化会让切片失效。

## 6. 候选筛选证据

性能门槛使用五类 workload 的 5 次中位数，越低越好：

```text
hackbench: process/socket, process/pipe, thread/socket, thread/pipe
lmbench:   lat_proc fork, -P 8 -W 1 -N 5
```

候选要求：五项中位数的几何平均至少提升 5%，且任一单项不得回退超过 5%。
`tools/analyze_concurrency_focus.py` 固化了这一判据。

### 6.1 核心组合观测值

`33fcb6b` 的 5 次 guest `/proc/uptime` 中位数为：

| workload | 中位数 |
| --- | ---: |
| process/socket | 64.17 s |
| process/pipe | 68.20 s |
| thread/socket | 59.53 s |
| thread/pipe | 59.76 s |
| lat_proc fork | 97.13 s |

这些数字用于同一宿主现场下筛候选，不作为 Linux 或比赛机的绝对性能基线。

### 6.2 scheduler 拒绝原因

原候选让 fair/RT runnable 汇总和 least-loaded placement 读取原子计数，减少依次
锁全部 runqueue。该思路类似 Linux `rq->nr_running`，但本地计数与现有队列、
迁移和节流边界没有形成可靠闭环：

1. 原版在线程 socket hackbench 第 2 次稳定停滞；
2. 增加“计数为零时回查权威队列”后，线程 socket 第 1 次仍停滞；
3. 缩减为仅 RT 计数后，普通任务没有预期广泛收益，烟测 process-pipe 从核心
   中位数 68.20 s 异常到 220.77 s，并且 750-task 压力长时间未完成。

因此没有把计数快路径带入主线。

### 6.3 heap magazine 拒绝原因

候选用完整 central buddy 保留 512 MiB 容量，再给每 hart 的 8 B--16 KiB 幂次
class 提供 intrusive magazine，批量 refill 16、每 class 上限 64、OOM 前 drain。
它修复固定 shard 容量孤岛的方向是合理的，但独立门槛的第一项已经确定失败：

```text
core process/socket median: 64.17 s
heap process/socket median: 75.60 s
regression:                 17.81%
```

此外 idle cleanup 在开中断时可能触发 allocator，而 per-hart cache 使用普通
自旋锁。本项目若继续实现该方向，至少要像 Linux local/per-CPU fast path 一样
明确 migration/preemption 与 IRQ re-entry 约束，不能仅依赖“临界区很短”。

## 7. 验证记录

### 7.1 资产

- kernel main HEAD：`33fcb6b72f90d56d8e99dc4d844d83bbf29a0e08`
- final test source：本地 `final-2026`，
  `1eac61d3becaa592c8ef12a7535f0ec6bb9e3e36`
- 先前只读检查到 remote `final-2026` 为
  `b5ec6ef8497e1818cbdec3b54bb722f036e57972`；按规则没有自动拉取或改基线
- RISC-V image SHA-256：
  `d899fe43d333d1d17ad8a5f8a8b74b68117b8c1ceacfc3843bfeadb1ca705bd1`
- QEMU：11.0.3

### 7.2 静态检查

主工作树以下两架构均通过；现有 warning 不计为本批次失败：

```sh
TMPDIR=$PWD/.tmp cargo check --offline --locked \
  --manifest-path ../os/Cargo.toml \
  --target riscv64gc-unknown-none-elf
TMPDIR=$PWD/.tmp cargo check --offline --locked \
  --manifest-path ../os/Cargo.toml \
  --target loongarch64-unknown-none-softfloat
git -C ../os diff --check
```

### 7.3 并发压力与 LTP

最终组合在 RISC-V 8 hart、2 GiB、snapshot 下完成：

- 400 process task hackbench，200 messages/sender：105.996 s，rc=0；
- `fork03/04/05/07/08/09/10`：全部 rc=0；
- `close01/02/close_range02`：全部 rc=0；
- `close_range02` 的 11 个断言全部 TPASS；
- 总结束标记：`CONCURRENCY_FOCUSED_DONE rc=0`。

日志：

```text
.tmp/concurrency-runs/core-final-regression-retry.log
```

### 7.4 IOZone

聚焦命令保持与 ext4/IRQ 批次一致：

```sh
iozone -t 4 -i 0 -i 1 -r 1k -s 4m
iozone -t 4 -i 0 -i 2 -r 1k -s 4m
```

历史同参数当前 ext4/IRQ 组合的三轮为 76.19、76.23、76.77 s，中位数
76.23 s。本批次现场同时存在三个已运行超过一天、各使用 8 vCPU/8 GiB 且持续
占用约 3.5--4.3 个宿主核的外部 QEMU。最终组合和精确基线 `5a3d3e2` 均在
5 分钟内无法完成第一段，因此当前 A/B 无判别力：

- 不把它写成 IOZone 功能通过；
- 不把宿主争用写成内核回退；
- 不声称本批次得到可信 IOZone 提升幅度。

对应现场日志：

```text
.tmp/iozone/linux-concurrency-final.log
.tmp/iozone/isolate-mm.log
.tmp/iozone/isolate-base.log
```

### 7.5 CAgent

主 `33fcb6b` 使用官方 RISC-V image、8 hart、8 GiB、snapshot 启动成功并进入
`cagent_testcode.sh`，但 600 秒内没有产生任何 `testcase cagent` 记录，外层按
硬超时终止。judge 中显示的 10 个 0 分来自“没有可解析记录”，不是 10 个功能
断言逐项执行后失败，不能写成 CAgent 0/10 的功能结论。

现场同时有另一个 worktree 遗留的三组：

```text
make PIDs: 33023, 37226, 43067
QEMU PIDs: 33134, 37893, 43782
```

三个 `make` 的 PPID 均为 1，命令带 `QEMU_TIMEOUT=0`；QEMU 均使用 `-snapshot`，
已运行约 27--28 小时，每个配置 8 vCPU/8 GiB 并持续占用约 3.5--5.6 个宿主核。
这与精确基线 IOZone 同样无法推进相互印证。由于这些进程属于另一个 worktree，
安全策略拒绝在没有用户明确授权时终止它们。本轮 CAgent 因此记录为“环境阻塞、
未完成”，待释放宿主资源后必须重跑，不能记为通过。

日志：

```text
.tmp/final-runs/20260801-190454-riscv64-cagent/serial.log
```

### 7.6 未运行项目

按用户要求没有运行完整 BuildStorm，也没有运行 unixbench、libcbench 或完整
iozone 套件。没有把工具链检查、最小编译或高 CPU 运行状态冒充 BuildStorm
成功。

## 8. 复现

并发性能与回归工具支持分阶段运行，避免每个候选重复执行长压力：

```sh
CONCURRENCY_PHASE=benchmark CONCURRENCY_SAMPLES=5 \
  tools/run_concurrency_focus.sh <workspace> \
  .tmp/iozone/iozone-root.img <benchmark.log>

CONCURRENCY_PHASE=regression CONCURRENCY_SAMPLES=1 \
  tools/run_concurrency_focus.sh <workspace> \
  .tmp/iozone/iozone-root.img <regression.log>

tools/analyze_concurrency_focus.py <baseline.log> <candidate.log>
```

IOZone 工具的 `IOZONE_RUNS=1` 可用于提交级隔离，默认仍为三轮：

```sh
IOZONE_RUNS=1 IOZONE_SKIP_BUILD=1 IOZONE_KERNEL_ELF=<kernel-elf> \
  tools/run_iozone_focus.sh <workspace> \
  .tmp/iozone/iozone-root.img <iozone.log>
```

CAgent：

```sh
ARCH=riscv64 SMP=8 MEM=8G IMAGE_MODE=snapshot \
  BOOT_TIMEOUT=900 TEST_TIMEOUT=600 LOG=error ./run.sh cagent
```

## 9. AI 使用说明

AI 用于：比较候选 diff、检索本地 Linux 参考树、推导锁与对象生命周期、生成
聚焦测试工具、执行静态/运行态验证并整理报告。所有保留或拒绝结论均基于本地
源码、可复现日志和实际命令；没有生成伪造 benchmark、硬编码测试名返回值、
修改评分脚本或篡改 `/proc/uptime`。
