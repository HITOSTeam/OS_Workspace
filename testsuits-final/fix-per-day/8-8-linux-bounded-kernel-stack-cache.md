# 8-8 Linux 风格有界内核栈映射缓存

## 问题概述

RISC-V BuildStorm 的依赖图收窄到单个 crate 后，host CPU 会从约 7.5 核逐步降到
约 1 核。串口停滞期间 guest shell、uptime 探针和 QEMU 都仍然可响应，因此现场不是
死锁；真正的问题之一是编译风暴的进程/线程 churn 让内核反复创建和销毁高半区内核栈
映射。

旧实现每次 `kstack_alloc()` 都在共享的 `KERNEL_SPACE` 中建立映射，并调用
`flush_kernel_shared_tlb()`；每次 `KernelStack::drop()` 又立即删除映射，删除路径同样
完成 shared-kernel TLB shootdown。RISC-V 上远端部分需要 SBI RFENCE，QEMU TCG 还会
因此丢失 softmmu TLB 和 translation-block jump cache。一个短生命周期线程要付一次
map 和一次 unmap 的全机失效，而 BuildStorm 正是 fork、exec、rustc/codegen thread
最密集的 workload。

本批采用其他工作人员提出的“缓存已映射内核栈”方向，但没有原样接收初稿。最终实现
按 Linux 的 VMAP stack cache 补齐了容量边界、复用前清零、锁作用域和分配失败回退，
并用独立 clean baseline 做了 RISC-V A/B。它不改变线程、地址空间或 TLB 的可观察
语义，只把反复 map/unmap 变成有界映射复用。

## 如何发现

### 1. perf 把“慢”指向反复 TLB/TB 冷启动

RISC-V 长测的 QEMU perf 报告中，`helper_lookup_tb_ptr`、TB hash lookup、softmmu
translation 和跨 vCPU mutex 路径占据主要 CPU。单看这些 host 符号只能说明 QEMU
长期处在冷翻译路径，不能直接证明 guest 根因，因此继续核对 guest 源码：

- `os/src/task/id.rs` 的新栈映射后无条件调用 `flush_kernel_shared_tlb()`；
- 栈析构调用 `MemorySet::remove_area()`，其 invalidation finish 也会对共享内核映射
  完成 shootdown；
- RISC-V 的远端失效最终走 SBI RFENCE，且要等待其它在线 hart 确认；
- BuildStorm 的 rustc、cc、ld 和线程池持续制造内核栈生命周期 churn。

长测现场保存在：

```text
testsuits-final/.tmp/final-runs/20260807-riscv-stall-repro-160/run/serial.log
testsuits-final/.tmp/final-runs/20260807-riscv-stall-repro-160/run/host-metrics.log
testsuits-final/.tmp/final-runs/20260807-riscv-stall-repro-160/run/probe-latency.csv
testsuits-final/.tmp/final-runs/20260807-riscv-stall-repro-160/run/vcpu-pc-samples.txt
```

该证据链先定位候选机制；最终是否采用仍由下面的同条件 guest workload A/B 决定，
没有把 QEMU 自身热点直接当成修改有效的证明。

### 2. 其他工作人员的初始 A/B 给出同方向结果

已有的 RISC-V `fork_thread_group_perf_smoke` A/B 各跑 11 轮，全部 `rc=0`：

```text
testsuits-final/.tmp/final-runs/20260807-riscv-kstack-cache-ab-166/baseline-retry/
testsuits-final/.tmp/final-runs/20260807-riscv-kstack-cache-ab-166/optimized/
```

guest 中位数从 208,362 us 降到 170,929 us，耗时减少 17.97%，等价吞吐提高
21.90%。这说明方向值得采用，但初稿尚未清除复用栈，缓存容量也没有严格对齐 Linux，
所以本批先审查和收敛实现，再重新独立测量。

### 3. clean baseline 上的独立复测闭合因果

从顶层 `c948a928`、`os/` `a24b950` 建立隔离 worktree，只加入本批两文件改动。
两边使用相同 release 配置、官方 RISC-V 镜像、8 hart、8 GiB、独立 qcow2 overlay，
各启动两次 QEMU，每次连续运行 11 轮，总计每边 22 轮。每轮都有 60 秒 guest 硬截止，
全部 `rc=0`。

测试程序先建立包含 16 个线程的 thread group，然后顺序执行 128 次
`fork()`、子进程 `exit()` 和父进程 `waitpid()`，直接放大内核栈创建/销毁成本。
主 A/B 原始数据：

```text
.tmp/ablate/20260808-kstack-refined-baseline-1/results.csv
.tmp/ablate/20260808-kstack-refined-baseline-2/results.csv
.tmp/ablate/20260808-kstack-refined-candidate-2/serial.log
.tmp/ablate/20260808-kstack-refined-candidate-3/results.csv
```

