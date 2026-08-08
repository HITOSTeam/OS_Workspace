# 8-8 Linux 风格 RISC-V VirtIO 块完成混合等待

## 问题概述

RISC-V BuildStorm 的 `tg-xtask` 前置编译并没有死锁，但一次把同步块请求从
“短轮询 + 反复 `yield`”改成“提交后立即睡眠”的过渡实现，使真实编译进度明显
回退。短小的块 I/O smoke 曾显示纯 WaitQueue 比旧的反复轮询更快，完整编译风暴却
暴露了相反结果：QEMU VirtIO 的大量 4 KiB 请求通常会很快完成，每个请求都睡眠会
额外制造调度、唤醒和中断开销，并放大 RISC-V 的地址空间切换成本。

本次保留真正的阻塞等待，但在它前面恢复一个有界的快速完成窗口：最多轮询 64 次，
一旦调度器要求重新调度便提前退出；只有请求仍未完成时才进入 WaitQueue。这样既不让
慢设备请求无限占用 CPU，也不强迫每个微秒级请求经历完整的睡眠/唤醒往返。

## 如何发现

### 1. 长跑先证明是性能回退，不是卡死

对照运行：

```text
testsuits-final/.tmp/final-runs/20260807-riscv-dcache-buildstorm-full-162/
testsuits-final/.tmp/final-runs/20260807-riscv-mmio-kstack-block-buildstorm-full-171/
```

两轮都是 RISC-V、8 vCPU、8 GiB、同一决赛镜像，探针始终可以执行 shell，串口没有
panic 或 OOM。旧候选在 guest uptime `1889.70 s` 时输出为 `10989 B`；带纯睡眠块等待
的候选到 `2304.49 s` 才达到近似相同阶段的 `10965 B`，达到同一编译位置约慢
`414.79 s`，即 **21.95%**。后者当时仍在增加 deps 文件，因此按用户要求及时终止，
没有把低吞吐误判成死锁，也没有继续消耗完整 18000 秒上限。

这项长跑同时包含其他内存管理和块缓存修改，不能单独把 21.95% 全部归给等待策略，
所以随后从同一份当前源码构建只相差一个常量的内核，做隔离 A/B。

### 2. 同源消融锁定纯睡眠路径

受控内核：

| 版本 | 唯一差异 | 内核 SHA-256 |
| --- | --- | --- |
| `spin64` | `COMPLETION_POLL_SPINS = 64`，超出预算后睡眠 | `2f98247fcdc8f9dc592cdc6be5fd1fdaf2363506cf2b88dc2d205dff25e8d019` |
| `spin0` | `COMPLETION_POLL_SPINS = 0`，每个请求直接睡眠 | `54fd54eb19072e8656c00fd312b5886174866a56539dc8fde03c7a2631c79e11` |

两边使用相同 RISC-V 8 vCPU、8 GiB、同一基准镜像的独立 qcow2 overlay、240 秒
`cargo build -p tg-xtask`，关闭 `DEBUG_PERF` 和 QEMU `-perfmap`；host `perf` 只附加到
准确的 QEMU PID，不改变 guest 配置。原始数据：

```text
testsuits-final/.tmp/final-runs/20260808-riscv-blockcache-capacity-ab-173/cap16k/
testsuits-final/.tmp/final-runs/20260808-riscv-blockwait-poll-ab-175/spin0/
```

| 采样 | `spin0` uptime / output | `spin64` uptime / output | `spin64` 输出增幅 |
| ---: | ---: | ---: | ---: |
| 1 | 60.91 s / 518 B | 60.78 s / 644 B | +24.32% |
| 2 | 121.26 s / 1054 B | 121.18 s / 1107 B | +5.03% |
| 3 | 195.74 s / 1352 B | 194.14 s / 1536 B | +13.61% |

最后一个可比采样按 guest uptime 归一化后，混合等待的输出速率提高 **14.55%**。
同时核对 Cargo tail，`spin64` 已进入 `aws-lc-sys`、`serde_json`、`zerocopy` 和
`getrandom` 等更后面的 crate，排除了仅由 stdout 缓冲造成的领先。

