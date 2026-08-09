# 8-9 RISC-V 按 Svvptc 省略新 PTE 本地屏障

## 问题概述

上一轮已经把 missing PTE 变成 valid PTE 的刷新从跨核 SBI RFENCE 缩小为 faulting hart
上的 `sfence.vma addr, asid`。这解决了远端 shootdown，但在支持 Svvptc 的 RISC-V CPU
上，本地屏障仍然多余。

Svvptc 规定了地址转换缓存对 invalid PTE 的行为：软件把无效 PTE 发布为有效 PTE 后，
新映射会在有界时间内变得可见。支持该扩展时，内核不必为每个新 PTE 主动执行
`sfence.vma`；不支持时仍保留原来的本地刷新。替换已有映射、收紧权限、unmap 和 ASID
复用仍可能留下旧 valid translation，不属于这条快路。

BuildStorm 的 exec、动态链接、mmap 和匿名内存首次触页会发布大量新 PTE。在 RISC-V
TCG 下，一条 guest `sfence.vma` 还会使 QEMU 的 softmmu TLB 和翻译块跳转缓存变冷，
因此这个看似很小的屏障会被数十万次 page fault 放大。

## 如何发现

### 1. `/proc/perf` 证明新 PTE 本地刷新仍是大账

在同一诊断内核、同一官方镜像的 120 秒 BuildStorm gate 中，仅通过 QEMU CPU 属性切换
Svvptc。开启侧明确打印：

```text
[kernel] riscv svvptc enabled
```

末次探针得到：

| 指标 | Svvptc 开启 | Svvptc 关闭 |
| --- | ---: | ---: |
| `tlb_new_pte_svvptc_skips` | 204,670 | 0 |
| `tlb_new_pte_fences` | 0 | 208,576 |
| ext4 cache loads | 27,406 | 27,513 |
| block submitted | 28,141 | 28,255 |
| heap allocation failures | 0 | 0 |

两侧实际文件系统工作量相差不到 1%，但刷新路径完全按能力位分开，证明探测结果确实进入
了新 PTE 热路径。该 gate 的 Cargo 输出分别为 424 B 和 434 B，且都没有生成 deps；它
只证明机制命中，不能作为 BuildStorm 提速结论。

### 2. perf 只用于解释热点，不用于本批耗时结论

上述两轮都启用了 `-perfmap`，从第一个探针开始采集 60 秒，`Total Lost Samples=0`。
基线的 `tb_gen_code` self 占比为 9.03%，候选为 7.54%；基线还执行了约 20.9 万次本地
新 PTE fence。方向与 QEMU 翻译缓存反复变冷的判断一致。

不过 `-perfmap` 本身会让 `perf_report_code`、libdw 和 libelf 进入热路径，因此这两轮
不用于精确计时。性能证明改用关闭 `-perfmap` 的同硬件消融。

### 3. 修正最初 A/B 的硬件混杂因素

最初直接比较 `-cpu rv64,svvptc=true` 和 `svvptc=false`，同时改变了两件事：

- QEMU 模拟的 invalid-PTE cache 行为；
- 内核是否跳过 `sfence.vma`。

这组测试虽显示约 30%--36% 的延迟差异，却不能把全部收益归因于内核分支。最终对因
A/B 将两侧 QEMU 都固定为 `rv64,svvptc=true`，只在临时消融内核中让
`has_svvptc()` 返回 false。测试后立即恢复源码，正式工作树不含该消融。

## Linux 对照

本批核对本地 `exampleOs/linux`：

```text
commit 4549871118cf616eecdd2d939f78e3b9e1dddc48
```

Linux `arch/riscv/include/asm/pgtable.h::update_mmu_cache_range()` 的规则是：

1. 若全局 ISA bitmap 包含 `RISCV_ISA_EXT_SVVPTC`，直接返回；
2. 否则逐页执行 local TLB flush，避免缓存 invalid entry 的实现产生额外 spurious fault。

