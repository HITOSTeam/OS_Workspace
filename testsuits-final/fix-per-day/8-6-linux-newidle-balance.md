# 8-6 空闲核心不再全域轮询，改用 newidle 拉取

## 问题概述

空闲 hart 在清理循环中反复扫描所有 hart 的 99 个 RT 优先级桶和全部 runqueue。低并发阶段中，"其他 CPU 正在工作"反而让每个 idle CPU 持续执行全局轮询，把调度扫描本身变成主要 CPU 开销。

Linux 的 `sched_balance_newidle()`（处理器刚进入 idle 时触发的负载均衡）会先检查 `this_rq->avg_idle`（该 CPU 历史空闲时间的指数加权平均），如果平均 idle 时间太短（代价判断），就直接跳过跨域迁移，避免拉取任务的开销超过实际空闲时长。本项目的旧实现没有这层代价判断，每次 idle 都做全量扫描。

## 背景知识

**为什么多核系统仍会出现空闲 CPU**

multicore（多核）系统里，任务多并不代表每个 CPU 始终都有任务可运行。
任务可能等待磁盘或网络 I/O，也可能主动 sleep（睡眠），还可能执行完毕并退出。
这些任务阻塞后，会暂时离开 runqueue（运行队列），对应的 CPU 就可能进入 idle（空闲）状态。
与此同时，另一个 CPU 的运行队列里可能恰好积压了 3～4 个可运行任务。
这种不均衡是任务唤醒时机、亲和性和运行时间共同造成的自然现象，并不罕见。
调度器需要把部分任务迁移到空闲 CPU，才能重新利用并行能力。

**最直接但昂贵的办法：全局扫描**

global scanning（全局扫描）是最容易想到的方案：每个空闲 CPU 查看所有其他 CPU 的队列。
如果发现某个队列有多余任务，就把一个任务拉到本地运行。
问题在于，查看队列通常需要 lock acquisition（获取锁），否则队列可能正被并发修改。
假设系统有 12 个 CPU，其中 8 个处于空闲状态，每个空闲 CPU 一轮检查 12 个队列。
那么一轮就会产生 8 × 12 = 96 次锁操作，而且很可能一次任务也没有找到。
多个 CPU 还会反复争用相同的锁和 cache line（缓存行），进一步放大开销。
最后，“寻找工作”花掉的 CPU 时间可能比真正执行工作还多。

**Linux 如何用 `avg_idle` 判断值不值得找**

Linux 在每个运行队列上记录 `this_rq->avg_idle`，表示该 CPU 每次连续空闲时长的历史平均。
它采用 exponentially weighted average（指数加权平均），让近期样本比很久以前的样本权重更高。
如果 `avg_idle` 很短，说明这个 CPU 通常很快就会在本地收到新任务。
此时做昂贵的 cross-domain balancing（跨调度域负载均衡）往往得不偿失。
因为扫描还没完成，本地任务可能已经到达，而扫描期间消耗的锁和 CPU 时间无法收回。
所以 Linux 会比较预期空闲时间与均衡成本，太短时直接跳过昂贵的 newidle 拉取。
只有历史空闲时间足够长、预计能摊平扫描成本时，才继续寻找可迁移任务。

**负载均衡发生在什么时机**

Linux 主要在三个时机调整 CPU 之间的负载。
第一是 periodic rebalance（周期性再均衡），每隔几毫秒检查一次长期不均衡。
第二是 newidle balance（新空闲均衡），CPU 刚刚发现本地没有任务时尝试拉取工作。
第三是 fork/exec placement（创建或执行程序时的放置），在任务出现或更换程序时选择合适 CPU。
三种时机能容忍的成本不同：周期检查要控制频率，任务放置要避免拖慢创建路径。
就触发方式而言，newidle 是其中最便宜的一次性机会，因为每次进入 idle 只尝试一次。
newidle 路径只在一次进入 idle 时触发一次，因此适合做一次受控、有限的拉取。
它不应该在 idle 循环里无休止地重复全局扫描。

