# 8-8 RISC-V Linux 风格高半区 MMIO 共享映射

## 问题概述

RISC-V 旧实现把 PLIC、VirtIO MMIO 等设备寄存器恒等映射在低物理地址。它们落在
Sv39 root entry 0，而用户地址空间也必须使用 root entry 0，因此这部分设备映射不能
共享进用户页表。内核又刻意在 trap/syscall 期间保留当前用户 SATP，于是每次块读写、
驱动中断和 fallback poll 都要临时进入 `KernelPageTableGuard`：

1. 写 kernel SATP；
2. 对 kernel ASID 执行一次 `sfence.vma`；
3. 完成一次 MMIO/DMA 操作；
4. 恢复 user SATP；
5. 再对 user ASID 执行一次 `sfence.vma`。

在 BuildStorm 的数百万次小块 I/O 下，这不是普通 CSR 开销。QEMU TCG 会反复丢掉
softmmu TLB 和 translation-block jump cache，后续执行重新进入
`helper_lookup_tb_ptr`、TB hash lookup 和地址翻译慢路径。

本批采用另一位工作人员提出的高半区 MMIO 方向，但没有采用会间歇停在
`init_proc` 的初版。最终版本把设备映射放进一个可共享的内核高半区 root，删除
VirtIO 提交、poll 和驱动边界的 SATP guard；当前阻塞式完成队列还不是 hardirq-safe，
所以 PLIC 外部中断入口暂时保留唯一一处 guard。该收敛版本经过 10 次启动/读写稳定性
门禁、双架构编译、冷 I/O、exec、host perf 和 240 秒 tg-xtask C-B-C-B 验证后才合入。

## 如何发现

### 1. 长测不是死锁，而是 RISC-V 单线程吞吐塌陷

下列 RISC-V BuildStorm 现场中，shell 探针始终可 fork/exec、QEMU RSS 稳定、没有
OOM/panic；依赖图收窄后只有约一个宿主核工作，说明 tg-xtask 编译仍在算而不是全局
死锁：

```text
testsuits-final/.tmp/final-runs/20260807-buildstorm-riscv-full-128/run/
testsuits-final/.tmp/final-runs/20260807-riscv-stall-repro-160/run/
testsuits-final/.tmp/final-runs/20260807-riscv-dcache-buildstorm-full-162/
```

QEMU perf 的主要热点是 `helper_lookup_tb_ptr`、`tlb_set_page_full`、
`qht_lookup_custom`、`riscv_env_mmu_index` 和跨 vCPU mutex。它们只能说明 TCG 长期
处在翻译缓存冷态，继续反汇编和核对 guest 源码后才闭合根因：

- `os/src/drivers/block/virtio_blk.rs` 的 read、write、IRQ、poll 都进入
  `KernelPageTableGuard`；
- `os/src/mm/memory_set.rs` 的 RISC-V `activate_token()` 每次 SATP 切换都执行
  ASID 范围 `sfence.vma`；
- 设备恒等映射位于低地址 root 0，确实无法直接复制到用户页表；
- LoongArch 依靠 DMW/direct map，不支付该 RISC-V 独有成本。

### 2. perf 之后仍以对因 workload A/B 决定是否采用

本批用 `perf stat` 包住“独立启动 QEMU + 冷读一个连续 54.9 MB 文件”的完整过程。
每边 5 个独立样本的 host hardware counter 中位数如下：

| perf 指标 | 基线 | 候选 | 差异 |
| --- | ---: | ---: | ---: |
| cycles | 6,624,268,692 | 6,528,969,004 | **-1.44%** |
| instructions | 24,173,670,226 | 23,762,663,361 | **-1.70%** |
| branches | 3,819,956,295 | 3,735,785,936 | **-2.20%** |
| branch misses | 14,472,093 | 13,902,636 | **-3.93%** |

`task-clock` 和单次 wall time 受宿主调频、其它进程和 boot hart 波动影响，未用于上述
结论；guest 内部冷读的 14+14 样本见后文。perf 证明候选确实减少宿主执行工作，guest
计时再证明这些减少能转化成吞吐，而不是只改变采样比例。

原始 perf stat 文件：

```text
.tmp/ablate/20260808-mmio-perfstat-base-{1..5}.csv
.tmp/ablate/20260808-mmio-perfstat-candidate-{1..5}.csv
```

### 3. `-perfmap` 的使用与本轮无效样本

