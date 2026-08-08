# 8-8 参考 Linux 把 mprotect 从全表重建改为局部 VMA 更新

## 问题概述

RISC-V BuildStorm 在编译 `tg-xtask` 时仍明显慢于 LoongArch。它不是死锁：guest shell 探针持续可用，依赖文件持续生成，QEMU 也仍在消耗 CPU。`-perfmap` 样本最终把热点定位到 `MemorySet::mprotect_user_range_inner()`：每次修改一个很小的地址区间，旧实现都会：

1. `mem::take()` 取走完整 `self.areas`；
2. 遍历、搬移地址空间中的每个 `MapArea`；
3. 新建完整 `Vec<MapArea>`；
4. 对整个列表重新排序。

BuildStorm 中 rustc/linker 会频繁修改映射权限。随着 VMA 数量增加，这条路径退化为每次 mprotect 都做 O(全部 VMA) 的复制、重分配和排序。

LoongArch 也会执行这段通用 VMA 代码，所以不是完全没有问题；只是它通过 DMW/IOCSR 访问内存和设备，没有 RISC-V 上 `sfence.vma`、SBI RFENCE 以及 QEMU softmmu/TB cache 被打冷的放大链。因此同一份元数据低效在 LoongArch 上主要表现为额外 CPU 工作，在 RISC-V TCG 上更容易成为主热点。

## 如何发现

诊断轮使用 QEMU `-perfmap`，并在终止 QEMU 前先 `SIGSTOP`、归档精确 PID 对应的 `/tmp/perf-<pid>.map`，避免 map 与 `perf.data` 不匹配：

```text
testsuits-final/.tmp/final-runs/20260808-riscv-buildstorm-exec-pte-perfmap-full-199/
```

60 秒 perf 样本中：

- `helper_lookup_tb_ptr` children/self 为 41.60% / 13.57%；
- guest `__rust_realloc` 对应 PC `0x80373876` children 为 5.29%；
- `0x803ab0fe`、`0x803ab0ca`、`0x803ab0ea` 分别为 2.44%、2.38%、2.30%，反汇编和 `addr2line` 均落在 `mprotect_user_range_inner()` 搬移 0x58-byte `MapArea` 的循环内。

因此 QEMU 的翻译查找热点是结果，不是根因：guest 内核反复重建 VMA Vec，进而制造大量分配、复制和 TCG 翻译/TLB 冷启动工作。

Linux 对照使用本地 `exampleOs/linux` commit `4549871118cf`：

- `mm/mprotect.c:do_mprotect_pkey()` 用 `vma_iter_init(..., start)` 从目标地址开始，随后只用 `for_each_vma_range(..., end)` 遍历相交 VMA；
- `mprotect_fixup()` 调用 `vma_modify_flags()`；
- `mm/vma.c:vma_modify()` 先尝试和相邻 VMA 合并，仅在目标边界切入现有 VMA 时拆分前段或后段；
- VMA 存在 Maple Tree 中，不会为了修改一个区间复制整个地址空间的 VMA 集合。

## 怎么解决

在现有有序、互不重叠的 `Vec<MapArea>` 过渡结构上实现 Linux 式局部更新：

1. 用两次 `partition_point()` 二分定位可能与 `[start, end)` 相交的最小切片；
2. 若 mprotect 边界已经与现有 `MapArea` 对齐，直接原地更新该切片，不分配临时 Vec、不搬移无关 VMA、不全表排序；
3. 只有首尾边界切入某个 area 时，才 drain 相交切片并局部 split，再 splice 回原位置；
4. 把 PTE 权限切换提取为 `update_map_area_permissions()`，保留 PROT_NONE 的 saved flags、延迟 TLB shootdown、RISC-V 新增可执行权限时的 icache 同步等原有语义；
5. 继续依赖原有有序/无重叠 debug invariant，并用跨核 TLB、COW、lazy mmap、mremap 和 RISC-V icache 回归覆盖边界行为。

这仍是过渡实现：边界 split 时 Vec 尾部可能被 splice 搬移。长期更好的方案是像 Linux Maple Tree 一样使用区间索引，使查找、拆分和合并都保持局部。

## 对应提交

- `os/` 提交：`6c0752901c7fdf7075f616b6227bcad74f37fe7c`（`mm: update mprotect areas locally`）。
- `os/` 基线：`14bd76d6731a0144043b4faa0f7ed0c7b030db91`。

## 对比提升

### 300 秒无 perfmap BuildStorm A/B

同一 QEMU 参数、同一基础镜像、相同 300 秒硬截止：

| 指标 | 修改前 run 194 | 修改后 run 201 | 改善 |
| --- | ---: | ---: | ---: |
| guest uptime | 300.51 s | 301.28 s | 基本一致 |
| deps 文件 | 68 | 74 | **+8.82%** |
| BuildStorm 输出 | 2,527 B | 2,683 B | +6.17% |
| OOM / panic | 0 | 0 | — |

原始数据：

```text
testsuits-final/.tmp/final-runs/20260808-riscv-buildstorm-exec-pte-local-short-194/
testsuits-final/.tmp/final-runs/20260808-riscv-mprotect-local-short-201/
```

### perfmap 诊断 A/B

修改后的固定时长诊断轮：

```text
testsuits-final/.tmp/final-runs/20260808-riscv-mprotect-local-perfmap-short-202/
```

| 指标 | 修改前 run 199 | 修改后 run 202 |
| --- | ---: | ---: |
| probe 5 deps | 27 | **41** |
| probe 8 deps | 66 | **76** |
| perf 触发时 deps | 87 | 89 |
| `__rust_realloc` guest PC | 5.29% children | 相关 PC 合计约 **0.07%** |
| mprotect 搬移热点 | 单点 2.30%–2.44% | 单点最高约 **0.02%** |
| `helper_lookup_tb_ptr` self | 13.57% | **12.03%** |

perf 采样时两轮分别有 87 和 89 个 deps，阶段足够接近；修复目标 `__rust_realloc` 和 mprotect 全表搬移已从主热点消失。`riscv_cpu_tlb_fill` 仍是后续瓶颈，不能把它的占比变化归因成这次优化收益。

### 正确性和构建验证

- RISC-V 10 项综合回归：10/10 通过；
- mprotect 专项回归：5/5 通过，包括 1 页、64 KiB、4 MiB 跨核权限切换；
- `riscv_icache_smp_smoke`：128 次更新 × 7 个远端 hart 通过；
- `ARCH=riscv64 cargo check` 通过；
- `ARCH=loongarch64 cargo check`（`loongarch64-unknown-none-softfloat`）通过；
- `DEBUG_PERF=false`，`rustfmt --check` 与 whitespace check 通过。

回归日志：

```text
testsuits-final/.tmp/final-runs/20260808-riscv-mprotect-local-regressions-200/serial.log
testsuits-final/.tmp/final-runs/20260808-riscv-mprotect-local-regressions-200/mprotect-serial.log
```