### 3. perf 与反证消融排除块缓存容量和算法

无 `-perfmap` 的 `cycles:u` 采样均为 `Total Lost Samples: 0`。纯睡眠版本采到约
`201.44B` cycles，混合版本约 `209.30B` cycles；后者总 cycles 更高是因为它在相同
时间内完成了更多编译工作，不能只看总采样数判定回退。更有区分度的是宿主
`pthread_mutex_lock`：纯睡眠约占 1.79%，即约 `3.61B` sampled cycles；混合等待虽然
推进更多，仍只有 1.40%，约 `2.93B` cycles，绝对值下降约 **18.8%**。这与减少 QEMU
vCPU 睡眠/唤醒协调的机制一致。

为避免把同批块缓存改动误当成根因，还做了两个负对照：

1. 当前哈希缓存只改变容量 `2048 -> 16384`。前 121 秒输出完全相同，约 194 秒时
   16384 版本只领先 4.1%，不足以解释长跑约 22% 的阶段性回退；
2. 容量同为 16384 时，当前 `HashMap + 有界 clean-first 回收` 相比旧
   `BTreeMap/LRU`，三个探针的归一化输出速率分别提高约 24.8%、28.8% 和 26.3%。
   因此新块缓存算法是收益项，不是本次减速源。

对应原始数据：

```text
testsuits-final/.tmp/final-runs/20260808-riscv-blockcache-capacity-ab-173/cap2k/
testsuits-final/.tmp/final-runs/20260808-riscv-blockcache-algorithm-ab-174/btree16k/
```

### 4. 再与旧 `spin + yield` 做直接消融

360 秒验证仍比更早的旧内核慢约 7% 后，又从当前源码的隔离快照构建了只恢复旧
“轮询一轮、yield、再轮询”控制流的临时内核：

| 版本 | 内核 SHA-256 |
| --- | --- |
| 当前混合等待 | `2f98247fcdc8f9dc592cdc6be5fd1fdaf2363506cf2b88dc2d205dff25e8d019` |
| 临时 legacy `spin + yield` | `5f45d38492f502435e1b6b6e5d819e332ca68104f58e31a5163ff6bc869fa604` |

legacy 原始数据：

```text
testsuits-final/.tmp/final-runs/20260808-riscv-blockwait-legacy-ab-178/legacy/
```

| 采样 | legacy uptime / output | 混合 uptime / output | 混合归一化速率提升 |
| ---: | ---: | ---: | ---: |
| 1 | 61.02 s / 606 B | 60.78 s / 644 B | +6.69% |
| 2 | 121.32 s / 1083 B | 121.18 s / 1107 B | +2.33% |
| 3 | 194.04 s / 1476 B | 194.14 s / 1536 B | +4.01% |

三个采样方向一致，因此剩余约 7% 不是“混合方案不如旧 yield”造成的。临时快照构建
时共享 `results/` 比原内核多一个嵌入 app 名称，BuildStorm 不使用该名称表；即便保守
地把几个百分点差异视为不够严格，这组数据也不支持回滚混合等待。共享工作树在整个
消融期间保持原文件 SHA-256
`7dedeaff9898ecf17fae66fc94eec13a28bdaaca4944ffbcce48bed4a3a7e491`，临时 legacy
代码没有回写。

## Linux 对照

本批对照本地 Linux `exampleOs/linux` commit
`4549871118cf616eecdd2d939f78e3b9e1dddc48`：

- `block/bio.c:1469-1500` 的 `bio_await()` 在提交前安装 completion，提交后通过
  `blk_wait_io()` 睡眠，完成回调 `bio_wait_end_io()` 调用 `complete()`；这给出了无
  丢失唤醒的真正阻塞路径；
- `block/blk-mq.c:5250-5265` 的 `blk_hctx_poll()` 在设备支持 polling 时调用驱动
  `poll()`，完成、信号、错误或 oneshot 条件会退出，并且只在 `!need_resched()` 时继续
  `cpu_relax()`；这给出了“低延迟请求可轮询，但轮询必须受调度约束”的边界；
- `block/blk-core.c:950-980` 只在队列和请求具备 poll 条件时进入 `blk_mq_poll()`，说明
  Linux 不是对所有设备、所有请求无条件忙等。

