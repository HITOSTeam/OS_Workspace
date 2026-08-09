# 8-9 公共 exit-to-user 与 new-PTE range publication

## 问题概述

公共 VM、调度和用户态返回语义曾分别复制在 RISC-V 与 LoongArch trap/MMU
实现中。两个直接后果是：

1. LoongArch 返回用户态前会对同一个 deferred scheduler tick 重复执行 runtime、
   `RLIMIT_CPU` 和抢占处理；RISC-V 只处理一次。
2. ready-only fault-around 一次最多安装 16 个新 PTE，但公共 fault 循环每页调用一次
   架构 hook。LoongArch 每次都执行发布屏障并刷新 8 KiB even/odd TLB pair，相邻两页
   会重复刷新同一 pair；公共代码还保留了两套 `target_arch` 分支。

这不是某条指令的局部快慢问题，而是公共语义与架构机制边界不稳定。BuildStorm 的
数百万次文件 fault 和用户态返回会把这种复制与重复工作持续放大。

## 如何发现

代码审计对照两个 trap handler 时确认了 LoongArch double tick：同一个
`deferred_scheduler_tick` 在相邻两个分支中被记账两次。fault-around 审计则发现
`fault.rs` 的安装循环逐页调用 `update_mmu_cache_for_new_pte()`。

带 `/proc/perf` 的双架构诊断进一步量化了 range 形状：

| 指标 | RISC-V 8 hart | LoongArch 12 hart |
| --- | ---: | ---: |
| `tlb_new_pte_batches` | 32,130 | 33,406 |
| installed PTE pages | 59,762 | 60,008 |
| publication range pages | 67,101 | 66,933 |
| LoongArch unique 8 KiB pairs | 0 | 49,286 |
| RISC-V Svvptc skips | 32,103 | — |
| RISC-V new-PTE fences | 0 | — |

日志：

```text
.tmp/fault-around/20260809-common-exit-newpte-riscv-diag-256/serial.log
.tmp/fault-around/20260809-common-exit-newpte-loongarch-diag-257/serial.log
```

## Linux 对照

本地 Linux 参考树为 `exampleOs/linux` commit
`4549871118cf616eecdd2d939f78e3b9e1dddc48`。

- Linux entry/exit 层把 signal、resched、timer/irq-exit 等返回用户态工作放在公共
  语义层，架构入口只保存寄存器和解码硬件事件。
- `update_mmu_cache_range(vmf, vma, addr, ptep, nr)` 以范围而非单页表达新映射发布。
- missing-to-present 不替换旧 frame，远端 hart 最多经历一次可恢复的 spurious fault，
  不需要同步全机 shootdown；replace、permission downgrade 与 unmap 仍必须在旧 frame
  可复用前完成失效。
- RISC-V 有 Svvptc 时 new-PTE publication 不需要 `sfence.vma`；LoongArch 一个 TLB
  entry 覆盖 even/odd 两页，因此应按唯一 8 KiB pair 合并处理。

## 怎么解决

### 1. 建立唯一的公共 exit-to-user 路径

新增 `trap::exit_to_user_mode_loop()`，按固定顺序处理：

```text
fatal signal check
  -> consume deferred scheduler tick once
  -> timer work
  -> runtime / rlimit / tick preemption
  -> signal delivery
  -> cgroup block
  -> syscall-return preemption
  -> NEED_RESCHED
```

两个架构 trap handler 只传入 `syscall_return` 并调用该公共函数。LoongArch 原先重复的
tick 记账被结构性消除，以后修改返回语义也不会再复制两份。

### 2. 使用 new-PTE publication transaction

`NewPtePublicationBatch` 记录一次 fault commit 真正安装的页数以及最小/最大 VA；所有
PTE store 完成后只调用一次统一的
`arch::update_mmu_cache_for_new_pte_range()`。Drop 仍会完成 publication，避免未来错误
返回路径遗漏硬件发布。

