# 8-8 RISC-V Linux 风格新建可执行 PTE 刷新

## 问题概述

RISC-V lazy fault 在安装普通数据页时，已经采用“发布新 PTE 后只刷新当前 hart”的
路径；但只要新 PTE 带 `X`，旧实现就改走完整的 `PageTableUpdateBatch`：

1. 进入地址空间失效事务；
2. 记录 fault page；
3. 同步本地及远端 instruction cache；
4. 提交事务，对所有 resident hart 发同步 SBI RFENCE；
5. 等待远端 TLB shootdown 完成。

这里混淆了两个不同需求：新装入的指令字节确实需要 `fence.i`，但 fault 前的 PTE 是
missing/invalid，远端 hart 不存在需要删除的旧 valid translation。BuildStorm 的 exec、
动态链接和 rustc 代码页缺页把这个错误放大成数十万次跨核失效。

本批把两件事拆开：可执行页仍按 Linux 顺序在发布 PTE 前同步 instruction cache；PTE
发布后只对 faulting hart 执行 `update_mmu_cache_for_new_pte()`。替换、降权、unmap 等
真正可能存在旧翻译的路径继续使用原有跨核事务。

## 如何发现

### 1. 先否决“删除最后一个 IRQ SATP guard”假设

宿主 perf 仍以 `helper_lookup_tb_ptr`、RISC-V TLB fill 和 QEMU TB hash lookup 为热点，
最初怀疑外部中断入口保留的最后一个 `KernelPageTableGuard` 是主因。候选在强制
`COMPLETION_POLL_SPINS=0` 后通过 7 项 block/wakeup/exec 回归，但对因数据不支持采用：

- 精确同源的 54.9 MB 冷读 4 组配对，中位数仅从 700 ms 变为 695 ms，约 **0.7%**；
- 300 秒 BuildStorm 从基线 `58 deps / 2144 bytes` 变为
  `54 deps / 2116 bytes`，没有改善；
- `helper_lookup_tb_ptr` children 占比仅从约 43.39% 降到 41.62%，减少的 host 慢路径
  没有转成 guest 吞吐。

因此该候选已完整撤销。最后一个 IRQ guard 仍是当前 block completion/wakeup 尚未完全
hardirq-safe 时的安全边界，不能仅凭 host perf 热点删除。

对应现场：

```text
testsuits-final/.tmp/final-runs/20260808-riscv-no-irq-satp-spin0-targeted-188/
testsuits-final/.tmp/final-runs/20260808-riscv-buildstorm-no-irq-satp-short-189/
.tmp-os-noirq-ab-190/
```

### 2. `/proc/perf` 定位到可执行页 fault

随后在隔离源码快照中临时打开 `DEBUG_PERF`，对同一镜像、同一 120 秒 BuildStorm
分别构建精确基线和只改 executable-new-PTE 路径的候选。基线末次探针为：

```text
output_bytes=989
deps_count=23
tlb_page_batches=549222
tlb_remote_ipis=1105800
tlb_shootdown_wait_cycles=364526297
icache_local_fences=471328
```

RISC-V timebase 为 10 MHz，因此 shootdown wait counter 累计约 36.45 秒。它是多个
hart 的等待周期总账，不能直接当成单线程 wall time，但足以说明同步远端失效是热账。
`tlb_page_batches` 与 `icache_local_fences` 同阶，又只在 executable fault 分支出现，继续
对照源码后定位到 `os/src/mm/memory_set/fault.rs` 的 executable batch。

候选在完成更多编译工作的情况下，跨核 TLB 计数反而大幅下降：

| 120 秒指标 | 精确基线 | 候选 | 差异 |
| --- | ---: | ---: | ---: |
| `target/debug/deps/*.d` | 23 | 31 | **+34.8%** |
| Cargo 输出字节 | 989 | 1,245 | **+25.9%** |
| `tlb_page_batches` | 549,222 | 98,334 | **-82.1%** |
| `tlb_remote_ipis` | 1,105,800 | 135,633 | **-87.7%** |
| `tlb_shootdown_wait_cycles` | 364,526,297 | 39,202,585 | **-89.2%** |