QEMU `-perfmap` 很有用：它把 TCG 生成代码写入 `/tmp/perf-<qemu-pid>.map`，让
`perf report` 可以把 JIT PC 关联回 guest 地址。使用时必须在 QEMU 退出前同时归档：

```text
perf.data
/tmp/perf-<pid>.map
本轮 kernel ELF
```

否则报告只能看到匿名 JIT 地址。仓库 runner 示例为：

```text
.tmp/ablate/run_perfmap.sh
```

本轮曾用固定 30 秒 warmup 直接启动 `-perfmap` 采样，但 perfmap 的额外翻译/写 map
成本让 guest 在 SMP online 阶段命中启动超时，串口只到
`riscv64 SMP startup timeout`，工作负载根本没有开始。该 `perf.data` 明确排除：

```text
.tmp/ablate/20260808-satp-buildstorm-perf-baseline/
```

以后必须等 shell/workload readiness marker 后再启动 perf，并给 perfmap 诊断内核单独
放宽启动窗口；不能用固定 host sleep 猜 guest 已进入目标阶段。由于本批已有长测 perf
定位和有效的 `perf stat`/workload A/B，没有拿这份启动期无效样本补强结论。

## Linux 对照

本批对照本地 Linux `exampleOs/linux` commit
`4549871118cf616eecdd2d939f78e3b9e1dddc48`：

- `arch/riscv/include/asm/pgalloc.h` 的 `sync_kernel_mappings()` 在 `pgd_alloc()` 时
  把 `USER_PTRS_PER_PGD..PTRS_PER_PGD` 的内核半区复制进每个用户 pgd；
- `mm/ioremap.c` 的 `generic_ioremap_prot()` 从 `IOREMAP_START..IOREMAP_END` 分配
  内核虚拟地址，再用 `ioremap_page_range()` 映射设备物理页；
- 驱动和 hardirq 在当前 `mm` 上访问内核高半区，不为每次设备访问切换 SATP；
- Linux block 完成中断不在 hardirq 中睡眠，也不拿会与被中断提交路径形成同 hart
  重入的阻塞调度锁，实际唤醒/后半部机制与本内核当前实现不同。

CongCore 尚没有完整 vmalloc/ioremap allocator，所以先保留一个固定 1 GiB 窗口：
`0xffff_ffff_4000_0000..0xffff_ffff_8000_0000`，即 Sv39 root entry 509。QEMU
`virt` 机器本批使用的设备物理地址都低于 1 GiB；root 509 又位于内核栈 root 510 和
trampoline root 511 之下，不与用户映射冲突。

## 怎么解决

### 1. 建立并共享高半区设备 root

`KERNEL_MMIO_WINDOW_BASE` 和 `mmio_va(pa)` 提供固定物理到内核虚拟地址偏移。
`MemorySet::new_kernel()` 仍保留早期启动需要的 identity MMIO 映射，同时把每段
`MMIO` 映射到高半区窗口。创建用户地址空间时：

- 预先建立并标记 MMIO root 为 global；
- 把 root 509 从 kernel page table 分享进 user page table；
- 把该 root 加入用户 VMA 冲突检测和 overlap-end 计算，禁止用户映射覆盖；
- PTE 不带 `U` 权限，用户态不能直接读写设备寄存器。

### 2. PLIC 与 VirtIO 改用高半区地址

PLIC 基址和 VirtIO `MmioTransport`/`mmio_phys_to_virt()` 使用 `mmio_va(pa)`。
DMA buffer 仍使用已经共享的物理 direct map。这样 read/write 提交、设备入口和
fallback poll 都能在当前 SATP 上运行，删除这些位置的
`KernelPageTableGuard::enter()`。

### 3. 保留外部中断 guard，明确后续删除条件

初版连 PLIC dispatch 的 guard 一起删除后，出现间歇性 `init_proc` 停滞。进一步
缩小现场发现，当前块完成中断会直接进入阻塞 wait queue/scheduler 唤醒路径；该路径
使用的 `spin::Mutex` 可能与被中断提交路径形成同 hart 重入，尚不具备 Linux hardirq
上下文的约束。

最终版本只在 `handle_external_interrupt()` 周围保留 guard。另一位工作人员的最终
候选先完成 10/10 次启动/读写 smoke；叠加当前 `da190f` 内核栈缓存后，本批又完成
10/10 次，每次都到 shell，并完成块读、块写和 `sync`：

```text
.tmp/mmio-investigation/out/smoke/fix-smoke-{1..10}/serial.log
.tmp/mmio-investigation/out/smoke/combined-smoke-{1..10}/serial.log
```

