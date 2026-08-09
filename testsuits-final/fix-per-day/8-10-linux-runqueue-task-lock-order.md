# 8-10 参照 Linux 消除 runqueue 到任务状态锁的反向依赖

## 修复概括

LoongArch 12-hart BuildStorm 在高并发编译阶段会突然进入吞吐悬崖：QEMU 仍消耗约
12 个宿主核，但串口、磁盘和构建进度长时间不再推进。归档内核的逐 vCPU PC 采样把现场
定位到同一个反馈环：多个唤醒者在 `wakeup_should_preempt_task()` 中等待当前任务的
`TCB.inner`，另一个 hart 在进入 `ppoll` 的 `PreparedWait` 状态转换附近等待相关任务锁，
而若干 runqueue 路径又会在持有 rq 锁时阻塞获取 `TCB.inner`。

本批按照 Linux 的锁所有权边界，将“物理 rq 成员关系”收敛到 rq 锁和原子
`ready_queue_hart`，禁止持 rq 锁时等待宽泛的任务状态锁；需要完整任务状态的校验移到
释放 rq 锁之后。精确的 EEVDF 唤醒比较改为一次非阻塞快照，竞争时由 rq 中实际的下一实体
完成保守判定。

对应 `os/` 提交：

```text
270b8af sched: avoid task locks in wakeup paths
```

## 问题概述

修复前，调度器的几条热路径混用了两套锁域：

```text
wakeup / enqueue
  -> TCB.inner
  -> rq lock
  -> current/woken TCB.inner（再次或另一任务）

fetch / fair pick
  -> rq lock
  -> candidate TCB.inner

blocking syscall
  -> TCB.inner / wait transition
  -> wakeup / rq operation
```

`TaskManager::add()`、fair/RT 候选选择和唤醒抢占判断都可能在 rq 锁内读取调度策略、组、
状态或 vruntime。与此同时，阻塞和唤醒状态机可能先持任务状态锁再进入 rq 操作。这不仅扩大
rq 临界区，也形成了 `rq -> TCB.inner` 与 `TCB.inner -> rq` 的反向依赖。BuildStorm 的
rustc/build-script 退出、pipe/poll 唤醒和子进程回收会同时放大这两条路径。

这次现场不是宿主 OOM，也不是单纯的 LoongArch TLB 成本：平台期 QEMU 仍有高 CPU，
但 I/O 和可见进度为零；多个 vCPU 长时间停在完全相同的内核 PC，符合自旋锁 convoy。

## 如何发现

### 1. BuildStorm 的平台期先排除了普通慢速阶段

旧候选轮 `560` 在进度 150 后，从 host sample 184 到 240 没有继续推进，I/O 也停止；
诊断轮 `562` 更早在进度 20 复现同类平台期。两轮均没有 panic、OOM 或宿主 memory PSI
压力，QEMU 进程仍持续消耗 CPU。

原始资产：

```text
testsuits-final/.tmp/final-runs/20260810-loongarch-rq-lock-order-gate-560/
testsuits-final/.tmp/final-runs/20260810-loongarch-rq-lock-order-pc-capture-562/
```

### 2. vCPU PC 采样闭合到任务锁等待

轮 `562` 保留了本轮使用的 `kernel.elf`，并周期性执行 QEMU HMP
`info registers -a`。平台期内：

- 5 个 hart 反复固定在 `0x801a5b34`；
- CPU10 反复固定在 `0x802b7714`；
- PC 和寄存器跨多个采样周期保持不变，而其他 hart 仍在运行。

用该轮归档 ELF 做 `llvm-addr2line` 和反汇编后：

- `0x801a5b34` 是 `wakeup_should_preempt_task()` 读取当前任务调度类/优先级时的 ticket
  spin，多个唤醒者等待同一个 `TCB.inner`；
- `0x802b7714` 落在 `syscall_ppoll()` 内联的 `PreparedWait::new()` 状态转换路径；
- 更早的轮 `551` 还捕获过 hart 停在 `TaskManager::add()` 的 rq 内任务锁路径。

因此热点符号不只是 perf 中的相关性：PC 长时间不变、等待对象和源码锁序能够互相解释。
诊断使用归档 ELF 而不是当前重新构建的 ELF，避免地址漂移造成错误符号化。

```text
testsuits-final/.tmp/final-runs/20260810-loongarch-rq-lock-order-pc-capture-562/vcpu-pc-samples.txt
testsuits-final/.tmp/final-runs/20260810-loongarch-rq-lock-order-pc-capture-562/kernel.elf
```

### 3. perf 提供方向，PC 采样负责定因

