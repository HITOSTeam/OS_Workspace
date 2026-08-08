# 8-8 参考 Linux 把 mprotect 从全表重建改为局部 VMA 更新

## 问题概述

RISC-V BuildStorm 在编译 `tg-xtask` 时仍明显慢于 LoongArch。它不是死锁：guest shell 探针持续可用，依赖文件持续生成，QEMU 也仍在消耗 CPU。`-perfmap` 样本最终把热点定位到 `MemorySet::mprotect_user_range_inner()`：每次修改一个很小的地址区间，旧实现都会：

1. `mem::take()` 取走完整 `self.areas`；
2. 遍历、搬移地址空间中的每个 `MapArea`；
3. 新建完整 `Vec<MapArea>`；
4. 对整个列表重新排序。

BuildStorm 中 rustc/linker 会频繁修改映射权限。随着 VMA 数量增加，这条路径退化为每次 mprotect 都做 O(全部 VMA) 的复制、重分配和排序。

LoongArch 也会执行这段通用 VMA 代码，所以不是完全没有问题；只是它通过 DMW/IOCSR 访问内存和设备，没有 RISC-V 上 `sfence.vma`、SBI RFENCE 以及 QEMU softmmu/TB cache 被打冷的放大链。因此同一份元数据低效在 LoongArch 上主要表现为额外 CPU 工作，在 RISC-V TCG 上更容易成为主热点。

## 背景知识

这一节给只上过操作系统课的读者铺路。已经熟悉的可以跳过。

**VMA 是什么**。课上讲进程地址空间时，通常画成「代码段、数据段、堆、栈」几块。内核
实际上是用一串区间来记的，每个区间叫一个 VMA（Virtual Memory Area，虚拟内存区域），
记着起止地址、权限（读/写/执行）、以及这段内存对应文件的哪里（如果是文件映射）：

```text
进程地址空间 = 一串互不重叠、按地址排好序的 VMA

0x10000-0x12000  r-x  /bin/prog 的代码
0x12000-0x13000  rw-  /bin/prog 的数据
0x20000-0x40000  rw-  堆
0x7fff...        rw-  栈
```

每次 mmap 新映射一段、munmap 取消一段、mprotect 改一段的权限，都是在这串区间上做
增删改。区间数量会随程序运行不断增长——一个链接器可能有上千个 VMA。

**mprotect 干什么**。`mprotect(addr, len, prot)` 改一段已有映射的权限。典型场景：
动态链接器先把某段映射成可写，填好重定位表，再改成只读；JIT 先写机器码再改成可执行。
它只碰 `[addr, addr+len)` 这一段，逻辑上跟其他 VMA 无关。

麻烦在于 `[addr, addr+len)` 的边界不一定和现有 VMA 对齐。如果一次 mprotect 只改一个
VMA 的中间一小段，就得把这个 VMA **拆成三个**：前段保持旧权限、中段用新权限、后段
保持旧权限。所以实现里必须有 split 逻辑。

**为什么「全表重建」很贵**。旧实现每次 mprotect 都把整串 VMA 取走、逐个搬进新数组、
再整体排序。改一个 4 KiB 的小区间，代价却和「地址空间里一共有多少个 VMA」成正比。
VMA 越多越慢，而编译过程恰好会不断制造新 VMA：

```text
理想： 二分找到相交的那几项 → 只改这几项          O(log n)
旧实现：取走全部 → 全部搬一遍 → 全部重排序        O(n) 复制 + O(n log n) 排序
```

Linux 把 VMA 存在 Maple Tree（一种区间索引树）里，查找、拆分、合并都只碰局部，从来
不为改一个区间去动整个集合。本文的修改是在现有「有序数组」结构上做同样的事：先二分
定位，再局部改。

**perf 与 perfmap 怎么定位到这里**。perf 是采样式 profiler：让性能计数器每计满 N 个
周期就中断一次，在中断里记下当前的 PC（程序计数器）和调用栈，跑一段时间后统计哪个
地址出现得最多。因为记下来的是**地址**，要变成函数名得靠符号表。QEMU 用 `-perfmap`
参数会写出 `/tmp/perf-<pid>.map`，每行是「起始地址 长度 名字」，perf 读它才能把
JIT 出来的翻译块和 guest 内核地址还原成可读名字。没有这个文件，报告里只剩一串十六
进制。

两个注意点。一是 map 文件必须和 `perf.data` 来自同一个进程实例，所以本文的做法是
先 `SIGSTOP` 冻住 QEMU、按精确 PID 归档 map，再结束进程。二是采样只给**统计分布**，
不给精确调用次数；它能指出时间花在哪条指令附近，但要证明因果还得反汇编 + `addr2line`
落回源码行，本文就是这么确认那三个 PC 在搬 `MapArea` 的循环里的。

**为什么同一份低效在两个架构上表现不同**。LoongArch 有 DMW（直接映射窗口），一段
虚地址不经页表直接对应物理地址，内核访问内存和设备寄存器可以走它，不占 TLB 也不用
刷。RISC-V 没有这个机制，改完页表要执行 `sfence.vma`，跨核还得通过 SBI 发远程刷新
请求；再加上 QEMU 用 TCG 动态翻译，页表一变翻译块缓存就被打冷。所以同样多余的
CPU 工作，在 RISC-V 上会被这条链路放大成主热点，在 LoongArch 上只是「多干了点活」。

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