**CongCore 旧实现的问题**

旧 idle 清理循环反复调用 `has_ready_rt_any_at_or_above()` 和 `has_ready_tasks()`。
前一个函数会跨所有 hart 扫描 99 个 RT（实时调度）优先级桶。
后一个函数会获取每个 runqueue 的锁，再检查是否存在可运行任务。
在低并发阶段，真正忙碌的 hart 可能只有一两个，其他 idle hart 却同时做相同搜索。
这些搜索又发生在清理循环中，不是每次进入 idle 只做一次，因此成本会不断累积。
结果是空闲核心消耗大量宿主 CPU，忙碌核心真正执行用户工作的占比反而下降。

**`perf` 和 `-perfmap` 为什么能看出这个问题**

`perf` 是 sampling profiler（采样分析器），它不记录每一次函数调用。
它利用 PMU（Performance Monitoring Unit，性能监控单元）计数器溢出或定时器中断抽样。
中断处理程序会记录当时的 PC（Program Counter，程序计数器）和 call stack（调用栈）。
随后 `perf` 读取 symbol table（符号表），把采到的机器地址解析成函数名。
QEMU 的 JIT（即时编译）翻译块在运行时才生成，普通静态符号表不知道这些地址的名字。
`/tmp/perf-<pid>.map` 提供“地址范围 -> 符号名”的映射，让 guest 内核热点能够被识别。
没有这份映射，报告或 flame graph（火焰图）里通常只会出现难以解释的十六进制地址。
采样结果表示 CPU 时间落在哪些路径上的统计分布，不等于函数的精确调用次数。
因此这里看重的是热点占比和修复前后分布变化，而不是把 sample 数当成绝对计数。

## 如何发现

host 资源日志显示 QEMU RSS、内存、swap 与块设备均正常，无法单靠日志区分真实编译和调度扫描。按照前文所述的 `perf` 统计采样与 `-perfmap` 运行时符号映射原理，启用 QEMU `-perfmap` 后，固定样本约四成落在 `has_ready_rt_count()`、`has_ready_rt_any_at_or_above()` 与 RT 带宽刷新；这里的占比用于定位 CPU 时间分布，不是函数的精确调用次数。

```sh
QEMU_EXTRA_ARGS=-perfmap ARCH=loongarch64 IMAGE_MODE=snapshot \
  testsuits-final/run.sh shell
perf record -F 99 -e cycles:u -g -p <qemu-pid> -o perf.data -- sleep 15
```

证据目录：

```text
testsuits-final/.tmp/final-runs/20260806-buildstorm-futex-perfmap-diagnose-5/
testsuits-final/.tmp/final-runs/20260806-buildstorm-newidle-perfmap-after-1/
testsuits-final/.tmp/final-runs/20260806-minibuild-newidle-watchdog-1/
```

## 怎么解决

**每 hart O(1) 计数**：为每个 hart 维护一个 RT 任务总数的原子变量 `READY_RT_TOTAL_COUNTS[hart]`，与实际 RT 入队/出队同步增减。idle 存在性判断只做一次 acquire load，不再扫 99 个优先级桶。

**本地 pick 优先**：`fetch_task()` 先从本地 RT / fair 队列 pick。只有本地实时任务和公平任务选择都失败时，才扫描在线 donor 的轻量负载计数。

**newidle fair pull**：候选 donor 至少需要两个 runnable 单位（`queued_fair + current > 1`），在 donor runqueue 锁内弹出一个已经排队的 fair task。只迁移已排队 fair task，不碰正在运行的 task 或 RT task，donor 至少保留一个 runnable，单次最多拉一个，严格检查 CPU affinity。