`candidate-2/results.csv` 的早期 expect 正则截短了部分 guest 数字，因此统计时使用
同目录 `serial.log` 中完整的 `elapsed_us`；该日志的 11 个 rc 均为 0。修正 expect
后 `candidate-3/results.csv` 可直接使用。

| 指标 | 基线（n=22） | 候选（n=22） | 改善 |
| --- | ---: | ---: | ---: |
| guest elapsed 中位数 | 201,315 us | 134,971.5 us | **-32.96%** |
| 等价 guest 吞吐 | 1.00x | 1.4915x | **+49.15%** |
| host elapsed 中位数 | 290 ms | 209 ms | **-27.93%** |
| 成功轮次 | 22/22 | 22/22 | 无回归 |

主 A/B 候选与最终提交的语义相同，但最终审查又把 32-KiB 清零操作移到了 cache mutex
之外。为避免把这项锁作用域调整当作未经验证的“显然更快”，又用最终源码二进制跑了
11 轮：guest 中位数 111,940 us、11/11 `rc=0`。该单次启动只作为最终源码没有回退的
确认，不拿更好的 111,940 us 代替上表的保守主结论：

```text
.tmp/ablate/20260808-kstack-final-candidate/results.csv
.tmp/ablate/20260808-kstack-final-candidate/serial.log
```

## Linux 对照

本批对照本地 Linux `exampleOs/linux` commit
`4549871118cf616eecdd2d939f78e3b9e1dddc48` 的 `kernel/fork.c`：

- `CONFIG_VMAP_STACK` 下定义 `NR_CACHED_STACKS = 2`，每 CPU 保留两个已经映射的
  `vm_struct`，避免常规线程 churn 重复进入 vmalloc/vfree 和 TLB flush；
- `alloc_thread_stack_node_from_cache()` 与
  `try_release_thread_stack_to_cache()` 使用 per-CPU slot，并检查 NUMA locality；
- 缓存命中后调用 `clear_pages()` 清除 stale pointer，再把栈交给新线程；
- cache 满时走 delayed `vfree()`，仍有明确的有界 fallback；
- 未启用 VMAP stack 时，内核栈来自线性映射页，线程创建/退出本来就不需要修改
  kernel page table。

CongCore 没有 NUMA placement，也没有成熟的 per-CPU vmalloc cache，因此使用总预算
`MAX_HARTS * 2` 的共享小缓存，容量与 Linux “每 CPU 两个”在机器总量上相同。全局锁
只包围一次 `Vec::pop()` 或 `Vec::push()`，清零和 map/unmap 都在锁外；如果后续 perf
证明这把锁成为热点，再改成 per-hart slot，而不是在没有证据时增加跨 hart 回收复杂度。

## 怎么解决

### 1. 有界保留映射和物理页

`KSTACK_CACHE` 保存已经退出任务的 stack ID，映射和 backing frames 都继续留在
`KERNEL_SPACE`。release 配置的栈为 32 KiB：RISC-V 8 hart 最多缓存 16 个，即
512 KiB；LoongArch 12 hart 最多缓存 24 个，即 768 KiB。缓存不是无界泄漏。

`KernelStack::drop()` 优先把 ID 放进 cache。cache 已满或 `Vec::try_reserve()` 失败
时，才执行原来的 `remove_area()`、shared-kernel shootdown 和 ID 回收路径。因此
低内存元数据分配失败不会让 Drop panic，也不会丢失栈 ID。

### 2. 复用前清零，且不持全局锁触页

`kstack_alloc()` 命中 cache 后立即取得该 ID 的唯一所有权，然后释放 cache mutex，
再清零整个映射。清零使用一处带完整安全条件说明的 `write_bytes`：缓存中的 ID 已经
不属于任何 task，完整范围仍然 writable mapped，pop 已把唯一所有权转移给当前调用者。
这与 Linux 的 `clear_pages()` 目的相同，防止新任务看到旧栈数据或 stale pointer。

清零后直接返回，不修改页表、不发本地 fence，也不发远端 SBI RFENCE。只有 live
thread high-water mark 超过缓存可以提供的已映射栈数量时，才建立新映射；只有回收时
cache 已满，才真正删除旧映射。

### 3. 可关闭的 `/proc/perf` 计数

增加三个只在 `DEBUG_PERF=true` 时更新的计数器：

- `tlb_kstack_reuses`：从 cache 取得映射；
- `tlb_kstack_maps`：高水位增长而新建映射；
- `tlb_kstack_unmaps`：cache 满而删除映射。

诊断内核只运行一轮 128-iteration benchmark，最终得到：