未来应把 block completion wakeup 延后到 softirq/timer/专门 worker，或让完成路径真正
hardirq-safe；到那时再删除最后一个 guard，而不是现在用“Linux 不切页表”掩盖当前
完成队列语义差异。

### 4. 被否决的更小 ASID 0 快路

审查期间还隔离测试了“只在切入永久 kernel ASID 0 时跳过一次 `sfence.vma`，恢复
user SATP 仍刷新”的小补丁。它语义保守，但没有 workload 收益：

- exec 扩展样本中位数从约 860.7 ms 增到 931.3 ms，慢 8.2%；
- 54.9 MB 冷读中位数从 735 ms 增到 780 ms，慢 6.1%；
- perf cycles 虽约少 0.95%，但没有转成 wall-time 改善。

因此该小补丁没有进入提交。真正有效的是消除整个设备热路径的 SATP enter/exit，而不
是只省 enter 一侧的一条 fence。原始日志：

```text
.tmp/ablate/20260808-satp-exec-{base,candidate}-*/serial.log
.tmp/ablate/20260808-satp-cold-{base,candidate}-*/serial.log
.tmp/ablate/20260808-satp-perfstat-{base,candidate}-*.csv
```

## 对因提升

### 1. 54.9 MB 冷块读

每次从官方 root image 新建 qcow2 overlay，启动 RISC-V 8 hart/8 GiB，在 guest cache
全冷时用 4 KiB 请求读取连续文件
`/usr/bin/riscv64-linux-gnu-lto-dump-14`。14 组配对采用前半 B-C、后半 C-B 顺序，
每轮有 15 秒 host 硬截止：

| 指标 | 基线（n=14） | 候选（n=14） | 改善 |
| --- | ---: | ---: | ---: |
| guest elapsed 中位数 | 780 ms | 715 ms | **-8.33%** |
| 等价吞吐 | 1.00x | 1.091x | **+9.09%** |
| 配对结果 | — | 12 胜 / 1 平 / 1 负 | 方向稳定 |
| 成功运行 | 14/14 | 14/14 | 无回归 |

原始日志：

```text
.tmp/ablate/20260808-mmio-cold-base-{1..14}/serial.log
.tmp/ablate/20260808-mmio-cold-candidate-{1..14}/serial.log
.tmp/ablate/run_cold_io_expect.sh
```

### 2. exec/file-page-cache 回归门禁

`exec_file_page_cache_perf_smoke` 每个内核独立启动 10 次，每次包含 warmup 和 3 个
计时 round。候选赢 6/10 组，且全部 failures=0：

| 指标 | 基线（n=10） | 候选（n=10） | 差异 |
| --- | ---: | ---: | ---: |
| guest 中位数 | 894,905.5 us | 887,836.5 us | **-0.79%** |
| 成功运行 | 10/10 | 10/10 | 无回归 |

该 workload 离散度较大，所以只把它解释成“旧版 11%--13.5% 回退已经消失”，不把
0.79% 当成本批主要收益。原始日志：

```text
.tmp/ablate/20260808-mmio-exec-base-{1..10}/serial.log
.tmp/ablate/20260808-mmio-exec-candidate-{1..10}/serial.log
```

### 3. 240 秒 tg-xtask C-B-C-B 风险闸门

四轮都从独立 root/user qcow2 overlay 启动，guest 执行：

```sh
timeout 240 cargo build -p tg-xtask
```

runner 每 30 秒记录 output bytes/lines、`target/debug/deps/*.d`、xtask 大小和探针
延迟，每 2 秒记录 QEMU CPU、RSS、I/O 与 host 可用内存。guest timeout 之外还有
330 秒 host 硬截止，四轮都按计划停止，没有 OOM、panic 或探针超时。

先跑候选再跑基线时，候选实际从宿主盘读取 132 MB、基线只读 16 MB，最终分别为
31 与 32 deps，说明 host page cache 顺序足以反转一个小差距，不能直接比较。继续一次
C-B 形成暖缓存 C-B-C-B 后：

| 暖缓存 240 秒指标 | 基线 | 候选 | 差异 |
| --- | ---: | ---: | ---: |
| `target/debug/deps/*.d` | 33 | 35 | **+6.06%** |
| Cargo 输出字节 | 1,141 | 1,150 | +0.79% |
| Cargo 输出行 | 37 | 37 | 持平 |
| QEMU CPU ticks | 165,871 | 157,999 | **-4.75%** |
| QEMU peak RSS | 2,248,696 KiB | 2,216,072 KiB | **-1.45%** |
| QEMU host `read_bytes` | 1,257,472 | 5,824,512 | 候选读盘更多仍领先 |
| 结果 | rc=124，xtask=0 | rc=124，xtask=0 | 都按 240 秒停止 |