这条 transaction 只表示 `NewPresent`。已有 PTE replacement、mprotect demotion、unmap
和 frame 延迟释放仍使用原有 `PageTableUpdateBatch` 同步 shootdown，不降低正确性。

### 3. 架构层只实现机制与能力

- RISC-V：Svvptc 下记录 skip 并返回；否则一次本地 ASID range flush，不发远程 IPI。
- LoongArch：所有 PTE 发布后只执行一组前后 barrier；range 对齐到 8 KiB pair，
  每个 pair 最多处理一次。
- 公共 MM 通过 `crate::arch` facade 使用 `AsidContext`、TLB batch 与 new-PTE hook，
  不再直接引用架构模块。

## 对应提交

| 项目 | 值 |
| --- | --- |
| 基线 | `bd250cd879cb13ba6afe9ff3d12b1ee26573f2ef` |
| 公共 exit | `84874af15fba3e905e4e9fde6ad1b7328a422fe1` |
| range publication | `9cbde1a48ae50693be0775271beac5722c04673e` |
| 提交标题 | `trap: share exit-to-user work` / `mm: batch new PTE publication` |

## 对因提升

production 内核关闭 perfmap 与 `DEBUG_PERF`，每次独立启动，顺序为 B-C-C-B。测试为
`exec_file_page_cache_perf_smoke`，表中取两次启动各自中位数的中位值：

| 架构 | 基线 | 候选 | 延迟变化 | 等价吞吐 |
| --- | ---: | ---: | ---: | ---: |
| RISC-V 8 hart | 222,397 us | 215,379 us | **-3.16%** | **+3.26%** |
| LoongArch 12 hart | 415,311 us | 400,104 us | **-3.66%** | **+3.80%** |

原始日志：

```text
.tmp/fault-around/20260809-p1-riscv-prod-bccb-{baseline-267,candidate-268,candidate-269,baseline-270}/
.tmp/fault-around/20260809-p1-loong-prod-bccb-{baseline-271,candidate-272,candidate-273,baseline-274}/
```

## 回归验证

- RISC-V MM/fault 运行态 7/7：file mmap、private/shared page cache、madvise、truncate、
  executable I-cache、exec page cache。
- LoongArch MM/TLB 运行态 9/9：上述共同集合加 local-TLB、ASID wrap、SMP shootdown。
- 最终公共返回路径：RISC-V 5/5、LoongArch 4/4，覆盖 timerfd、signal frame、
  wait/wakeup、并发 spawn；RISC-V 额外覆盖 mq notify。
- 两架构 `cargo check`、`cargo fmt -- --check`、whitespace check 通过；仅有既有 warning。

对应日志：

```text
.tmp/fault-around/20260809-common-exit-newpte-riscv-focus-253/serial.log
.tmp/fault-around/20260809-common-exit-newpte-loongarch-focus-255/serial.log
.tmp/sched64-runs/20260809-stable-exit-riscv-focus-327/serial.log
.tmp/sched64-runs/20260809-stable-exit-loong-focus-328/serial.log
```

## 双架构门禁拦截的回退

后续尝试过 clocksource mult/shift、盲目 `NEED_RESCHED` IPI 合并以及 fair queue u128→u64
重写。完整候选在 RISC-V 的 fork/yield 微基准分别改善 4.92%/2.78%，但 LoongArch fork
稳定回退 15.56%，因此全部撤回。IPI 合并还缺 Linux 的 idle/polling 状态，不能只凭
false→true 原子转换就认定远端无需 IPI。

```text
.tmp/sched64-runs/20260809-{loong,riscv}-sched64-bccb-*/serial.log
.tmp/sched64-runs/20260809-loong-{time-only,time-relaxed,ipi-only}-ablation-*/serial.log
```

这次负结果说明公共热路径必须比较每个架构相对自身 parent 的变化，不能把 RISC-V 的
成本模型直接外推到 LoongArch。