候选同时完成了更多 block I/O、dcache lookup 和 SATP switch，所以这些下降不能由“少做
了工作”解释。两轮都没有 OOM、panic 或探针停滞。原始日志：

```text
testsuits-final/.tmp/final-runs/20260808-riscv-buildstorm-perfdiag-191/serial.log
testsuits-final/.tmp/final-runs/20260808-riscv-buildstorm-exec-pte-local-perfdiag-192/serial.log
```

正式内核已经恢复 `DEBUG_PERF=false`；诊断计数仅用于因果定位，不进入正式跑分配置。

### 3. host perf 的解释边界

最终生产配置 300 秒 gate 在 workload 运行中采集了 15 秒宿主 perf，
`helper_lookup_tb_ptr` children 仍占约 40.96%，随后是 `riscv_cpu_tlb_fill`、
`tlb_set_page_full` 和 `qht_lookup_custom`。这说明 RISC-V TCG 翻译缓存仍有优化空间；
但该样本发生在候选已经领先基线的不同编译阶段，不能拿占比变化代替上面的精确源码
A/B。后续仍应以 `/proc/perf` 对因计数和固定 workload 进度决定是否采用改动。

## Linux 对照

本批核对本地 `exampleOs/linux` commit
`4549871118cf616eecdd2d939f78e3b9e1dddc48`：

- `arch/riscv/include/asm/pgtable.h::__set_pte_at()` 在 present+exec PTE 上先调用
  `flush_icache_pte(mm, pte)`，再以 `set_pte()` 发布 PTE；
- `arch/riscv/mm/cacheflush.c::flush_icache_pte()` 通过 per-folio
  `PG_dcache_clean` 避免对未被再次写脏的指令页重复同步，并由 `flush_icache_mm()` 处理
  active remote hart 与 deferred hart；
- `arch/riscv/include/asm/pgtable.h::update_mmu_cache_range()` 在发布新 valid PTE 后只执行
  local `sfence.vma`，用于不保证 Svvptc、可能缓存 invalid entry 的实现；
- 只有替换、撤销权限或 unmap 等存在旧 valid translation 的操作才需要远端 TLB
  shootdown。

CongCore 目前没有 Linux 的 folio `PG_dcache_clean` 元数据，所以本批保留已有的每次
executable fault instruction-cache 同步语义，不顺手扩大成 frame metadata 重构。关键是
严格复用 Linux 的操作顺序，并把 I-cache coherency 与 TLB invalidation 分离。

## 怎么解决

修改 `os/src/mm/memory_set/fault.rs` 的 missing-PTE commit 顺序：

1. 若新 PTE 带 `X`，先调用 `mark_icache_stale(self.asid.as_ref())`；
2. 调用 `page_table.map()` 发布新 PTE；
3. 完成 frame/VMA bookkeeping；
4. RISC-V 无条件调用 `update_mmu_cache_for_new_pte(asid, fault_va)`，只刷新 faulting hart；
5. 删除 executable 分支的 `begin_page_table_update()`、`record_page()` 和 `commit()`。

这样同时满足：

- 指令内容先于 executable PTE 对其它 hart 可见；
- active remote hart 立即 `fence.i`，inactive hart 通过 per-mm stale marker 延后消费；
- faulting hart 在 PTE 发布后执行 local address+ASID `sfence.vma`；
- 远端 hart 即使发生并发 spurious fault，也会在既有 valid-PTE recheck 中自然返回；
- replacement/unmap 的原有同步 TLB transaction 完全不变。

这是机制级修复，没有按 cargo、rustc、BuildStorm 进程名或测试名添加特判。

## 对因提升

### 1. 精确同源 120 秒诊断 A/B