Linux `sched_balance_newidle()` 还会根据 sched_domain 层级（LLC → NUMA → 全系统）逐级查找 busiest group，并用 `can_migrate_task()` 检查 affinity 和 cache-hotness。CongCore 没有 sched-domain / PELT / cache-hotness 模型，因此实现的是最小可验证子集：本地计数、保留一个 runnable、affinity、非 running、单任务拉取。复杂的周期性负载均衡留待实际证据需要时再加入。

## 对应提交

- 状态：待提交，当前实现仍位于未提交工作树。
- 基线：顶层 `21332ba37bf1ba0efe8229e7f80eeffa3b99a239`；`os/` `b0185b3a4522c0ffc52599d73bd17b3d52320815`。
- 建议提交主题：`sched: add local runnable fast path and newidle pull`。

## 对比提升

perf 中旧全局扫描热点从报告消失（`has_ready_rt_count` 和 `has_ready_rt_any_at_or_above` 不再出现在 0.1% 以上的 flat report 中）。相近 guest uptime 的 minibuild 文件数 `17 -> 25`（+47.06%），前四次探针中位数 `594.5 -> 457.5 ms`（-23.04%）。

这是带 `-perfmap` 的定位性 A/B，不等价于正式 BuildStorm 完成时间。优化后文件数达到 25 后连续三个周期未变化，完成固定 perf 样本后已主动终止 VM。完整 BuildStorm 未运行，不宣称已通过。

以下是 AI 的具体分析，作为存档。

---

## 历史分析背景

这个问题是调度器 idle 路径的设计缺陷：idle 清理不加区分地扫描所有 hart 的所有 RT 优先级桶，在低并发阶段把 CPU 时间花在"寻找工作"而不是"做工作"上。以下保留完整的 perf 对照数据、per-hart 计数实现细节、LoongArch syscall restart 补齐、回归测试和性能 A/B。

## 1. 结论

本批次确认 BuildStorm 的一个主要 CPU 瓶颈位于调度器 idle 路径，而不是宿主机内存、
swap 或块设备资源耗尽。12 个 vCPU 中只有少量 CPU 有工作时，每个空闲 hart 都会在
清理循环中反复执行：

```text
has_ready_rt_any_at_or_above(RT_PRIO_MIN) || has_ready_tasks()
```

前者扫描所有 hart 的 99 个 RT 优先级计数并刷新带宽，后者锁住并扫描全部 runqueue。
这使"其他 CPU 正在运行任务"本身变成每个 idle CPU 的全局轮询工作。

参考本地 Linux `kernel/sched/fair.c` 的 `pick_task_fair()`、
`sched_balance_newidle()`、`can_migrate_task()` 和 `detach_tasks()`，本次改为：

- idle 清理只读取当前 hart 的 O(1) runnable 计数，等价于本地 `rq->nr_running` 快路径；
- 为每 hart RT 优先级桶维护一个总数，存在性判断不再扫描 99 个桶；
- 本地 pick 确认无任务时才做一次 newidle 风格的 fair 拉取；
- 只迁移已经排队的 fair task，绝不迁移正在运行的 task 或 RT task；
- donor 必须保留至少一个 runnable，单次最多拉一个 task，并严格检查 CPU affinity；
- 迁移后重新经过目标 runqueue 的 EEVDF placement/pick，不绕过调度类规则。

相同 LoongArch64、12 vCPU、8 GiB、snapshot、QEMU `-perfmap`、固定第 4 个 15 秒
采样的 A/B 中，修复前约四成 samples 落在全局 RT runnable 扫描及带宽刷新；修复后
`has_ready_rt_count` 和 `has_ready_rt_any_at_or_above` 从报告中消失。相同 guest uptime
约 100 秒时，真实 minibuild target 文件从 17 增加到 25；前四次响应探针的中位数从
594.5 ms 降到 457.5 ms，下降 **23.04%**。