`arch/riscv/kernel/cpufeature.c` 分别解析标准 `riscv,isa-extensions` 和兼容用的旧
`riscv,isa`，并对所有可用 hart 的能力 bitmap 做交集。只有全部可运行 hart 都支持的
扩展才进入 host ISA bitmap。设备树绑定对 `svvptc` 的描述位于
`Documentation/devicetree/bindings/riscv/extensions.yaml`。

CongCore 没有 Linux 完整的 cpufeature bitmap，本批采用等价的最小机制：遍历 FDT 中
所有本内核可管理且状态为 available 的 CPU node，只有每个 node 都精确声明 Svvptc
时才打开全局能力位。

## 怎么解决

### 1. 严格解析每个可用 hart 的 ISA 属性

`os/src/arch/riscv64/mod.rs` 增加 Svvptc 启动探测：

- 优先解析标准的 NUL 分隔 `riscv,isa-extensions` 字符串列表；
- 缺少标准属性时兼容旧 `riscv,isa`；
- 旧 ISA 字符串按下划线分隔多字母扩展，并忽略合法的 major/minor 版本后缀；
- 使用完整扩展名比较，不用可能误命中的子串搜索；
- 跳过 `status` 明确不是 `okay`/`ok` 的 CPU；
- 任一可用 hart 缺少该能力就保持关闭。

同一批还让 `hart_topology_from_dtb()` 使用相同的 available 规则，避免能力探测和实际
上线 hart 集合不一致。

### 2. 只省略 missing-to-valid 的本地刷新

`update_mmu_cache_for_new_pte()` 在当前 ASID 仍有效且系统支持 Svvptc 时直接返回；否则
保持 PTE write barrier 和 `sfence.vma addr, asid`。调用范围没有扩大：

- missing PTE 发布可走快路；
- executable PTE 仍在发布前完成原有 I-cache 同步；
- replacement、permission downgrade、unmap 和 ASID rollover 仍走原有失效协议；
- 不支持 Svvptc 的硬件继续使用保守本地 fence。

### 3. 增加可关闭的对因计数

`DEBUG_PERF=true` 时 `/proc/perf` 新增：

```text
tlb_new_pte_fences
tlb_new_pte_svvptc_skips
```

正式源码已恢复 `DEBUG_PERF=false`。计数器只用于诊断，不进入正式成绩配置。

## 对应提交

| 项目 | 值 |
| --- | --- |
| `os/` 基线 | `b18fffbd7439d22571cf530b21786fb08bce62e9` |
| `os/` 修复 | `71da5616ef74cec66a04a8f08ba241c144317458` |
| 提交标题 | `riscv64: skip new-PTE fences with Svvptc` |
| 顶层集成 | 本说明文档所在提交 |

## 对因提升

### 精确同硬件 B-C-C-B 消融

测试固定为 QEMU 11.0.3、RISC-V64、8 hart、8 GiB、`-cpu rv64,svvptc=true`，每次从
相同 raw backing 创建独立 qcow2 overlay。候选和消融基线都为 `DEBUG_PERF=true`，只差
`has_svvptc()` 是否被临时强制为 false：

| 内核 | SHA-256 |
| --- | --- |
| 正常 Svvptc 候选 | `a2af8076601acffbefc2240be140c1d7823cc86005dbb8788c8af5fa6b04f8a3` |
| 强制 fence 消融基线 | `24013664cfe3960b60f601cc53deecb3cb127d02f9cb9fa7a8a50a9ea701b00e` |

顺序为 baseline -> candidate -> candidate -> baseline。每次启动后连续执行 7 次
`exec_file_page_cache_perf_smoke`；每次测试自身先 warmup，再运行 3 个计时 round 并报告
中位数。合并两次启动后每侧各有 14 个外层样本：