```text
tlb_kstack_reuses: 128
tlb_kstack_maps: 20
tlb_kstack_unmaps: 2
```

128 次循环恰好复用了 128 个栈；20 次 map 是 shell、初始 thread group 等启动高水位，
而 steady-state fork 不再继续 map/unmap。两次 unmap 是缓存达到固定预算后的正常
fallback。原始串口日志：

```text
.tmp/ablate/20260808-kstack-refined-perf-2/serial.log
```

生产提交已经恢复 `DEBUG_PERF=false` 和 `DEBUG_SCHED=false`，不会让每次栈操作承担
额外原子计数成本。

## 对应提交

| 项目 | 值 |
| --- | --- |
| 顶层分支 / 基线 | `dev_final` / `c948a92870b83a2df4fe483f6fc3f1cdea16e65c` |
| `os/` 基线 | `a24b950e6013e6a3d6da26ddb72b676cccaf3052` |
| `os/` 修复 | `da190f90640edc08de48f628da16f259fc5ca077`（`task: cache mapped kernel stacks`） |
| 顶层集成 | `bbe72850a0194001142b5c0f6204cf712ff259c2`（`task: integrate mapped kernel stack cache`） |

`os/` 修复只包含 `src/task/id.rs` 和 `src/perf.rs`。共享工作树中其他工作人员正在开发
的 MMIO 高半区、ASID、slab、scheduler、file mmap 等改动均未加入该提交。

## 对因提升与当前边界

可以直接证明的是：RISC-V 8-hart fork/thread churn 微基准的 guest 中位耗时减少
**32.96%**，吞吐提高 **49.15%**；`/proc/perf` 又证明 128 次循环全部走已映射栈复用，
而不是继续触发 128 组 map/unmap shootdown。初始独立工作人员 A/B 的 +21.90% 与
本批复测方向一致。

当前不能声称：

- 完整 RISC-V BuildStorm 已因此通过，或完整耗时已经按 32.96% 降低；依赖图收窄后
  只有少量进程活跃，微基准百分比不能外推到整轮；
- 该提升能与 dcache、block cache 或其它批次直接相加；
- 所有 RISC-V TLB 问题已经解决。驱动切换页表、MMIO 映射和 ASID fast path 是不同
  机制，必须分别做受控 A/B。

审查期间也隔离测试了另一位工作人员的 RISC-V 高半区 MMIO/IRQ guard 候选。它在
一轮启动中停在 `init_proc`，成功启动的 `exec_file_page_cache_perf_smoke` 中位数又比
两个基线启动慢约 11%--13.5%。因此该候选没有进入本批；“Linux 方向合理”不能替代
当前实现的稳定性和 workload A/B。

后续工作已找到该初版停滞与当前 block completion hardirq 路径之间的边界：最终版本
保留 PLIC 外部中断入口 guard，只删除 VirtIO 提交、poll 和驱动边界的切换，并重新
完成 10/10 稳定性与独立 A/B 后作为单独批次合入。详见
`8-8-linux-riscv-high-half-mmio.md`；这里保留的是当时拒绝初版的历史结论。

### 240 秒 RISC-V `tg-xtask` 风险闸门

集成提交完成后，又从相同官方镜像分别创建全新 root/user qcow2 overlay，用精确最终
候选和 clean baseline 各跑一次 `timeout 240 cargo build -p tg-xtask`。候选先跑、
基线后跑，因此 host page cache 若有跨轮影响，会偏向基线；两边都关闭 `DEBUG_PERF`
和 QEMU `-perfmap`，并在 timeout 写出结果后的第一个探针主动结束 QEMU。

原始日志：

```text
.tmp/ablate/20260808-kstack-tg-gate-candidate-2/probe-latency.csv
.tmp/ablate/20260808-kstack-tg-gate-candidate-2/host-metrics.log
.tmp/ablate/20260808-kstack-tg-gate-candidate-2/serial.log
.tmp/ablate/20260808-kstack-tg-gate-baseline/probe-latency.csv
.tmp/ablate/20260808-kstack-tg-gate-baseline/host-metrics.log
.tmp/ablate/20260808-kstack-tg-gate-baseline/serial.log
```

| 240 秒结果 | 基线 | 最终候选 | 差异 |
| --- | ---: | ---: | ---: |
| Cargo 输出字节 | 858 | 1,049 | 归一化速率 **+22.06%** |
| Cargo 输出行 | 28 | 34 | 归一化速率 **+21.23%** |
| `target/debug/deps/*.d` | 27 | 29 | **+7.41%** |
| QEMU CPU ticks | 154,520 | 146,477 | **-5.20%** |
| QEMU peak RSS | 2,205,888 KiB | 2,062,204 KiB | **-6.51%** |
| QEMU `read_bytes` | 38,699,008 | 30,060,544 | **-22.32%** |
| probe latency 中位数 | 2,241.0 ms | 1,995.5 ms | **-10.95%** |
| 结果 | `rc=124`，未生成 xtask | `rc=124`，未生成 xtask | 均按硬截止结束 |