这次 `perfmap` A/B 只用于定位和证明热点移除，不作为正式 BuildStorm 完成时间：
`-perfmap` 会显著放大 QEMU TCG 开销。优化后文件数达到 25 后连续三个周期未变化，
完成固定 perf 样本后已主动终止 VM，没有无限等待。本批次尚未宣称完整 BuildStorm
或正式 judge 通过。

## 2. 版本与环境

| 资产 | 值 |
| --- | --- |
| 顶层分支 / 基线 | `dev_final` / `21332ba37bf1ba0efe8229e7f80eeffa3b99a239` |
| `os/` 基线 | `b0185b3a4522c0ffc52599d73bd17b3d52320815` |
| final test source | `final-2026` / `b5ec6ef8497e1818cbdec3b54bb722f036e57972` |
| 本地 Linux 参考树 | `exampleOs/linux` / `4549871118cf616eecdd2d939f78e3b9e1dddc48` |
| QEMU / perf | 11.0.3 / 7.1.6 |
| 架构 | LoongArch64，12 vCPU，8 GiB |
| 镜像模式 | snapshot |
| LoongArch 镜像 SHA-256 | `2ad9d955684297abe9db48d94f1b7fcc488268fc8f481408c55b1ec27f520c6a` |

`testsuits-final/run.sh` 在 profile 启动前重新校验了 14 GiB 基准镜像的 SHA-256。
QEMU 每 2 秒记录 RSS、线程、CPU ticks、I/O、host `MemAvailable` 和 `SwapFree`；guest
每 60 秒执行一次有 20 秒硬上限的进度探针。

## 3. 根因与 perf 证据

### 3.1 为什么日志只能排除资源问题

卡慢现场中 QEMU CPU 时间持续增加，RSS 稳定在约 1.3 GiB，host `MemAvailable`
保持约 25 GiB，swap 没有持续下降，guest 探针仍能在一秒内返回。因此可以排除宿主
内存耗尽和完全死锁，但不能仅凭日志判断 CPU 在运行 rustc、QEMU TCG，还是内核
调度扫描。

`-perfmap` 配合准确的 QEMU PID 将 guest JIT 地址解析为 Rust 符号后，修复前固定样本：

```text
testsuits-final/.tmp/final-runs/
  20260806-buildstorm-futex-perfmap-diagnose-5/
    perf.data
    perf-194.map
```

共有约 2.4K samples，lost samples 为 0。flat report 中主要调度条目为：

| 符号组 | 主要单项 overhead | 合计判断 |
| --- | --- | ---: |
| `has_ready_rt_count` | 7.53%、7.17%、5.08% | 约 20% |
| `has_ready_rt_any_at_or_above` | 2.39%、2.34%、2.08%、1.89% 等 | 约 16% |
| `refresh_rt_bandwidth` | 2.28%、0.60%、0.49% 等 | 约 4% |

同名函数会因 JIT translation block 地址拆成多个 report 条目，不能只看第一行。三组
合计约四成，足以解释单线程/低并行阶段中 idle hart 反而消耗大量宿主 CPU。

### 3.2 修复后 perf

相同参数的固定样本保存在：

```text
testsuits-final/.tmp/final-runs/
  20260806-buildstorm-newidle-perfmap-after-1/
    perf.data
    perf-196.map
```

共有 2157 samples，lost samples 为 0。`has_ready_rt_count` 与
`has_ready_rt_any_at_or_above` 不再出现在 `--percent-limit 0.10` 的 flat report 中；
热点转移到真实的内存释放和地址翻译工作：

| 符号 | overhead |
| --- | ---: |
| QEMU `helper_lookup_tb_ptr` | 21.65% |
| guest buddy allocator `Heap::dealloc` | 8.58% |
| QEMU `tlb_set_page_full` | 5.77% |
| `cpu_atomic_fetch_addq_le_mmu` | 2.28% |
| fair runqueue 内部条目 | 单项 1.98% 或更低 |
| `refresh_rt_bandwidth` | 单项 0.66% 或更低 |