原始日志：

```text
.tmp/ablate/20260808-mmio-tg-gate-candidate/
.tmp/ablate/20260808-mmio-tg-gate-baseline/
.tmp/ablate/20260808-mmio-tg-gate-candidate-warm/
.tmp/ablate/20260808-mmio-tg-gate-baseline-warm/
```

可以证明的是：候选减少 RISC-V 设备热路径的 host CPU 工作，冷块读中位吞吐提升
9.09%，并让暖缓存 tg-xtask 240 秒内生成的 deps 增加 6.06%。不能声称完整
tg-xtask 已通过；240 秒时两边都离生成二进制很远，因此本批没有继续盲跑一小时。

## 对应提交

| 项目 | 值 |
| --- | --- |
| 顶层分支 | `dev_final` |
| `os/` 基线 | `da190f90640edc08de48f628da16f259fc5ca077` |
| `os/` 修复 | `9f06a1d882ded0624188e5bcaf8b325bcb263d45`（`riscv64: share high-half MMIO mappings`） |
| 顶层集成 | `34d97b6d`（`riscv64: integrate high-half MMIO mappings`） |
| 修复文件 | `irq.rs`、`trap.asm`、`config.rs`、`virtio_blk.rs`、`memory_set.rs`、`mm/mod.rs` |

共享工作树中 slab、file-backed exec、signal、scheduler 和其它工作人员的修改均未加入
该 `os/` 提交。生产提交保持 `DEBUG_PERF=false`、`DEBUG_SCHED=false`。

## 验证与复现

隔离 clean worktree 的最终提交树同时通过：

```sh
TMPDIR=/tmp/congcore-mmio-rv \
  ARCH=riscv64 cargo +nightly-2026-07-15 check --offline \
  --target riscv64gc-unknown-none-elf

TMPDIR=/tmp/congcore-mmio-la \
  ARCH=loongarch64 cargo +nightly-2026-07-15 check --offline \
  --target loongarch64-unknown-none-softfloat
```

| 资产 | 版本 |
| --- | --- |
| final source | `b5ec6ef8497e1818cbdec3b54bb722f036e57972`（`final-2026`） |
| RISC-V 镜像 SHA-256 | `d899fe43d333d1d17ad8a5f8a8b74b68117b8c1ceacfc3843bfeadb1ca705bd1` |
| QEMU | 11.0.3 |
| 架构 / SMP / 内存 | RISC-V64 / 8 / 8 GiB |
| Linux 参考源码 | `4549871118cf616eecdd2d939f78e3b9e1dddc48` |
| 基线内核 SHA-256 | `ac7c4497e3a9b69064724cd6604f0050539f754c6fcd590f3c3f166f3b225596` |
| 候选内核 SHA-256 | `7564892eff306c36628bfb4e4eec279c48d0f18a151f4e0d768db6a2ca46d258` |
| release 配置 | `codegen-units=1`，`DEBUG_PERF=false` |

## 当前边界与下一步

最后一处外部中断 guard 仍会为每次 IRQ 支付一次 enter/exit；它是当前完成队列语义的
安全边界，不应在没有 hardirq-safe 改造时删除。下一批应优先：

1. 把 block completion 的调度唤醒从 hardirq 中延后，删除最后一处 SATP guard；
2. 给 SATP switch、kernel shared shootdown 增加只在诊断内核开启的计数器；
3. 用 readiness marker 改造 perfmap runner，归档 map + ELF，再在 tg-xtask 单 crate
   阶段采样；
4. 短 gate 只有出现稳定增益后，才重新进入完整 RISC-V BuildStorm。

## AI 使用说明

本批使用 AI 辅助复核专家的 perf/日志证据、对照 Linux RISC-V pgd/ioremap 路径、
隔离另一位工作人员的候选补丁，并设计带硬超时的 RISC-V 稳定性、冷 I/O、exec、
perf stat 与 tg-xtask C-B-C-B。初版无 IRQ guard 的实现因间歇停滞被否决，小型 ASID 0
快路因 wall time 回退被否决；只有保留安全边界的最终六文件改动进入提交。所有数字均
可由上列 serial/probe/host/perf 文件复算，AI 判断未替代真实 guest 执行和双架构编译。