此前带 perfmap 的受扰动窗口中，`TaskManager::add` 已达到 7.57%，调度 BTree 和原子
helper 同时较热。这说明 rq 热路径值得检查，但单个 perf 百分比不能证明死锁或锁 convoy；
最终定因依赖轮 `562` 的逐 vCPU PC、归档 ELF 和源码锁图。正式性能 gate 关闭 perfmap、
`DEBUG_PERF` 和 guest probe，避免诊断开销改变结果。

## Linux 对照

本地 Linux 参考树为 `exampleOs/linux` commit
`4549871118cf616eecdd2d939f78e3b9e1dddc48`。

- `include/linux/sched.h::struct sched_entity` 把 `run_node`、`on_rq`、deadline、vruntime
  等 rq 所需字段嵌入持久任务实体；
- `kernel/sched/fair.c::__enqueue_entity()` 和 `__dequeue_entity()` 在 rq 锁域内直接操作
  嵌入式 rb node，不为树节点分配，也不需要获取另一把宽泛任务状态锁；
- `kernel/sched/core.c::try_to_wake_up()` 用专门的 `p->pi_lock`、`p->on_rq`、
  `p->on_cpu` 和 rq 锁建立状态机顺序，而不是在 rq 锁内阻塞读取任意任务内部状态；
- picker 把 rq 树成员关系视为 runnable 的权威来源，状态发布通过明确的 acquire/release
  顺序完成。

CongCore 目前还没有嵌入式 `sched_entity`，因此本批采用可维护的过渡边界：rq 内只依赖 rq
拥有的树和原子成员标记，完整 TCB 校验放在 rq 外，精确但非关键的比较在锁竞争时退化为
rq-head 判定。

## 怎么解决

### 1. 入队只获取一次目标任务状态

`TaskManager::add()` 在进入 rq 前获取目标任务的可变状态，快照队列类型、fair group、
权重和当前 fair entity；进入 rq 后直接用这份状态完成 placement，不再从 rq 锁内重新
获取目标或当前任务的 `TCB.inner`。

物理实体插入后才以 release 顺序发布 `in_ready_queue=true`；
`ready_queue_hart` 继续作为 rq 所有权代际标记，避免并发 add/remove 丢失 Ready 任务。

### 2. rq 内只相信物理成员关系

RT/fair picker 不再在 rq 锁内读取 `task_status`。候选必须同时满足：

- 实体仍存在于当前 rq 的树或队列；
- `ready_queue_hart` 仍等于当前 hart。

`fetch()` 在 rq 锁内摘除实体并释放所有权，释放 rq 锁后才阻塞读取 `task_status` 和更新
runtime checkpoint。若发现陈旧状态就丢弃并重新挑选，不把状态锁重新带回 rq 临界区。

`remove()` 也在进入 rq 前快照 group hint 和调试字段，避免删除路径重复形成 rq 到任务锁
依赖。

### 3. 唤醒抢占使用非阻塞快照和 rq-head fallback

fair 的精确 pairwise 比较分别对 current/woken 做一次 `try_borrow_mut()` 快照，然后才读取
rq。任一任务锁正在竞争时，不等待该锁；调用者仍可直接检查被唤醒者是否已经是 rq 树中
实际的下一实体。

`fair_task_is_next_on_hart()` 用全局唯一的 task id 在 rq 树中定位 wakee，不再为读取
group id 获取 TCB 锁。调度类/RT 优先级的唤醒比较同样使用 try-lock：current 不可读时，
RT wakee 保守抢占；fair wakee 采用实际 rq-head 结果；woken 自身不可读时暂缓精确抢占。

### 4. 放弃跨架构有风险的激进 fallback

中间实验曾把“任务状态锁竞争”直接解释为“总是请求重调度”，并尝试只在
`NEED_RESCHED` false->true 时发 IPI。它消除了 LoongArch cliff，但 RISC-V BuildStorm
早期 gate 出现可疑变慢，而且改变了已有 IPI 语义。

最终实现恢复原有 IPI 发送规则，只保留锁顺序修复；fair 竞争场景改用 rq 中真实下一实体，
而不是无条件抢占。随后重新执行双架构聚焦测试和 RISC-V B-C-C-B gate。

## 对应提交

| 项目 | 值 |
| --- | --- |
| `os/` 基线 | `014eb34c7fcf9dbea96a63de4301b3432bd99cef` |
| `os/` 修复 | `270b8af` |
| 提交标题 | `sched: avoid task locks in wakeup paths` |
| 顶层集成 | 本说明文档所在提交 |

## 对因提升

### LoongArch cliff 复现与最终 gate

最终候选使用 LoongArch64、12 hart、8 GiB、相同 raw snapshot，关闭 perfmap、host perf、
`DEBUG_PERF`、guest diag 和交互探针：