| 指标 | 强制 fence 基线 | Svvptc 候选 | 改善 |
| --- | ---: | ---: | ---: |
| 样本数 | 14 | 14 | 相同 |
| 跨样本中位数 | 337,886.5 us | 224,516.5 us | **-33.55%** |
| 等价吞吐 | 1.000x | 1.505x | **+50.50%** |
| 配对胜负 | — | 14 胜 / 0 负 | 方向一致 |
| failures | 0 | 0 | 无回归 |

两对单独计算时，候选延迟分别下降 33.95% 和 33.16%，说明结果不是由运行顺序或单次
启动 hart 造成。这个测试证明的是 exec/file-page-cache 缺页路径的对因收益，不能直接
外推为完整 BuildStorm 总用时下降 33.55%。

原始结果：

```text
.tmp/ablate/20260809-svvptc-exact-base-218/results.csv
.tmp/ablate/20260809-svvptc-exact-candidate-219/results.csv
.tmp/ablate/20260809-svvptc-exact-candidate-reverse-220/results.csv
.tmp/ablate/20260809-svvptc-exact-base-reverse-221/results.csv
```

机制 gate 与 perfmap 资产：

```text
testsuits-final/.tmp/final-runs/20260809-riscv-svvptc-on-gate-211/
testsuits-final/.tmp/final-runs/20260809-riscv-svvptc-off-gate-212/
```

## 回归验证

Svvptc 开启路径完成 7/7 RISC-V 运行态回归：

| 测试 | 结果 | 覆盖边界 |
| --- | ---: | --- |
| `lazy_fault_local_tlb_smoke` | PASS | 并发 missing PTE 与 spurious fault 恢复 |
| `cow_mprotect_smoke` | PASS | 已有映射权限变化 |
| `tlb_shootdown_smp_smoke` | PASS | 旧 valid PTE 仍跨核失效 |
| `file_mmap_lazy_fault_smoke` | PASS | 普通文件 lazy fault |
| `riscv_icache_smp_smoke` | PASS | 128 次更新乘 7 个远端 hart 的 I-cache 同步 |
| `memfd_mremap_shared_smoke` | PASS | 共享映射与 remap |
| `stack_madvise_dontneed_smoke` | PASS | 丢弃后重新缺页 |

日志：

```text
.tmp/ablate/20260809-svvptc-regressions-on-217/serial.log
```

双架构 `cargo check`、RISC-V release build、`cargo fmt --all -- --check` 和
`git diff --check` 均通过，只有仓库已有 warning。LoongArch64 不进入 Svvptc 代码，
本批做了编译检查，没有新增 LoongArch64 运行态结果。

## 当前边界

- 120 秒 BuildStorm gate 尚未生成 `tg-xtask` 或 deps，因此本批不声称完整 BuildStorm
  已通过，也不继续盲跑长测。
- OpenSBI 启动摘要不一定列出 Svvptc；能力来源是传给内核的 FDT。当前 QEMU FDT 的
  8 个 hart 均声明该扩展，`svvptc=false` 时声明会消失。
- 当前拓扑在启动时确定，不实现 Linux CPU hotplug 后重新计算全局 ISA 交集的完整机制。
  CongCore 当前没有运行时 CPU hotplug，这不影响现有边界。
- `-perfmap` 很适合定位 TCG 热点，但会显著改变翻译块生成成本；精确耗时必须关闭它，
  并使用同硬件模型的源码消融。

## AI 使用说明

本批使用 AI 辅助持续监控带硬截止的 BuildStorm、归档精确 PID perfmap、解析
`/proc/perf`，核对 Linux `update_mmu_cache_range()` 与 cpufeature 的全 hart 交集规则，
并设计同硬件 B-C-C-B 消融。初版硬件属性 on/off 对比因同时改变 QEMU 行为而被降级为
机制证据；最终性能结论只采用两侧均启用 Svvptc、仅改变内核分支的 28 个真实 guest
样本。所有临时消融均在构建后撤销，产品源码保持 `DEBUG_PERF=false`。
