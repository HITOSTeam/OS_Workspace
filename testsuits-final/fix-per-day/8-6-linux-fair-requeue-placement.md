# 8-6 CFS 唤醒和 requeue 不再全域扫描，保留原 CPU

## 问题概述

普通 fair wakeup（CFS 任务被唤醒重新入队）、时间片结束和 `sched_yield` 之后的
requeue 都会遍历全部 12 个 hart，并逐个获取远端 runqueue 锁来比较负载。任务甚至
会在每个时间片结束时迁移到"最空闲"的核心，放大锁竞争、QEMU 翻译块查询和
cache/TLB 冷失效。

```text
任务 A 时间片到期
        │
        ▼
  遍历 12 个 hart，逐个加锁比较负载
        │
        ▼
  选出"最空闲"核心，迁移过去
        │
        ▼
  下个时间片到期……再来一轮
```

根因：旧实现把"选择最空闲核心"当作每次 enqueue 的默认路径，包括 tick/yield
后的 requeue，在 12 核场景下每个时间片都做一次 O(n) 锁遍历。

## 背景知识

**从时间片轮转到 CFS**。课上讲的最简单调度是时间片轮转（Round-Robin）：每个
进程跑固定时间，到期就排到队尾，下一个接上。问题是，所有进程不管做什么都平分
CPU 时间——一个后台日志进程和一个交互式编辑器拿到的时间一模一样，交互体验很差。

Linux 2.6.23 引入了 CFS（Completely Fair Scheduler，完全公平调度器），思路是：
不分配固定时间片，而是追踪每个任务"已经用了多少 CPU 时间"，永远让用得最少的
任务先跑。这个"已经用了多少"就是 vruntime（虚拟运行时间）。所有可运行任务按
vruntime 从小到大排在一棵红黑树里，调度器每次挑最左边（最小 vruntime）的跑。
任务一直跑，vruntime 就一直涨，最终它会超过别人、被排到右边去，这就是"公平"
的含义。

```text
红黑树（按 vruntime 排序）

    [vruntime=80]
   /             \
[vruntime=50]   [vruntime=120]
      ↑
   下一个被调度
```

**vruntime 和权重**。不同任务可以有不同优先级（nice 值），高优先级任务的
vruntime 涨得慢——跑同样 1 ms，nice -5 的任务 vruntime 只涨 0.3 ms，nice 0 涨
1 ms。效果是高优先级任务排在左边更久，拿到更多实际 CPU 时间，但并不饿死低优先
级任务。

**睡眠任务唤醒时的 vruntime 问题**。假设任务 B 睡了 10 秒（阻塞在 I/O 或
wait），期间其他任务的 vruntime 都涨了几十 ms。B 醒来时它的 vruntime 还停在
睡前的值，比当前最小 vruntime 小很多。如果直接用这个旧值入队，B 会排在红黑树
最左边，持续抢占所有人，直到 vruntime 追上来——对一个睡了很久的任务，这个
"追赶期"可能长达数十 ms，其他任务全部饿着。

```text
当前最小 vruntime = 1000
任务 B 的旧 vruntime = 200   ← 差 800！

如果直接入队：B 要连续跑 800 个虚拟单位才让位
```

**place_entity 的修正**。Linux 用 `place_entity()` 解决这个问题：唤醒任务入队
时，把它的 vruntime 至少拉到 `cfs_rq->min_vruntime - sched_latency/2`。这样
它还是比当前最左边的任务靠左一点（获得一小段"补偿"来鼓励交互任务），但不会
离得太远。效果是：睡过头的任务醒来能很快得到调度，但不会霸占 CPU。

```text
修正后：
  B 的 vruntime = max(旧值, min_vruntime - latency/2)
                = max(200, 1000 - 12/2)
                = 994

  B 排在树上稍靠左，很快被选中跑一会，但不会连续独占
```

**CPU 选择和调度域**。place_entity 管的是"任务入队后排第几"，但还有一个
问题："任务入到哪个 CPU 的队列？"Linux 的 `select_task_rq_fair()` 根据唤醒
类型做不同处理：

- `WF_FORK` / `WF_EXEC`（新建/exec）：做完整 sched-domain（调度域，按
  NUMA/LLC/核心层级划分的 CPU 拓扑）遍历，把新任务分散到空闲核心；
- `WF_TTWU`（普通唤醒）：走快路径，倾向保留任务之前跑过的 CPU（`prev_cpu`），
  避免把热缓存全扔掉。