上表是本批的主要性能证明：两个隔离内核只差 executable-new-PTE 刷新顺序。候选在
120 秒内生成 deps 数增加 34.8%，同时将同步远端 TLB 的三项核心指标降低
82.1%--89.2%。

### 2. 最终生产配置 300 秒风险闸门

最终 `DEBUG_PERF=false` 内核使用独立 qcow2 overlay，runner 每 30 秒检查输出、deps、
shell 延迟和 guest 内存，每 2 秒记录 QEMU CPU/RSS/I/O，并在 300 秒硬截止主动终止：

| 300 秒指标 | 旧生产基线 | 最终生产内核 | 差异 |
| --- | ---: | ---: | ---: |
| `target/debug/deps/*.d` | 58 | 68 | **+17.2%** |
| Cargo 输出字节 | 2,144 | 2,527 | **+17.9%** |
| 平均 QEMU CPU | 7.163 核 | 7.236 核 | +1.0% |
| QEMU peak RSS | 2,283.0 MiB | 2,363.3 MiB | +80.3 MiB |
| 最大 probe latency | 755 ms | 744 ms | 无退化 |
| panic / OOM | 0 / 0 | 0 / 0 | 无回归 |

最终内核推进得更多，RSS 与写 I/O 略高符合更多 crate 已进入编译的阶段差异；宿主最低
可用内存仍为约 15.75 GiB。旧生产基线与最终工作树之间还包含其它工作人员的并行改动，
因此这张表只作为最终安全性和方向性门禁，严格因果结论以上一节隔离 A/B 为准。

300 秒时 `tg-xtask` 尚未生成，runner 按约定停止；本批不声称完整 BuildStorm 或 xtask
已经通过，也没有继续盲跑数十分钟。日志和资产：

```text
testsuits-final/.tmp/final-runs/20260808-riscv-buildstorm-short-184/
testsuits-final/.tmp/final-runs/20260808-riscv-buildstorm-exec-pte-local-short-194/
```

### 3. 生产回归

同一个 production kernel 完成 7/7 RISC-V 定向回归：

| 测试 | 结果 |
| --- | --- |
| `wait_wakeup_race_smoke` | 256 iterations，PASS |
| `concurrent_pread_smoke` | 8 workers / 128 reads，PASS |
| `signal_frame_fault_smoke` | 64 iterations，PASS |
| `exec_file_page_cache_perf_smoke` | median 506,897 us，failures=0 |
| `socketpair_exit_eof_smoke` | stream + seqpacket/CLOEXEC，PASS |
| `lazy_fault_local_tlb_smoke` | PASS |
| `riscv_icache_smp_smoke` | 128 updates × 7 remote harts，PASS |

其中最后两项直接覆盖本批改变的 local TLB 和跨 hart instruction-cache 语义。日志：

```text
testsuits-final/.tmp/final-runs/20260808-riscv-exec-pte-local-regressions-193/serial.log
```

## `-perfmap` 采集规范

本机 `qemu-system-riscv64 -help` 明确提供：

```text
-perfmap        generate a /tmp/perf-${pid}.map file for perf
```

以后所有需要解释 QEMU TCG 热点的诊断轮，应在 QEMU 参数中加入 `-perfmap`。正确流程是：

```sh
qemu-system-riscv64 ... -perfmap &
qemu_pid=$!

# 等 guest 明确打印 workload readiness marker 后再采样，不能用固定 sleep 猜阶段。
perf record -F 999 -g -p "$qemu_pid" -o perf.data -- sleep 15

# 必须在 QEMU 退出前归档；同时保存与该轮完全匹配的 kernel ELF。
cp "/tmp/perf-${qemu_pid}.map" "$run_dir/"
cp "$kernel_elf" "$run_dir/kernel.elf"
```

一次可复核的 perf 资产至少包括：

```text
perf.data
perf-<qemu-pid>.map
kernel.elf
serial.log
probe-latency.csv
host-metrics.log
```