Cargo stdout 会受缓冲和 crate 完成顺序影响，单轮 host I/O 也会受 page cache 波动影响，
所以上表只用于确认真实 workload 没有回退；本批的定量对因结论仍以前述 22+22 轮
fork/thread 微基准为主。更稳健的 deps 计数只领先 7.41%，说明内核栈缓存有用，但不是
RISC-V `tg-xtask` 整体慢数倍的唯一根因。两边输出和 deps 在每个探针都继续增长，QEMU
RSS 有界、host swap 未耗尽，没有卡死或资源泄漏迹象。

测试接线的第一次尝试漏设 Cargo PATH，在 guest uptime 1.13 秒以 `rc=127` 返回；该轮
没有进入 rustc，已明确排除在 A/B 外。后续重启 overlay 读取 Cargo tail 时又发现停止
QEMU 前没有 `sync`，只能看到部分已落盘输出，因此没有用不完整的 crate 名称补强结论。

短 gate 已通过“不回退”条件，但候选 240 秒仍只生成 29 个 deps，离完整 `tg-xtask`
很远。结合此前完整 RISC-V 长测在 3,229 秒仍未生成 xtask，本批不继续盲跑一小时；
下一步应先隔离验证剩余的 RISC-V SATP/TLB 或文件系统候选，只有新的短 gate 出现量级
改善后再进入完整 BuildStorm。若进度明显落后或超过停滞上限，仍按约定及时停止并
保留日志。

## 验证与复现

最终两文件提交在隔离 clean worktree 中通过：

```sh
TMPDIR=$PWD/.tmp ARCH=riscv64 cargo check \
  --manifest-path os/Cargo.toml \
  --target riscv64gc-unknown-none-elf

TMPDIR=$PWD/.tmp ARCH=loongarch64 cargo check \
  --manifest-path os/Cargo.toml \
  --target loongarch64-unknown-none-softfloat
```

受控 A/B runner：

```text
.tmp/ablate/run_fork_perf_expect.sh
```

它为 root 与 user 盘分别创建新的 qcow2 overlay，等待真实 shell prompt，每轮在 guest
运行：

```sh
timeout 60 /user/fork_thread_group_perf_smoke.bin
```

并在 11 轮结束后主动退出 QEMU。任一轮超时、rc 非零、shell 丢失或 QEMU EOF 都会
立刻结束，不会无限等待。

`tg-xtask` gate runner：

```text
.tmp/ablate/run_tg_gate_expect.sh
```

两边分别执行：

```sh
timeout --signal=INT --kill-after=10s 360s \
  bash .tmp/ablate/run_tg_gate_expect.sh LABEL KERNEL_ELF 240
```

runner 每约 30 秒记录 guest uptime、Cargo 输出字节/行数、deps 数量、xtask 大小和
result，同时每 2 秒记录 QEMU CPU、RSS、I/O 以及 host MemAvailable/SwapFree；guest
240 秒 timeout 之外还有 host 360 秒最终截止。

| 资产 | 版本 |
| --- | --- |
| final source | `b5ec6ef8497e1818cbdec3b54bb722f036e57972`（`final-2026`） |
| RISC-V 镜像 SHA-256 | `d899fe43d333d1d17ad8a5f8a8b74b68117b8c1ceacfc3843bfeadb1ca705bd1` |
| QEMU | 11.0.3 |
| 架构 / SMP / 内存 | RISC-V64 / 8 / 8 GiB |
| Linux 参考源码 | `4549871118cf616eecdd2d939f78e3b9e1dddc48` |
| 基线内核 SHA-256 | `4da372288f9145f92d30f1450df35100591dda18aa07f59a6751d9b83fe23472` |
| 最终候选内核 SHA-256 | `42a6a2b1b0d08bc561935257c478f25300c8ea97fcb4f34e7a246676a7800d2a` |

## AI 使用说明

本批使用 AI 辅助复核专家提供的 perf/日志分析、追踪 CongCore 与 Linux 的内核栈
生命周期、审查其他工作人员的候选补丁，并设计 clean baseline 的多轮 RISC-V A/B。
采用候选前补上 Linux 已有的复用前清零、严格容量和失败回退；统计时发现
`candidate-2/results.csv` 被 expect 正则截短，改从串口原值重算，避免使用错误数据。
所有性能结论均可由上列 CSV/串口日志重新计算，AI 判断未替代双架构编译、真实 guest
执行、硬超时或独立 overlay。