CongCore 还没有 Linux 完整的 blk-mq、设备 poll capability、adaptive hybrid polling
和 block plug。当前实现复制的是成熟的控制流语义，而不是逐行移植：固定 64 次的
短窗口负责 QEMU VirtIO 的常见快速完成，`need_resched` 对应检查负责调度公平，超过
窗口后用 completion WaitQueue 真正睡眠。更完整的后续方案应根据
`short_poll_completions / submitted` 和完成延迟动态调整预算，并只对声明低延迟 poll
能力的设备启用。

## 怎么解决

`os/src/drivers/block/async_queue.rs::submit_and_wait()` 的任务态完成路径现在按以下顺序
执行：

1. 最多调用 `drain_used()` 64 次，直接收割已经进入 used ring 的请求；
2. 每 8 次检查 `should_resched_for_busy_poll()`，需要调度时提前停止轮询；
3. 若请求仍未完成，增加 `completion_sleeps` 计数，并调用
   `request.waiters.wait_until()` 离开 run queue；
4. IRQ 或其他 poller 在队列锁外先发布完成状态，再唤醒 request waiter 和 queue-full
   waiter。

`completed` 使用 Release/Acquire 发布结果；`wait_until()` 在登记 waiter 后重新检查
谓词，因此完成 IRQ 恰好发生在“停止轮询、准备睡眠”之间也不会丢失唤醒。启动早期
没有当前任务的路径继续同步 poll，避免在调度器未就绪时尝试睡眠。

被否决的两个极端方案是：

- 旧的“64 次轮询后反复 cooperative yield，再回来继续轮询”不会永久忙等，但慢请求
  仍会反复上下文切换；
- 过渡的“每个请求立即睡眠”对聚焦 smoke 有收益，却在真实 BuildStorm 的大量快速
  4 KiB I/O 中产生睡眠/唤醒风暴。

混合方案同时保留两者正确的部分：快速完成不切换上下文，真正慢的请求不烧 CPU。

## 对应提交

本报告生成时工作区由多个工作人员共享。验证确认块等待修改只依赖 `os` 基线已经
存在的 `WaitQueue` 和 `should_resched_for_busy_poll()` 后，只提交了
`async_queue.rs`；同批 RISC-V TLB、内核栈、块缓存诊断等共享脏修改没有混入。

| 项目 | 值 |
| --- | --- |
| 顶层分支 / 基线 | `dev_final` / `fb620770bac83a04fce43fce95691e6aeb8216da` |
| `os/` 基线 | `960fd0f`（`vfs: trust versioned ext4 dentries`） |
| 内核修复 | `19ef1c8e9c31bb84aedc19f57a56d32f0342ecae`（`block: use bounded hybrid completion waits`） |
| 顶层集成 | 本报告所在提交（`tests: integrate hybrid block completion`） |

`completion_sleeps` 已保留在 block diagnostics 结构中。当前 `perf.rs` 同时包含别的
诊断批次，因此没有为一个导出字段把整份共享脏文件一起提交；后续整理 perf 批次时可
把该字段接入 `/proc/perf`。

## 对因提升与当前边界

本次可以证明的直接结果是：在完全相同源码和 240 秒 `tg-xtask` workload 下，
`spin64 + sleep` 相对 `spin0 + sleep` 的最后归一化进度提高 **14.55%**，同时宿主
`pthread_mutex_lock` sampled cycles 约下降 **18.8%**。这证明混合完成策略修复了纯
睡眠造成的主要回退。

当前不能声称：

- 完整 RISC-V BuildStorm 已通过；本次受控 A/B 都按设计在 240 秒以 `rc=124` 停止；
- 相比所有旧内核已经净提升 14.55%；旧长跑和当前候选还包含其他不同修改；
- `tg-xtask` 已在官方基准时间内完成。下一次完整运行必须在合并候选稳定后，从全新
  overlay 启动并持续监控输出、deps、QEMU CPU/RSS/I/O 和探针延迟；若确认吞吐仍明显
  低于旧基线，应及时停止而不是等待 18000 秒。