如果 map 或匹配的 ELF 缺失，只能把报告解释为 host/TCG 层热点，不能把匿名 JIT PC
强行归因到某个 guest 函数。本批生产 gate 已保存 `perf.data` 与 `kernel.elf`，但没有
启用 `-perfmap`，所以只使用其 host 符号；主要因果结论来自完整的 `/proc/perf` A/B。

## 对应提交

| 项目 | 值 |
| --- | --- |
| `os/` 基线 | `9f06a1d882ded0624188e5bcaf8b325bcb263d45` |
| `os/` 修复 | `14bd76d`（`riscv64: avoid remote shootdown for executable new PTEs`） |
| 顶层集成 | 本说明文档所在提交 |

`os/` 提交只包含 `src/mm/memory_set/fault.rs` 中 executable-new-PTE 的 hunk；该文件
中另一位工作人员并行进行的 file-backed mmap 修改仍留在共享工作树，没有混入提交。

## 验证与资产

双架构检查均通过：

```sh
TMPDIR=$PWD/.tmp \
  CARGO_TARGET_DIR=$PWD/.tmp/check-rv-exec-pte-local-193 \
  ARCH=riscv64 CARGO_NET_OFFLINE=true \
  cargo +nightly-2026-07-15 check --manifest-path os/Cargo.toml \
  --target riscv64gc-unknown-none-elf

TMPDIR=$PWD/.tmp \
  CARGO_TARGET_DIR=$PWD/.tmp/check-la-exec-pte-local-193 \
  ARCH=loongarch64 CARGO_NET_OFFLINE=true \
  cargo +nightly-2026-07-15 check --manifest-path os/Cargo.toml \
  --target loongarch64-unknown-none-softfloat
```

`rustfmt --check` 和 diff whitespace check 同时通过。正式配置为
`DEBUG_PERF=false`、`DEBUG_SCHED=false`、`DEBUG_WATCHDOG=false`。

| 资产 | 值 |
| --- | --- |
| 顶层分支 | `dev_final` |
| 顶层集成基线 | `61b735ea` |
| `os/` 修复提交 | `14bd76d` |
| Linux 参考源码 | `4549871118cf616eecdd2d939f78e3b9e1dddc48` |
| 架构 / SMP / 内存 | RISC-V64 / 8 / 8 GiB |
| production kernel SHA-256 | `4b488e93dce90aa036fc374558e2296bb128bef74075b85a8c88d1c9600b2613` |
| production 配置 | release，`DEBUG_PERF=false` |

## 当前边界与下一步

本批只修正“missing executable PTE 被误当成旧翻译失效”的语义。当前 host perf 仍显示
TCG TLB fill/TB lookup 是大户，诊断内核也仍有 98,334 个 page batch；下一轮应继续用
perf 与现有 TLB counter 按调用路径归因，而不是继续猜测 QEMU：

1. 仿照 Linux `PG_dcache_clean`，评估给 file-cache frame 增加 instruction-clean 状态，
   避免未被写脏的同一代码页重复 `fence.i`；
2. 将剩余 `tlb_page_batches` 按 lazy fault、COW、unmap、mprotect/munmap 分类计数；
3. block completion 真正 hardirq-safe 后，再重新评估最后一个 IRQ SATP guard；
4. 只有短 gate 再次显示稳定收益，才运行完整 RISC-V BuildStorm。

## AI 使用说明

本批使用 AI 辅助持续监控 BuildStorm、解析 `/proc/perf` 与 host perf、隔离精确源码 A/B、
核对 Linux RISC-V `flush_icache_pte()`/`update_mmu_cache_range()` 顺序，并设计带硬截止的
RISC-V 回归和生产 gate。删除 IRQ guard 的候选因无 workload 收益被撤销；只有经过
精确计数证明、双架构编译和 RISC-V I-cache/TLB 回归的刷新顺序改动保留。所有结论均
来自上述可复查日志和真实 guest 执行。
