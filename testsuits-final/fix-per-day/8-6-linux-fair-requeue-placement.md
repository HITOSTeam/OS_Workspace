# 8-6 Linux fair requeue / wakeup CPU placement

## 问题概述

普通 fair wakeup、tick 和 `sched_yield` 后的 requeue 都会遍历 12 个 hart，并获取远端
runqueue/Processor 锁。任务甚至会在每个时间片结束时迁移，放大锁竞争、QEMU TB 查询
以及 cache/TLB 冷失效。

## 如何发现

无 `-perfmap` 的 BuildStorm `perf record` 排除了 OOM 和 I/O 停滞，并把稳定 guest PC
解析到 `pick_least_loaded_hart_from_mask()`、`resolve_enqueue_hart()` 与
`TaskManager::add()`。设计参考 Linux `kernel/sched/fair.c::select_task_rq_fair()`、
普通 `WF_TTWU` fast path 以及 `sched_balance_newidle()`。

```sh
perf record -F 99 -e cycles:u -g -p <qemu-pid> -o perf.data -- sleep 15
# guest
/user/sched_yield_smp_perf_smoke.bin
```

严格微基准数据位于：

```text
.tmp/final-runs/20260806-sched-yield-baseline-scan-48/results.csv
.tmp/final-runs/20260806-sched-yield-linux-affinity-49/results.csv
```

## 怎么解决

新任务继续轮询分散；普通 wakeup 和 requeue 保留原 CPU，只有 affinity 不允许时才
回退；真正进入 idle pick 时才由 newidle 路径拉取排队 fair task。曾尝试全局 idle-hart
位图，但 BuildStorm 明显退化后已完整回滚。长期方案应在有证据需要时引入 Linux 式
sched-domain/PELT，而不是恢复每次唤醒的全域扫描。

代码不再让 `EnqueueKind::Requeue` 调用全域
`pick_least_loaded_hart_from_mask()`；只有新任务选择初始核心，已有任务保留
`prev_cpu`，处理器亲和性失效时才选择其他在线核心。

## 对应提交

- 状态：待提交，当前实现仍位于未提交工作树。
- 基线：顶层 `21332ba37bf1ba0efe8229e7f80eeffa3b99a239`；`os/`
  `b0185b3a4522c0ffc52599d73bd17b3d52320815`。
- 建议提交主题：`sched: preserve fair task CPU on requeue`。

## 对比提升

49,152 次 `sched_yield` requeue 的 guest 中位数由 `575552 us` 降至 `234696 us`
（-59.2%，吞吐约 2.45x），host 中位数由 `633 ms` 降至 `307 ms`（-51.5%）。
BuildStorm 短窗口仅相差约 3%，因此不把它作为端到端收益或完整通过证据。

---

## Problem

BuildStorm 的无 `-perfmap` 采样显示系统不是 OOM、I/O 阻塞或持续死锁：12 个
TCG 线程持续工作，QEMU 平均使用约 10.93 个 host core，最低
`MemAvailable` 约 19.9 GiB。15 秒 `perf record -F 99 -e cycles:u -g` 获得
17,608 个样本且丢样为 0，其中 `helper_lookup_tb_ptr` 约占 15.65%。调用链中的
热点 guest PC 落在：

- `pick_least_loaded_hart_from_mask()`；
- `resolve_enqueue_hart()`；
- `TaskManager::add()`。

原实现对每个普通 fair wakeup，以及 tick / `sched_yield` 后的每次
`EnqueueKind::Requeue`，都遍历 12 个 hart。每次遍历会获取每个远端 runqueue
锁，并通过 `current_task_on_hart()` 尝试获取每个远端 Processor 锁。更严重的是，
它允许任务在每个时间片结束时迁移。这既放大锁和 QEMU TB 查询开销，也破坏热
cache / TLB 亲和性。

## Linux reference

参考 `exampleOs/linux/kernel/sched/fair.c::select_task_rq_fair()`：

- CPU 选择从 `prev_cpu` 开始；
- 注释明确说明普通 `WF_TTWU` 唤醒通常走 fast path，完整 sched-domain
  load-balance 通常只由 `WF_EXEC` / `WF_FORK` 触发；
- 时间片结束后的 requeue 不重新执行跨 CPU placement，迁移属于明确的
  balancing 路径；
- CPU 真正进入 idle pick 时，再由 `sched_balance_newidle()` 拉取排队任务。

CongCore 还没有 Linux 的 sched-domain / PELT 模型，因此没有伪造一套简化的
全域扫描。新任务继续使用现有 `select_hart_for_new_task()` 轮询分散；普通唤醒和
requeue 保留 task 的原 CPU；affinity 禁止原 CPU 时才回退到当前或其他允许的
在线 hart。上一批已经实现的 `pull_fair_task_for_idle()` 负责真正 idle 时的迁移。

## Change

- 删除 fair wakeup / requeue 共用的 `pick_least_loaded_hart_from_mask()`；
- `Initial` 保留创建阶段已选择的轮询 CPU；
- `Requeue` 保留原 CPU，不再在每个 tick / yield 后迁移；
- 普通 wakeup 保留 `prev_cpu`，显式 `WF_SYNC` 路径仍可偏向 waker；
- 所有路径仍重新校验在线 mask 与 task affinity。

曾试验过一个无锁全局 idle-hart 位图，并在普通唤醒时选择任意 idle sibling。
同一 BuildStorm 阶段第 4 个探针只有 38 行，而原基线为 46 行，确认退化后提前
停止并完整回退。没有把不完整的 idle-domain 模型留在正式实现中。

## Focused performance proof

新增 `sched_yield_smp_perf_smoke`：12 个共享地址空间的 worker 同步开始，每个
执行 4,096 次 `sched_yield`，测量 49,152 次 worker requeue 完成时间。测试直接
覆盖 perf 命中的路径，不依赖 BuildStorm crate 完成顺序。

两份内核除本次 placement 修复外一致，均以 LoongArch 12 hart、8 GiB、相同只读
root image 和相同 `/user` image 启动；各运行 7 轮，全部 rc=0：

| implementation | guest elapsed (us), sorted | median |
| --- | --- | ---: |
| old 12-rq scan | 533112, 547652, 554428, 575552, 579378, 594487, 604401 | 575552 |
| Linux requeue ownership | 224902, 229563, 232052, 234696, 246452, 260571, 264221 | 234696 |

中位耗时下降 **59.2%**，等价吞吐约 **2.45x**。host wall-time 中位数也从
633 ms 降到 307 ms（下降 51.5%）。原始证据：

- `.tmp/final-runs/20260806-sched-yield-baseline-scan-48/results.csv`
- `.tmp/final-runs/20260806-sched-yield-linux-affinity-49/results.csv`

BuildStorm 的短窗口仅用于防回退观察：同阶段旧版最终探针为 63 行，修复版为
61 行，差异约 3%，不足以证明端到端吞吐变化，因此没有把它当作性能结论。

## Regression

`20260806-scheduler-regressions-linux-affinity-50` 在 LoongArch 12 hart 上全部通过：

- `signal_frame_fault_smoke`：64 iterations；
- `socketpair_exit_eof_smoke`：stream / seqpacket 各 16 iterations；
- `concurrent_spawn_wait_smoke`：256 children；
- `wait_wakeup_race_smoke`：256 iterations；
- `fork_thread_group_perf_smoke`：128 forks，140025 us；
- `tlb_shootdown_smp_smoke`：4 MiB 跨核权限切换通过。

LoongArch release kernel 构建通过；RISC-V
`cargo check --target riscv64gc-unknown-none-elf` 通过；输出仅包含仓库原有
warnings。
