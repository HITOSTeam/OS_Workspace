# 8-6 让 inode-table 位置查找不再抢 allocator 锁

## 问题概述

`Inode::find()` 每次都要拿 filesystem-wide allocator mutex 才能算出 inode-table 的磁盘位置，但 block size、inode size、inodes-per-group 和各组 inode-table 起始块在挂载后根本不变。结果是所有并行的 pathname lookup / stat 都和无关的 block/inode 分配排在同一把锁后面。另外，目录索引已经命中时还多读一次父 inode 判断类型。

## 背景知识

**磁盘 inode 在哪里**。ext4 把整块磁盘分成若干"块组"（block group），每个组里有固定数目的 inode（由 `inodes-per-group` 参数决定）。给定一个 inode 号，要算出它存在磁盘第几个块上，步骤是：

```text
第几组 = inode_num / inodes_per_group
组内偏移 = inode_num % inodes_per_group
磁盘块号 = 该组的 inode-table 起始块 + (组内偏移 × inode_size) / block_size
```

这些参数（block size、inode size、inodes-per-group、各组 inode-table 起始块）在格式化时就写死了，挂载后一个字节都不会变。

**为什么旧代码要拿 allocator 锁**。旧实现把上面这些参数存在和 block/inode 分配器共用的结构体里。分配器的 bitmap cursor、空闲计数这些字段确实需要互斥保护，但挂载后不变的几何参数也被同一把锁包住了。于是出现这种场景：

```text
hart 0: stat("/bin/ls")
  → 要查 inode 123 的磁盘位置
  → 拿 allocator mutex
  → 只是做一道除法和查表
  → 释放锁

hart 1: write() 需要分配新块
  → 等 allocator mutex
  → 修改 bitmap cursor

hart 2: stat("/etc/passwd")
  → 等 allocator mutex
  → 只是做除法和查表
```

hart 0 和 hart 2 做的事情完全不修改任何可变状态，它们互相等没有道理。

**Linux 是怎么做的**。Linux 的 `__ext4_get_inode_loc()` 通过 `EXT4_INODES_PER_GROUP(sb)`（挂载时从超级块读出的常量）算出组号，再从 `s_group_desc`（一个只读数组，挂载时一次性读好）取出 group descriptor，里面有 inode-table 起始块。整个过程不需要任何互斥锁——因为读的全是常量。

**内存里的 inode 表为何存在**。磁盘 inode 存在磁盘上，每次都去读磁盘太慢。内核在内存里维护一张"活跃 inode 表"：打开过的文件、遍历过的目录的 inode 都留在这张表里，下次再访问直接取内存副本。这张表按 inode 号做哈希索引，查表只需要 inode 号和一些不变的几何参数——正是本文优化的那些。

**i_count 与 i_nlink**。每个内存 inode 有两个计数器：

- `i_count`（内存引用数）：有几个打开的 fd、mmap、目录遍历在用这个 inode。降到零说明内核里没人在用它了，可以从内存表里移除。
- `i_nlink`（硬链接数）：有几个目录项指向这个 inode。降到零说明文件名全删了，等最后一个引用也消失就可以释放磁盘空间。

本文的优化和这两个计数没有直接关系，但理解它们有助于理解后面的"目录索引命中时不需要重新读父 inode"——因为目录索引本身就是由父 inode 对象维护的，索引命中已经隐含"父对象是目录"这个信息。

**目录索引命中走快路径**。旧代码在目录索引命中的情况下仍然从磁盘重读父 inode 判断"它是不是目录"。但目录索引本身就是为目录建立的，如果索引存在且命中，父对象必然是目录，不需要再确认。只有 cold miss 时才有必要补读一次。

## 如何发现

BuildStorm 慢探针阶段用 `perf record` 采样，稳定 guest PC 之一解析到 `Inode::find()`。host `MemAvailable` 和 `SwapFree` 充足，排除资源耗尽。

```sh
perf record -F 99 -e cycles:u -g -p <qemu-pid> -o perf.data -- sleep 15
# guest
/user/vfs_stat_smp_perf_smoke.bin
```

Linux 对照：
- `ext4_lookup()` → `__ext4_get_inode_loc()`：通过挂载时已确定的 `EXT4_INODES_PER_GROUP()`、inode size 和 group descriptor 直接算位置；
- `ext4_get_group_desc()`：从 `s_group_desc` 的 RCU（Read-Copy-Update，读侧不加锁的并发保护）可读数组取 group descriptor，不为普通 inode lookup 抢 allocator-wide mutex。

原始证据：

```text
.tmp/final-runs/20260806-vfs-stat-ab-baseline-81/results.csv
.tmp/final-runs/20260806-vfs-stat-ab-optimized-82/results.csv
.tmp/final-runs/20260806-fork-ab-baseline-79/results.csv
.tmp/final-runs/20260806-fork-ab-optimized-80/results.csv
```

## 怎么解决

**挂载时建立不可变 `InodeTableLayout`**：收集 block size、inode size、inodes-per-group 和每组 inode-table 起始块，allocator 和读侧通过同一个 `Arc` 共享。`Inode::find()` 直接从 layout 算位置，不再进 allocator lock。

**目录索引命中走快路径**：已有索引命中时直接用；cold miss 才补读父 inode 确认是目录。VFS adapter 只在 lookup 失败时补查父类型以区分 `ENOENT` 和 `ENOTDIR`。

没有引入 Linux 式 inode cache 或 RCU 查找——那需要先有可靠的 reclaim（回收机制）。本轮没有增加永久强引用。

## 对应提交

- 状态：待提交，当前实现仍位于未提交工作树。
- 基线：顶层 `21332ba37bf1ba0efe8229e7f80eeffa3b99a239`；`os/` `b0185b3a4522c0ffc52599d73bd17b3d52320815`。
- 建议提交主题：`ext4: make inode-table lookup lockless`。

## 对比提升

12,288 次 stat（12 worker 各 1,024 次 `newfstatat()`），交替 A/B 各 11 轮：

| 指标 | 修改前 | 修改后 | 改善 |
| --- | ---: | ---: | ---: |
| guest 中位数 | 153,869 us | 143,236 us | -6.9% |
| host 中位数 | 199 ms | 177 ms | -11.1% |

独立 fork/thread 对照也没有退化（guest -6.3%，host -3.4%）。

聚焦回归通过（`vfs_stat_smp_perf_smoke` 12,288 stat errors=0、`open_unlink_lifetime_smoke`、`vfs_pathwalk_smoke`、`unix_vfs_path_smoke`），ext4-fs 13/13，LoongArch release 构建和 RISC-V `cargo check` 通过。不代表完整 BuildStorm 已通过。

以下是 AI 的具体分析，作为存档。

---

## 历史分析背景

这是 `Inode::find()` 热路径上的一次简单但有效的锁消除：挂载后不变的磁盘几何信息没必要每次都在 allocator 的 mutex 里读。问题在 BuildStorm 规模下才明显，因为连续的 `cargo build` 产生大量并发 stat。下面保留完整的 perf 发现过程、Linux 对照和回归细节。

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
