# 8-6 Linux 式 shared-only mmap backing 状态

## 问题概述

每个干净 `MAP_PRIVATE` 文件页除 inode page cache 与实际 PTE/MapArea 外，还在每个 mm
的 `MmapBacking::resident_pages` 中保存一份派生 `BTreeMap`。fork 复制整棵树，典型
BuildStorm child 紧接着 exec 并立即析构，产生纯 bookkeeping 成本。

## 如何发现

BuildStorm perf 中 `BTreeMap<usize, MmapBackingPageState>::drop` 占总 period 1.87%，
相邻热点包含树插入、MemorySet discard 与 frame-tree 析构。源码所有权审计确认 private
页无需第三份索引。Linux 参考为 `address_space::i_pages/i_mmap`、`dup_mmap()` 与
`copy_page_range()`：复制 VMA/PTE 状态，不为每个 mm 复制干净 private page 索引。

```text
.tmp/final-runs/20260806-buildstorm-metadata-lru-87/run/
.tmp/final-runs/20260806-shared-only-mmap-backing-exec-before-91/results.csv
.tmp/final-runs/20260806-shared-only-mmap-backing-exec-after-92/results.csv
.tmp/final-runs/20260806-shared-only-mmap-backing-regressions-88/
```

```sh
perf record -F 99 -e cycles:u -g -p <qemu-pid> -o perf.data -- sleep 15
# guest
/user/exec_file_page_cache_perf_smoke.bin
```

## 怎么解决

`resident_pages` 只跟踪 `MAP_SHARED`，继续承担 shared frame accounting、dirty/
writeback 与 truncate 同步；干净 private 页仅由页表/MapArea 和 inode page cache
表示。长期方案是继续统一 page-cache 与 VMA reverse mapping，并用明确 ownership
不变量替代派生状态复制。

代码把 file fault、页表重建和 invariant 检查统一加上 `region.shared` 条件；
`MAP_SHARED` 仍记录 frame、引用数和 dirty 状态，`MAP_PRIVATE` 的干净页只存在于 inode
page cache 与实际页表映射中。
Linux 的 `address_space::i_pages` 保存文件页，`i_mmap` 关联虚拟内存区域，fork 复制
需要的区域和页表状态，不为每个内存空间克隆一棵干净私有页树。本项目尚无 Linux
区间反向映射，因此只删除可证明为派生数据的 private-page bookkeeping。

## 对应提交

- 状态：待提交，当前实现仍位于未提交工作树。
- 基线：顶层 `21332ba37bf1ba0efe8229e7f80eeffa3b99a239`；`os/`
  `b0185b3a4522c0ffc52599d73bd17b3d52320815`。
- 建议提交主题：`mm: track mmap backing state only for shared pages`。

## 对比提升

并发 rustc 测试的 guest 中位数 `627908 -> 479796 us`（-23.6%），host wall-time
`2570 -> 2013 ms`（-21.7%），各轮 failures=0。较小 fork 对照仅约 1.6%，因此记录
采用贴近 BuildStorm fork+exec 生命周期的强信号结果，不宣称完整 BuildStorm 通过。

---

## 问题与 perf 证据

加入 inode 元数据 LRU 后，BuildStorm 的短 `perf` profile 中
`BTreeMap<usize, MmapBackingPageState>::drop` 占总 period 约 1.87%，相邻热点还包括
该树的插入、`MemorySet` discard 和 frame tree 析构。检查实现发现，每个干净
`MAP_PRIVATE` file page 同时存在三份索引关系：

1. inode page cache 保存共享只读 frame；
2. page table/`MapArea` 保存当前 mm 的实际映射；
3. `MmapBacking::resident_pages` 又按 file-page 建一个派生 `BTreeMap`。

fork 会先复制 VMA/PTE，再完整 clone 第三份树；BuildStorm 的典型 child 随即 exec，
于是刚复制的派生树马上被析构。该树对 private mapping 的 fault、COW 和生命周期都
不是所有权来源，属于可移除的重复 bookkeeping。

profile 原始目录：

```text
.tmp/final-runs/20260806-buildstorm-metadata-lru-87/run/
```

采样总 period 为 `328958852037`，17,658 samples；最大 guest allocator deallocation
占 29.1%。