为执行这个停止条件，另做了一轮无 `-perfmap` 的 360 秒验证：

```text
testsuits-final/.tmp/final-runs/20260808-riscv-hybrid-blockwait-360-177/run/
```

该轮在 60/121/194/254/315 秒分别输出 `644/1135/1510/1981/2369 B`，探针延迟
`212--373 ms`，最终 Cargo 已推进到 `ring`、`aws-lc-rs`、`rustix` 和
`darling_core`；QEMU RSS 约 2.60 GiB，宿主可用内存约 23.3 GiB，swap 没有下降。
因此它不是卡死或资源泄漏。但与旧 run 162 相邻探针插值相比，315 秒仍慢约 7.4%，
没有证明当前完整候选获得净提升，所以按约定没有启动完整 BuildStorm。该轮在
`360.20 s` 以预期的 `rc=124` 截止并正常停止 QEMU。

## 验证

静态和库测试均通过：

```sh
cd os
TMPDIR=$PWD/../.tmp ARCH=riscv64 cargo check \
  --manifest-path Cargo.toml --target riscv64gc-unknown-none-elf
TMPDIR=$PWD/../.tmp ARCH=loongarch64 CARGO_NET_OFFLINE=true cargo check \
  --manifest-path Cargo.toml --target loongarch64-unknown-none-softfloat

cd ..
cargo test -p ext4-fs --target x86_64-unknown-linux-gnu
# 13 passed; 0 failed
```

RISC-V 运行态回归：

```text
testsuits-final/.tmp/final-runs/20260808-riscv-hybrid-blockwait-regressions-176/
```

| 测试 | 结果 |
| --- | ---: |
| `lazy_fault_local_tlb_smoke` | PASS |
| `file_mmap_lazy_fault_smoke` | PASS |
| `private_file_page_cache_smoke` | PASS |
| `clone_vm_mmap_smoke` | PASS |
| `tlb_shootdown_smp_smoke` | PASS |
| `riscv_icache_smp_smoke` | PASS |

生产配置已确认 `DEBUG_PERF=false`、`DEBUG_SCHED=false`。测试资产为 final source
`b5ec6ef8497e1818cbdec3b54bb722f036e57972`、RISC-V 镜像 SHA-256
`d899fe43d333d1d17ad8a5f8a8b74b68117b8c1ceacfc3843bfeadb1ca705bd1`、QEMU
`11.0.3`、perf `7.1.6`。

## `perfmap` 使用说明

需要把 QEMU JIT 地址归因到 guest Rust 函数时，继续按
`testsuits-final/AGENTS.md` 使用 `QEMU_EXTRA_ARGS=-perfmap`，并在 QEMU 退出前保存与
准确 PID 对应的 `/tmp/perf-${qemu_pid}.map`。本次短诊断保存在：

```text
testsuits-final/.tmp/final-runs/20260808-riscv-tg-p5-ab-172/p5-retry2/
```

该轮保留了 `perf.data` 和 79 MiB 的 `perf.map`，但 240 秒 workload 仅输出 548 B；
报告过程还把大量样本归入 `tb_gen_code -> perf_report_code -> libelf`。因此
`-perfmap` 很有用，但只能用于带硬截止的短时定位：

1. 先用 30--60 秒、99 Hz 的样本定位候选；
2. QEMU 退出前复制准确 PID 的 map，并检查 lost samples；
3. 发现探针超限时先向 perf 发送 `SIGINT` 使数据落盘，再停止 QEMU；
4. 所有正式性能 A/B 关闭 `-perfmap`，保持镜像状态、SMP、内存、workload 和截止时间
   相同。

不能把带 `-perfmap` 的绝对耗时与无 `-perfmap` 的运行直接相减，也不能在 map 已丢失
后用错误 PID 的文件强行解释旧样本。

## AI 使用说明

AI 用于交叉解析串口探针、host metrics、`perf.data`，设计容量、算法和等待策略的隔离
消融，并核对本地 Linux 源码的等待与 polling 边界。所有性能数字均来自上述可复现
日志；源码结论由实际文件和构建/运行回归复核，没有用模型估计值代替测量结果。
