# 8-6 只为共享映射记录 mmap backing 状态

## 问题概述

每个干净的 `MAP_PRIVATE` 文件页，除了在 inode page cache 里存一份、在页表里存一份，
还在每个进程的 `MmapBacking::resident_pages` 里多存了一棵 `BTreeMap`。fork 时会整棵
复制这棵树，但 BuildStorm 的典型 child 紧接着 exec 就把刚复制的树析构了——白干一场。

```text
进程 A 的地址空间           进程 B（fork 出来的）
┌──────────────────┐       ┌──────────────────┐
│ 页表：page → frame│       │ 页表：page → frame│  ← 已经有映射关系
├──────────────────┤       ├──────────────────┤
│ resident_pages   │       │ resident_pages   │  ← 又多一棵重复的树
│  (BTreeMap 副本) │       │  (BTreeMap 副本) │     fork 后 exec 马上析构
└──────────────────┘       └──────────────────┘
          │                          │
          └────────┐  ┌──────────────┘
                   ▼  ▼
           inode page cache           ← 真正的数据来源
```

对于私有映射的干净页，页表和 inode page cache 这两份已经够了——`resident_pages` 是
纯粹多余的 bookkeeping（记账开销）。

## 背景知识

这一节给只上过操作系统课的读者铺路。已经熟悉 mmap 的可以跳过。

**mmap 是什么**。`mmap()` 系统调用把一个文件（或者一段匿名内存）直接映射到进程的
虚拟地址空间里。映射建立后，进程读写那段地址就等于在读写文件，不需要再调用
`read()`/`write()`。操作系统利用页表（page table）把虚拟地址指向物理内存中缓存的
文件页，缺页时再从磁盘读入——对进程来说完全透明。

用一个类比：如果文件是仓库里的一本书，普通 `read()` 是"请图书管理员复印几页给我"，
`mmap()` 则是"把书直接放在我桌上，我翻到哪就看哪"。省了复印（内核到用户的拷贝），
但书放在桌上时别人也可能在翻。

**MAP_SHARED 与 MAP_PRIVATE 的根本差别**。mmap 有两种模式：

```text
MAP_SHARED（共享映射）
┌────────────────────────────────────────────────┐
│ 写入会直接修改文件内容                          │
│ 多个进程映射同一文件 → 互相可见对方的写入       │
│ 脏页最终要写回磁盘（writeback）                 │
│ 内核必须跟踪"哪些页被改过、什么时候该写回"     │
└────────────────────────────────────────────────┘

MAP_PRIVATE（私有映射）
┌────────────────────────────────────────────────┐
│ 写入不会改变文件                                │
│ 第一次写时，内核复制一份独立副本（COW，写时复制）│
│ 此后这页跟文件再无关系                          │
│ 多个进程各写各的，互不干扰                      │
│ 没写过的干净页仍共享 inode page cache 的那份    │
└────────────────────────────────────────────────┘
```

所以：只有共享映射才需要记录"哪些页脏了、引用计数多少、要不要写回"这些信息。
私有映射的干净页只是 inode page cache 的一个只读视图，写了就变成自己的匿名页，
不需要额外的回写跟踪。

**为什么多余的 BTreeMap 在 fork+exec 模式下特别贵**。BuildStorm 大量 fork 子进程，
每次 fork 都要深拷贝父进程的 `resident_pages`。子进程拿到这棵树后立刻 exec 换成
新程序——exec 会析构旧地址空间，包括刚复制的树。对一棵几千项的 BTreeMap 来说，
clone + drop 全是纯 CPU 开销，产生的 cache miss 和分配/释放还会挤占真正有用的工作。

**Linux 怎么做的**。Linux 内核里，`address_space::i_pages` 保存文件页缓存，
`i_mmap` 用区间树（interval tree）关联所有映射了这个文件的 VMA。fork 时
`copy_page_range()` 复制页表状态，不会为每个地址空间额外维护一棵干净私有页的
索引。换句话说，Linux 认为"页表 + page cache"就是私有干净页的完整真相，不需要
第三份记录。

**本项目的历史原因**。早期为了简化 fault 路径的查找，给每个 `MmapBacking` 加了
`resident_pages`，不区分 shared/private 一律记录。随着 inode page cache 和 COW
逐步完善，这份多余记录变成了纯粹的性能负担。

## 如何发现

BuildStorm perf 中 `BTreeMap<usize, MmapBackingPageState>::drop` 占总 period 1.87%，
相邻热点包含树插入、MemorySet discard 与 frame-tree 析构。源码审计确认 private
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

**收窄 `resident_pages` 的适用范围**：只在 `MAP_SHARED` 时记录 frame、引用数和
dirty 状态，继续用于 shared frame accounting、dirty/writeback 与 truncate 同步。
`MAP_PRIVATE` 的干净页只存在于 inode page cache 与实际页表映射中，不再额外记录。

具体改动四处：

- **file fault**：只在 `region.shared` 时往 backing 里增加 resident ref；
- **PTE 重建**：重建状态时只扫描 shared VMA；
- **shared file page**：仍保存 frame 和 dirty 位，用于写回；
- **debug invariant**：要求每个 resident entry 至少被一个 shared VMA 覆盖。

Linux 的做法更彻底：它有完整的区间反向映射（reverse mapping），能从一个物理页找到
所有映射它的 VMA。本项目还没有这套机制，因此只做了"把可证明为派生数据的
private-page bookkeeping 删掉"这一步，不影响 page fault、COW、writeback、
truncate 或 exec 的正确性。

## 对应提交

- 状态：待提交，当前实现仍位于未提交工作树。
- 基线：顶层 `21332ba37bf1ba0efe8229e7f80eeffa3b99a239`；`os/`
  `b0185b3a4522c0ffc52599d73bd17b3d52320815`。
- 建议提交主题：`mm: track mmap backing state only for shared pages`。

## 对比提升

| 指标 | 旧（每 mm 跟踪 clean private） | 新（只跟踪 shared） | 变化 |
| --- | ---: | ---: | ---: |
| 并发 rustc guest 中位数 | 627908 us | 479796 us | **-23.6%** |
| host wall-time 中位数 | 2570 ms | 2013 ms | **-21.7%** |
| failures | 0 | 0 | — |

较小的通用 fork 对照仅约 1.6%，信号不强，因此结论采用贴近 BuildStorm fork+exec
生命周期的测试。没有运行完整 BuildStorm 或官方 judge。

以下是 AI 的具体分析，作为存档。

---

## 历史分析背景

这个问题跨 mmap fault 路径、fork 的地址空间复制和 inode page cache 三个子系统。
早期为了让 fault 路径快速查找已加载页而引入的 per-mm BTreeMap，在 COW 和 page cache
完善后变成了纯粹的派生数据。下面保留完整的 perf 证据、Linux 对比、严格 A/B 和回归
测试记录。

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