## Linux 参考

参考本地 Linux 源码：

- `exampleOs/linux/include/linux/fs.h`：`address_space` 用 `i_pages` 保存 page cache，
  `i_mmap` 关联 VMA；
- `exampleOs/linux/mm/mmap.c::dup_mmap()`：fork 复制 VMA；
- `exampleOs/linux/mm/memory.c::copy_page_range()` 与 `vma_needs_copy()`：复制所需
  PTE 状态，不为每个 mm 再 clone 一份 clean private file-page 索引。

本轮据此把 `MmapBacking::resident_pages` 明确收窄为每 mm 的 `MAP_SHARED` 状态：它
仍用于 shared frame accounting、dirty/writeback 和 truncate 同步；干净
`MAP_PRIVATE` 页只由 page table/`MapArea` 与 inode page cache 表示。

具体改动只涉及四个共享机制：

- file fault 仅在 `region.shared` 时增加 backing resident ref；
- 从 PTE 重建状态时只扫描 shared VMA；
- shared file page 仍保存 frame 和 dirty 位；
- debug invariant 要求每个 resident entry 至少被一个 shared VMA 覆盖。

没有绕过 page fault、COW、writeback、truncate 或 exec 校验，也没有用固定返回值满足
测试。

## 严格性能 A/B

旧 ELF 是仅含上一轮 inode LRU 的精确产物：

```text
.tmp/baselines/20260806-private-mmap-derived-state/os-before
sha256 51d31125908a141af927fa5abf2933813971e97ef9459e3193035c6bc09c9614
```

新 ELF 只增加 shared-only backing 改动：

```text
sha256 9b907b647f8486ad9f6bb7e564fe51ad3b51798035192f5f153dd2de36b70ba1
```

性能测试在相同 LoongArch 12-hart、8 GiB、官方 root image 和 `-snapshot` 下运行。
每个 outer round 并行启动两个真实 `rustc -vV`，内部执行 3 轮；旧/新内核各 7 个
outer rounds，全部 `failures=0`：

| kernel | guest median | host outer median |
| --- | ---: | ---: |
| 每 mm 跟踪 clean private pages | 627908 us | 2570 ms |
| 只跟踪 shared pages | 479796 us | 2013 ms |

guest 中位耗时下降 **23.6%**，host wall-time 下降 **21.7%**。原始结果：

- `.tmp/final-runs/20260806-shared-only-mmap-backing-exec-before-91/results.csv`
- `.tmp/final-runs/20260806-shared-only-mmap-backing-exec-after-92/results.csv`

较小的通用 fork/thread 对照各 11 轮，guest 中位数从 147976 us 降至 145572 us
（1.6%），host 179 ms 到 181 ms 属于噪声范围；因此性能结论使用更贴近 BuildStorm
fork+exec 生命周期、信号更强的测试，没有挑选小测试中不稳定的 host 数字。

## 语义回归

LoongArch 运行目录：

```text
.tmp/final-runs/20260806-shared-only-mmap-backing-regressions-88/
```

以下六项全部通过：lazy file fault、private page-cache sharing、private madvise、
cross-mm shared mapping、shared truncate 和 exec thread teardown。

RISC-V 运行目录：

```text
.tmp/final-runs/20260806-shared-only-mmap-backing-riscv-93/
```

lazy fault、private page cache、128x7 SMP icache 及 exec teardown 全部通过。另外：

```zsh
TMPDIR=$PWD/.tmp ARCH=riscv64 cargo check --manifest-path os/Cargo.toml \
    --target riscv64gc-unknown-none-elf
```

通过。完整 BuildStorm 和官方 judge 仍待下一阶段验证。

inode metadata LRU 候选经 BuildStorm A/B 拒绝并回滚后，又用最终组合重跑同一组
LoongArch 六项回归，全部通过：

```text
.tmp/final-runs/20260806-shared-only-no-lru-regressions-96/
```

host `ext4-fs` 单测恢复为原有 13 passed，定向 `cargo fmt -p ext4-fs -p os --
--check` 通过。全 workspace fmt check 仍会报告未改动 vendor 文件的既有格式差异，
本轮没有机械修改 vendor。
