# 8-6 Linux-style lockless inode-table lookup

## 问题概述

`Inode::find()` 为计算只读 inode-table 位置也获取 filesystem-wide allocator mutex，
使并行 pathname/stat lookup 与无关查找、分配串行化；已有目录索引命中时还会重复读取
父 inode 类型。

## 如何发现

BuildStorm 慢探针阶段的 `perf record` 将稳定 guest PC 解析到 `Inode::find()`；host
`MemAvailable` 和 `SwapFree` 充足，排除资源耗尽。Linux 对照为
`ext4_lookup()`、`__ext4_get_inode_loc()` 与 `ext4_get_group_desc()`：挂载后不变的
几何和 group descriptor 可由读侧直接使用，无需 allocator-wide mutex。

```sh
perf record -F 99 -e cycles:u -g -p <qemu-pid> -o perf.data -- sleep 15
# guest
/user/vfs_stat_smp_perf_smoke.bin
```

```text
.tmp/final-runs/20260806-vfs-stat-ab-baseline-81/results.csv
.tmp/final-runs/20260806-vfs-stat-ab-optimized-82/results.csv
.tmp/final-runs/20260806-fork-ab-baseline-79/results.csv
.tmp/final-runs/20260806-fork-ab-optimized-80/results.csv
```

## 怎么解决

挂载时建立不可变 `InodeTableLayout`，由 allocator 与读侧通过同一个 `Arc` 共享；
`Inode::find()` 直接计算位置。目录索引命中走快路径，失败时才补查父类型以保持
`ENOENT/ENOTDIR`。更完整方案可引入 Linux 式 inode cache/RCU 查找，但必须先具备可靠
reclaim；本轮没有为此增加永久强引用。

`InodeTableLayout` 在挂载阶段收集 block size、inode size、inodes-per-group 和每组
inode-table 起始块；`Inode::find()` 只读该 `Arc`，分配器仍在自己的锁内使用同一
布局，不维护第二套位置公式。

## 对应提交

- 状态：待提交，当前实现仍位于未提交工作树。
- 基线：顶层 `21332ba37bf1ba0efe8229e7f80eeffa3b99a239`；`os/`
  `b0185b3a4522c0ffc52599d73bd17b3d52320815`。
- 建议提交主题：`ext4: make inode-table lookup lockless`。

## 对比提升

12,288 次 stat 的 guest 中位数 `153869 -> 143236 us`（-6.9%），host 中位数
`199 -> 177 ms`（-11.1%）；交替 fork/thread 对照也分别改善 6.3% 和 3.4%。
聚焦回归通过，但不代表完整 BuildStorm 已通过。

---

## Problem

BuildStorm 恢复运行 `20260806-buildstorm-single-stat-resume-67c` 在慢探针阶段
用 `perf record -F 99 -e cycles:u -g` 采样后，稳定来宾 PC 之一解析到
`ext4_fs::vfs::Inode::find()`。该运行到约 961 秒时 `tg-xtask` 仍不存在，随后
`wc -l` 探针超过 20 秒硬截止并及时终止；host 最低 `MemAvailable` 约 20.4 GiB，
最低 `SwapFree` 约 16.3 GiB，因此没有内存耗尽或 QEMU RSS 持续泄漏证据。

`Inode::find()` 原来为了把目录项 inode number 换算成 inode-table block，每次都
取得 `Ext4FileSystemHandle` 的 filesystem-wide cooperative mutex。该锁实际用于
block/inode allocator 的可变 cursor 和 bitmap 元数据，而 block size、inode size、
inodes-per-group 及各 group 的 inode-table 起始 block 在挂载后均不变。并行
pathname/stat lookup 因此会和完全无关的查找及分配串行化。

另外，对已存在目录索引的 lookup，adapter 先读一次父 inode 判断目录，
`Inode::find()` 又读一次；目录索引本身已经由该目录 inode 建立，并在 inode 删除或
复用时由 `inode_caches_invalidate()` 清除，这两个类型读取也是重复工作。

