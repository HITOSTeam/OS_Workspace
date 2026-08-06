# 8-6 Linux 式 newidle 平衡与 idle runnable 快路径

## 问题概述

空闲 hart 在清理循环中反复扫描所有 hart 的 99 个 RT 优先级桶和全部 runqueue；低并发
阶段中，“其他 CPU 正在工作”反而让每个 idle CPU 持续执行全局轮询。

## 如何发现

资源日志显示 QEMU RSS、host memory、swap 与块设备均正常，无法单靠日志区分真实编译
和调度扫描。启用 QEMU `-perfmap` 后，固定样本约四成落在
`has_ready_rt_count()`、`has_ready_rt_any_at_or_above()` 与 RT 带宽刷新。Linux 对照
为 `pick_task_fair()`、`sched_balance_newidle()`、`can_migrate_task()` 和
`detach_tasks()`。

```text
testsuits-final/.tmp/final-runs/20260806-buildstorm-futex-perfmap-diagnose-5/
testsuits-final/.tmp/final-runs/20260806-buildstorm-newidle-perfmap-after-1/
testsuits-final/.tmp/final-runs/20260806-minibuild-newidle-watchdog-1/
```

```sh
QEMU_EXTRA_ARGS=-perfmap ARCH=loongarch64 IMAGE_MODE=snapshot \
  testsuits-final/run.sh shell
perf record -F 99 -e cycles:u -g -p <qemu-pid> -o perf.data -- sleep 15
```

## 怎么解决

为每 hart 维护 O(1) runnable/RT 总数；本地 pick 确认无任务后才执行一次 newidle fair
拉取。只迁移已排队 fair task，检查 affinity，不迁移 running/RT task，donor 至少保留
一个 runnable，单次最多拉一个。更好的长期方案是根据真实 workload 逐步补齐 Linux
sched-domain、PELT 与 cache-hotness，而不是伪造完整模型。

实现把 `READY_RT_TOTAL_COUNTS[hart]` 与实时任务实际入队、出队同步增减；本地
runnable 判断只读当前 hart 的原子总数。只有本地实时任务和公平任务选择均失败时才
扫描 donor，并在 donor runqueue 锁内弹出一个已经排队的公平任务。

## 对应提交

- 状态：待提交，当前实现仍位于未提交工作树。
- 基线：顶层 `21332ba37bf1ba0efe8229e7f80eeffa3b99a239`；`os/`
  `b0185b3a4522c0ffc52599d73bd17b3d52320815`。
- 建议提交主题：`sched: add local runnable fast path and newidle pull`。

## 对比提升

perf 中旧全局扫描热点从报告消失；相近 guest uptime 的 minibuild 文件数
`17 -> 25`（+47.06%），前四次探针中位数 `594.5 -> 457.5 ms`（-23.04%）。
这是带 `-perfmap` 的定位性 A/B，不等价于正式 BuildStorm 完成时间。

---

## 1. 结论

本批次确认 BuildStorm 的一个主要 CPU 瓶颈位于调度器 idle 路径，而不是宿主机内存、
swap 或块设备资源耗尽。12 个 vCPU 中只有少量 CPU 有工作时，每个空闲 hart 都会在
清理循环中反复执行：

```text
has_ready_rt_any_at_or_above(RT_PRIO_MIN) || has_ready_tasks()
```

前者扫描所有 hart 的 99 个 RT 优先级计数并刷新带宽，后者锁住并扫描全部 runqueue。
这使“其他 CPU 正在运行任务”本身变成每个 idle CPU 的全局轮询工作。

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