道理很简单：新进程还没有缓存亲和性，分散有利；睡眠后醒来的进程刚在某个核上跑
过，L1/L2 里可能还有它的数据，迁走得不偿失。时间片到期后的 requeue 更不应该
迁移——它只是"该让别人跑一会了"，不意味着应该换核心。真正的跨核迁移由专门的
负载均衡器（周期性或 newidle 触发）完成，不应该搭在每次 enqueue 上。

**CongCore 旧实现的问题**。旧代码不区分以上几种情况，每次 enqueue（包括 tick
后 requeue）都调用 `pick_least_loaded_hart_from_mask()` 遍历所有核心、逐个
加锁查看负载，然后迁移。12 核场景下每个时间片都做一次 O(12) 锁遍历，相当于
把调度器的"分散新任务"逻辑误用到了"普通唤醒"和"时间片到期"上。

## 如何发现

用 perf 对 QEMU 进程采样 15 秒。perf 靠定时中断或 PMU（Performance Monitoring
Unit，处理器内置的硬件计数器单元）计数器溢出，在中断里记录当前 PC（程序计数器）
和调用栈，再靠符号表把地址还原成函数名。采样只给统计分布，不给精确调用次数，
所以能指出热点但不能证明因果。

没有使用 `-perfmap` 的 BuildStorm 采样排除了 OOM 和 I/O 停滞，17,608 个样本中
稳定的 guest PC 落在 `pick_least_loaded_hart_from_mask()`、
`resolve_enqueue_hart()` 和 `TaskManager::add()`。

```sh
perf record -F 99 -e cycles:u -g -p <qemu-pid> -o perf.data -- sleep 15
# guest
/user/sched_yield_smp_perf_smoke.bin
```

微基准数据：

```text
.tmp/final-runs/20260806-sched-yield-baseline-scan-48/results.csv
.tmp/final-runs/20260806-sched-yield-linux-affinity-49/results.csv
```

## 怎么解决

**新任务**：继续用轮询分散到各核心（对应 Linux 的 `WF_FORK`/`WF_EXEC` 完整
遍历）。

**普通唤醒和 requeue**：保留任务原来所在的 CPU（`prev_cpu`），只有 affinity
mask 不允许时才回退到当前 CPU 或其他在线核心。代码不再让
`EnqueueKind::Requeue` 调用全域 `pick_least_loaded_hart_from_mask()`。

**idle pick**：由已实现的 newidle 路径负责——只有处理器真正没有可运行任务时才
拉取其他队列的 fair task。

**回退的尝试**：曾试验一个无锁全局 idle-hart 位图，让普通唤醒选择任意空闲
兄弟核心。同一 BuildStorm 阶段第 4 个探针只有 38 行进度，基线为 46 行，确认
退化后完整回滚。

Linux `select_task_rq_fair()` 注释明确说普通 `WF_TTWU` 唤醒走快路径保留
`prev_cpu`，完整 sched-domain 遍历只由 `WF_EXEC`/`WF_FORK` 触发。CongCore
还没有 sched-domain 和 PELT（Per-Entity Load Tracking，每个调度实体的负载
追踪），因此没有伪造一套全域扫描模型。长期方案应在有实测证据需要时引入 Linux
式 sched-domain/PELT，而不是恢复每次唤醒的全域锁遍历。

## 对应提交

- 状态：待提交，当前实现仍位于未提交工作树。
- 基线：顶层 `21332ba37bf1ba0efe8229e7f80eeffa3b99a239`；`os/`
  `b0185b3a4522c0ffc52599d73bd17b3d52320815`。
- 建议提交主题：`sched: preserve fair task CPU on requeue`。

## 对比提升

49,152 次 `sched_yield` requeue 的 guest 中位数由 `575552 us` 降至
`234696 us`（-59.2%，吞吐约 2.45x），host 中位数由 `633 ms` 降至 `307 ms`
（-51.5%）。BuildStorm 短窗口仅相差约 3%，不足以作为端到端收益证据，不把它
当成完整通过。

以下是 AI 的具体分析，作为存档。

---

## 历史分析背景

这个问题涉及调度器的 CPU 选择与 runqueue 锁交互。旧实现把"选择最空闲核心"当作每次 enqueue 的默认路径，包括 tick/yield 后的 requeue，导致 12 核场景下每个时间片都做一次 O(n) 锁遍历。以下保留完整的 perf 证据、Linux 对照、idle-bitmap 回退记录和回归测试结果。

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