## Linux reference

参考本地 Linux 源码：

- `exampleOs/linux/fs/ext4/namei.c::ext4_lookup()` 从目录项取得 inode number 后调用
  `ext4_iget()`；
- `exampleOs/linux/fs/ext4/inode.c::__ext4_get_inode_loc()` 通过挂载态的
  `EXT4_INODES_PER_GROUP()`、inode size 和 group descriptor 计算 inode-table
  位置；
- `exampleOs/linux/fs/ext4/balloc.c::ext4_get_group_desc()` 从
  `s_group_desc` 的 RCU 可读数组取得已经加载的 group descriptor，不会为了普通
  inode lookup 获取 allocator-wide mutex。

本轮据此在挂载时建立一个不可变 `InodeTableLayout`，保存 block/inode 几何及各
group 的 inode-table block。allocator 路径和 lockless reader 通过同一个 `Arc`
共享该对象，避免维护两份位置计算公式。`Inode::find()` 直接从 layout 计算位置，
不再进入 filesystem-wide allocator lock。

已有目录索引命中时直接使用索引；cold miss 仍先读取父 inode 并确认它是目录。
VFS adapter 只在 lookup 失败时补查父类型，以继续区分 `ENOENT` 和 `ENOTDIR`。
没有增加伪 inode cache、永久强引用或新的 reclaim/LRU 抽象。

## Focused performance proof

测试为 `vfs_stat_smp_perf_smoke`：12 个 worker 各执行 1,024 次
`newfstatat()`，总计 12,288 次，并校验返回值和 size。两份 ELF 使用相同
LoongArch 12-hart、8 GiB、root image、`/user` image 和 `-snapshot`；旧 ELF 是
Cargo 保留的上一轮精确产物，随后立即交替运行新 ELF。每份连续 11 轮，全部
`errors=0`：

| implementation | guest median | host median |
| --- | ---: | ---: |
| allocator lock + repeated parent type read | 153869 us | 199 ms |
| shared immutable layout + cached-dir fast path | 143236 us | 177 ms |

来宾中位耗时下降 **6.9%**，host wall-time 中位数下降 **11.1%**。原始证据：

- `.tmp/final-runs/20260806-vfs-stat-ab-baseline-81/results.csv`
- `.tmp/final-runs/20260806-vfs-stat-ab-optimized-82/results.csv`

同一时段交替执行的独立 fork/thread 测试没有退化：

| implementation | guest median | host median |
| --- | ---: | ---: |
| previous kernel | 148524 us | 177 ms |
| optimized kernel | 139125 us | 171 ms |

来宾中位耗时下降 **6.3%**，host wall-time 下降 **3.4%**。原始证据：

- `.tmp/final-runs/20260806-fork-ab-baseline-79/results.csv`
- `.tmp/final-runs/20260806-fork-ab-optimized-80/results.csv`

早先非交替运行的 fork 结果跨 QEMU 启动波动较大，因此没有用其中最有利的一组
作为结论；上表使用相邻的旧/新 ELF 受控对照。

## Regression

最终结构版本的 LoongArch 12-hart 回归
`20260806-shared-inode-layout-regressions-final-83` 在逐项 60 秒来宾截止和 70 秒
外部截止内通过：

- `vfs_stat_smp_perf_smoke`：12,288 次 stat，errors=0；
- `open_unlink_lifetime_smoke`：打开后删除生命周期通过；
- `vfs_pathwalk_smoke`：包括 non-directory lookup 的 errno 语义通过；
- `unix_vfs_path_smoke`：ext4/tmpfs/Unix pathname 语义通过。

另外：

- `cargo test -p ext4-fs --target x86_64-unknown-linux-gnu`：13 passed；
- LoongArch release kernel 构建通过；
- RISC-V `cargo check --target riscv64gc-unknown-none-elf` 通过。

构建和测试只产生仓库已有 warnings。该微基准证明热点路径改善，但不代表
BuildStorm 已通过；全量恢复运行仍需单独用硬探针截止验证。