| 运行 | 结果 |
| --- | --- |
| 旧轮 `560` | 到 progress=150 后长期零进度、零 I/O |
| 诊断轮 `562` | progress=20 后复现，PC 采样捕获任务锁 convoy |
| 最终轮 `575` | progress=150 时仍持续写入，随后到 157、187；到达 `strum` 后再推进 32 个事件，最终 progress=204 |

最终轮从 05:40:36 运行到 05:50:26，按预设的 `post_strum_window_complete` 正常停止，
`expect_rc=0`。旧轮的两个 cliff 均未复现；这是一轮有界 production gate，不是完整
BuildStorm 完成结果。

```text
testsuits-final/.tmp/final-runs/20260810-loongarch-wakeup-rq-next-final-gate-575/
```

### 双架构调度聚焦回归

每侧各运行 4 次 `fork_thread_group_perf_smoke` 和 `sched_yield_smp_perf_smoke`，比较修复
基线与最终候选的中位数：

| 架构/指标 | 基线中位数 | 修复中位数 | 变化 |
| --- | ---: | ---: | ---: |
| LoongArch fork | 81,031.5 us | 81,382.5 us | +0.43% |
| LoongArch yield | 206,169.0 us | 210,743.5 us | +2.22% |
| RISC-V fork | 127,026.0 us | 122,338.0 us | **-3.69%** |
| RISC-V yield | 653,003.5 us | 669,035.5 us | +2.46% |

四项都在 5% 门禁内。两架构还通过：

- `concurrent_spawn_wait_smoke`：256 children；
- `wait_wakeup_race_smoke`：256 iterations；
- `signal_frame_fault_smoke`：64 iterations；
- RISC-V 额外通过 `mq_notify_signal_smoke`；
- fork/yield 连续重复三轮，无 hang 或丢失唤醒。

```text
.tmp/sched64-runs/20260810-loongarch-rq-lock-order-focus-clean-557/
.tmp/sched64-runs/20260810-loongarch-wakeup-rq-next-final-focus-574/
.tmp/sched64-runs/20260810-riscv-rq-lock-order-baseline-recheck-569/
.tmp/sched64-runs/20260810-riscv-wakeup-no-coalesce-focus-568/
```

### RISC-V B-C-C-B BuildStorm gate

为了排除中间实验在 RISC-V 上的疑似回退，在同一当前宿主、相同镜像和 30 秒采样下按
parent -> candidate -> candidate -> parent 运行到 180 秒硬截止。四轮都在预构建阶段按计划
停止，不能当作完整 BuildStorm；该 gate 只比较同一时间预算内的进度。

| 指标（约 182 s） | parent 中位数 | candidate 中位数 | 变化 |
| --- | ---: | ---: | ---: |
| deps | 64 | 62 | -3.13% |
| output bytes | 2,346 | 2,300 | -1.96% |

两项均未超过 5% 回退门禁，且四条进度曲线总体重叠。原始顺序和结果：

```text
testsuits-final/.tmp/final-runs/20260810-riscv-rq-lock-parent-ab-b1-570/
testsuits-final/.tmp/final-runs/20260810-riscv-rq-lock-candidate-ab-c1-571/
testsuits-final/.tmp/final-runs/20260810-riscv-rq-lock-candidate-ab-c2-572/
testsuits-final/.tmp/final-runs/20260810-riscv-rq-lock-parent-ab-b2-573/
```

## 回归验证

- `cargo fmt`：通过；
- LoongArch64 softfloat `cargo check`：通过；
- RISC-V64 `cargo check`：通过；
- 两架构 release build：通过；
- `git diff --check`：通过；
- 上述双架构聚焦运行态测试：通过；
- LoongArch 最终 BuildStorm production gate：通过并越过旧 cliff；
- RISC-V B-C-C-B BuildStorm 进度 gate：候选回退小于 5%。

## 当前边界与下一步

- 本批没有宣称完整 BuildStorm 通过；完整双架构 production 仍需后续长测确认最终耗时；
- try-lock fallback 修复的是前进性和锁顺序，调度实体仍保存在宽泛的 `TCB.inner` 中；
- fair rq 仍由 `BTreeMap/BTreeSet` 保存实体，enqueue/dequeue 仍可能分配；
- Linux 式最终结构应把 fair entity 和 intrusive tree node 嵌入 TCB，使 rq 热路径
  allocation-free，并让调度字段由 rq 锁唯一拥有；
- 后续公共 scheduler 改动继续执行双架构 B-C-C-B/A-B-B-A 门禁，不能只做一边完整运行、
  另一边 `cargo check`。