说明本次没有把成本改名或隐藏；被消除的全局扫描不再主导，后续应针对正常模式的新
阶段重新 profile，而不是继续围绕已经消失的旧热点调参。

## 4. Linux 参考语义

本地 Linux 源码给出的边界为：

| Linux 位置 | 语义 | 本次实现 |
| --- | --- | --- |
| `kernel/sched/fair.c:9297` | fair pick 即将返回 idle 时调用 `sched_balance_newidle()` | 本地 fetch 失败后才尝试 pull |
| `kernel/sched/fair.c:13181` | newly-idle CPU 从其他 CPU 拉 fair 工作 | 只从在线 donor 拉一个 queued fair task |
| `kernel/sched/fair.c:9740` | `can_migrate_task()` 检查 affinity，拒绝 running task | 检查 allowed mask；只从队列 pop，不接触 current |
| `kernel/sched/fair.c:9917-9930` | source `nr_running <= 1` 时停止，避免把 donor 抽空 | `queued_fair + current > 1` 才是候选 donor |

当前内核没有 Linux 的 sched domain、PELT、cache-hotness 和 idle-cost 反馈。本次没有
伪造一套这些结构，而是实现最小可验证子集：本地计数、保留一个 runnable、affinity、
非 running、单任务拉取。复杂的周期性负载均衡留待实际证据需要时再加入。

## 5. 实现细节

### 5.1 每 hart RT 总数

`READY_RT_COUNTS[hart][priority]` 仍是优先级比较的权威数据；新增的
`READY_RT_TOTAL_COUNTS[hart]` 与每次物理 RT enqueue/dequeue 同步增减。idle presence
检查现在只做一个 acquire load，不再把 99 个 priority bucket 扫一遍。

fair 类继续使用已有 `READY_FAIR_COUNTS`。`has_ready_tasks_on_hart()` 只组合这两个本地
计数；保留的全局 `has_ready_tasks()` 也只扫描固定数量的 per-hart atomic，不再获取
全部 runqueue lock。

### 5.2 newidle fair pull

`fetch_task()` 先按原顺序从本地 RT / fair 队列 pick。只有返回 `None` 时才扫描在线
donor 的轻量负载计数。候选 donor 至少需要两个 runnable 单位，随后在 donor rq lock
下弹出一个 fair task。

被弹出的任务若不允许在 idle hart 运行，会放回一个 allowed hart，必要时发送 IPI；
不会为了负载均衡绕过 affinity。允许迁移时先以 `EnqueueKind::Requeue` 加入目标队列，
再调用目标 `fetch()`，因此 RT 优先级和目标 EEVDF placement 仍然生效。

### 5.3 LoongArch syscall restart 可观测性与语义补齐

诊断过程中发现 RISC-V syscall 入口会保存 `last_syscall_id/args`，LoongArch 没有。
signal delivery 在 `ERESTARTSYS + SA_RESTART` 时依赖这些字段恢复 syscall 编号、参数并
回退 PC。LoongArch 现在与 RISC-V 同样在调用 syscall dispatcher 前保存六个参数。
这既使 watchdog 能给出准确现场，也补齐了通用 syscall restart 语义，不是
BuildStorm 路径硬编码。

watchdog 增加 exited-child、wait queue、pending/mask 和 last-syscall 字段，便于后续
区分锁等待、wait/reap 与信号重启问题。正式代码中的 `DEBUG_WATCHDOG` 已恢复为
`false`，正常运行不产生周期日志。

## 6. 同场景性能证明

两次 profile 的唯一调度器差异是本批次实现，参数均为：

```text
ARCH=loongarch64
IMAGE_MODE=snapshot
MEM=8G
SMP=12
QEMU_EXTRA_ARGS=-perfmap
guest probe interval=60s
guest probe timeout=20s
perf sample=index 4, 99Hz, 15s
```

进度与响应结果：

| 指标 | 修复前 | 修复后 | 变化 |
| --- | ---: | ---: | ---: |
| guest uptime 约 100 秒的 minibuild files | 17 | 25 | **+47.06%** |
| 探针 1 | 849 ms | 766 ms | -9.78% |
| 探针 2 | 568 ms | 449 ms | -20.95% |
| 探针 3 | 621 ms | 423 ms | -31.88% |
| 探针 4 | 540 ms | 466 ms | -13.70% |
| 前四次中位数 | 594.5 ms | 457.5 ms | **-23.04%** |
| QEMU peak RSS | 1,359,304 KiB | 1,348,608 KiB | 无增长 |
| host 最低 MemAvailable | 25,870,700 KiB | 25,968,360 KiB | 无资源压力 |

修复前文件数到 guest uptime 417 秒仍为 17；修复后约 100 秒已到 25，但之后三个探针
仍为 25。因为 `-perfmap` 会让正常不足 15 秒的 minibuild 放大到数分钟，且本次目的
是取得同位置热点证据，样本完成后主动停止，没有把被 instrumentation 扰动的绝对
时间包装成正式加速比。

## 7. 正确性回归

LoongArch64、12 vCPU、8 GiB、snapshot 下运行四项聚焦回归，每项 guest 内部硬上限
180 秒，driver 也检查 marker、退出码和 shell 返回：

| 测试 | 覆盖 | 结果 | host elapsed |
| --- | --- | --- | ---: |
| `concurrent_spawn_wait_smoke` | 8 worker × 32 次 fork/wait | 256 children PASS | 279 ms |
| `wait_wakeup_race_smoke` | pidfd waitid + wait4 竞态 | 256 iterations PASS | 368 ms |
| `fork_thread_group_perf_smoke` | 16-thread mm 的 128 次 fork/wait | PASS，guest 182016 μs | 280 ms |
| `tlb_shootdown_smp_smoke` | parent CPU0 / child CPU1 affinity 与跨核 TLB | PASS | 68 ms |

日志与资源记录：

```text
testsuits-final/.tmp/final-runs/
  20260806-scheduler-regressions-newidle-2/
    serial.log
    host-metrics.log
    test-timings.csv
```

该短跑 QEMU 采样 RSS 为 665160 KiB，host `MemAvailable` 为 26872976 KiB。所有用例
返回后 QEMU 立即退出，无残留实例。

正常、不带 `-perfmap` 的真实 minibuild 也完成了 cargo new、cargo build、直接执行和
command substitution，日志为：

```text
testsuits-final/.tmp/final-runs/20260806-minibuild-newidle-watchdog-1/
```

构建在第一个 15 秒探针前完成，与此前正常模式基线一致，没有引入可见退化。

## 8. 静态验证

```zsh
cd os
TMPDIR=$PWD/../.tmp ARCH=loongarch64 cargo check --manifest-path Cargo.toml \
    --target loongarch64-unknown-none-softfloat
TMPDIR=$PWD/../.tmp ARCH=riscv64 cargo check --manifest-path Cargo.toml \
    --target riscv64gc-unknown-none-elf
```

两项均成功，仅有仓库既有 warning。聚焦 QEMU 回归实际使用 release kernel；临时
watchdog 开关在最终检查前恢复为 `false`。

## 9. 测量边界与下一步

- 本批次证明的是调度器热点消失、同场景早期进度增加和并发语义回归通过；不声称
  BuildStorm 已完成。
- perfmap map 必须与产生 `perf.data` 的准确 QEMU PID 配对。两组 map/data 已保存在
  各自 run 目录，`/tmp/perf-*.map` 临时副本已清理。
- 下一轮应在不带 `-perfmap`、watchdog 关闭的正式受监控 BuildStorm 中确认是否越过
  原来的 minibuild/command-substitution 阶段。若再次无进度，先用 20 秒探针停止，
  再围绕新的 `Heap::dealloc`、TLB 或实际阻塞现场采样，不能继续猜测旧 RT 热点。
