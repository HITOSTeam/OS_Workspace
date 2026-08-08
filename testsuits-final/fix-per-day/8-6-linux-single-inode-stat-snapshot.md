# 8-6 stat 只读一次 inode，不重复查盘六七次

## 问题概述

`Ext4VfsNode::metadata()` 已经通过 `stat_snapshot()` 一次性读出 mode/uid/gid/nlink/size/rdev，但随后为判断文件类型（dir/symlink/fifo/chrdev/blkdev/socket）又依次调用六个 `is_*()` helper。每个 helper 都重新进 ext4 block-cache 查找和加锁。对普通文件来说要走完全部六个检查，一次 `newfstatat()` 最多重复读同一个磁盘 inode 七次。

## 背景知识

**stat 做了什么**。用户程序调用 `stat("/foo/bar", &buf)` 时，内核要把这个文件的所有元信息填到一个结构体里返回：文件大小、权限、所有者、修改时间、硬链接数、设备号、文件类型……这些信息全部存在磁盘 inode 里。

**为什么需要"一次性快照"**。inode 里的字段可能随时被其他核修改（比如正在写入的文件 size 在变、chmod 改了权限）。如果 stat 先读 size、再读 mode、再读 mtime，有可能得到前后不一致的结果——size 是改之前的，mode 是改之后的。正确做法是一次性把 inode 锁住、把所有字段拷贝出来（或者用序列锁保证没有并发写），得到一份一致的"快照"，然后在快照上做后续计算。Linux 的 `generic_fillattr()` 就是这么做的：在一次持有 inode 锁（或 `i_rwsem` 读锁）的窗口内把所有字段填好。

**inode 是什么（回顾）**。每个文件/目录在磁盘上有一个固定结构：

```text
┌───────────────────────────────────┐
│  i_mode  — 类型(4bit) + 权限(12bit) │
│  i_uid/i_gid — 所有者            │
│  i_size  — 文件大小               │
│  i_nlink — 硬链接数               │
│  timestamps — atime/mtime/ctime  │
│  block pointers / extents        │
└───────────────────────────────────┘
```

其中 `i_mode` 的高 4 位就编码了文件类型：普通文件、目录、符号链接、字符设备、块设备、FIFO、socket。一个字段同时告诉你"是什么类型"和"什么权限"。

**旧代码的问题**。本项目的 `stat_snapshot()` 已经正确地一次性读出了整个 inode 的快照。但 `metadata()` 函数在拿到快照之后，为了构造返回给用户的"文件类型"枚举值，又依次调用了：

```text
is_dir()       → 重新进 block-cache 读 inode
is_symlink()   → 重新进 block-cache 读 inode
is_fifo()      → 重新进 block-cache 读 inode
is_chrdev()    → 重新进 block-cache 读 inode
is_blkdev()    → 重新进 block-cache 读 inode
is_socket()    → 重新进 block-cache 读 inode
```

每个 helper 各自独立地去 block-cache 查找对应的 inode 块、加锁、读出 `i_mode`、释放锁。普通文件要走完全部六个分支都返回 false 才落到最后的 else，所以一次 stat 调用产生 1 + 6 = 7 次对同一个 inode 的读取。

在 12 个核同时做 stat 的时候，每次多余的 block-cache lookup 都要竞争 cache 管理器的锁，7 倍的锁竞争直接拉低吞吐。

**内存里的 inode 表**。Linux 把打开过的 inode 都留在内存里（inode cache），同一个 inode 在内存里只有一份，`stat` 直接读内存里的 `i_mode` 字段即可。本项目目前没有全局 inode cache，所以每次 `is_dir()` 这类调用都要回到 block-cache 层查找，代价远大于 Linux 里读一个内存字段。

**正确的做法**。既然 `stat_snapshot()` 已经拿到了完整快照（包含 `i_mode`），直接从快照里提取类型就行了：`mode & 0xF000` 告诉你是目录还是普通文件还是设备，不需要再单独去问六遍。

## 如何发现

BuildStorm 慢探针阶段的 perf 调用链多次落到 `Inode::find()`、`stat_snapshot()` 与 size/type 读取。同时 QEMU 约使用 11.3 核、host 内存充足，排除资源不足。

Linux 对照：
- `ext4_getattr()` / `generic_fillattr()`：从 `path->dentry` 取到同一个内存 inode，`i_mode`（模式字段，包含文件类型和权限）同时给出属性和类型，不重复查盘。

```sh
perf record -F 99 -e cycles:u -g -p <qemu-pid> -o perf.data -- sleep 15
# guest
/user/vfs_stat_smp_perf_smoke.bin
```

原始证据：

```text
.tmp/final-runs/20260806-vfs-stat-perf-before-small-cache-60b/results.csv
.tmp/final-runs/20260806-vfs-stat-single-snapshot-64/results.csv
.tmp/final-runs/20260806-fork-perf-before-small-cache-58b/results.csv
.tmp/final-runs/20260806-fork-perf-single-snapshot-65/results.csv
```

## 怎么解决

**让 `InodeStatSnapshot` 直接暴露 mode 类型判断**：`Ext4VfsNode::metadata()` 从同一次快照同时构造大小、权限、owner、link count、设备号和 `VfsNodeKind`。普通文件不再依次调用六个 `is_*()` helper。

**其他 adapter 也统一改成单次 snapshot**：所有需要类型判断的路径都改为一次 `stat_snapshot()`。

没有引入全局 inode cache（inode 缓存，把磁盘 inode 读到内存后保留住以避免重复读盘）——那需要配套 Linux 式 reclaim，本轮不引入无界强引用。

曾试验两种 8B..4KiB 小对象缓存（额外 per-hart 锁版本 VFS 改善 4.3% 但 fork 退化 5.5%；复用 arena 锁版本两项都退化），均已完整撤回。这也说明 perf 调用链归因需要用 workload A/B 验证，不能只凭 children 百分比保留复杂改动。

## 对应提交

- 状态：待提交，当前实现仍位于未提交工作树。
- 基线：顶层 `21332ba37bf1ba0efe8229e7f80eeffa3b99a239`；`os/` `b0185b3a4522c0ffc52599d73bd17b3d52320815`。
- 建议提交主题：`vfs: derive inode type from one stat snapshot`。

## 对比提升

12,288 次 stat（12 worker 各 1,024 次 `newfstatat()`），A/B 各 11 轮：

| 指标 | 修改前 | 修改后 | 改善 |
| --- | ---: | ---: | ---: |
| guest 中位数 | 215,468 us | 164,075 us | -23.9% |
| host 中位数 | 262 ms | 205 ms | -21.8% |

全部 errors=0。独立 fork/thread 回归没有退化（guest 144,965 → 141,705 us，host 195 → 176 ms）。

聚焦回归 `20260806-vfs-single-snapshot-regressions-66e` 逐项 60 秒硬截止通过：`vfs_stat_smp_perf_smoke`、`open_unlink_lifetime_smoke`、`vfs_pathwalk_smoke`、`unix_vfs_path_smoke`。ext4-fs 13/13，LoongArch release 构建和 RISC-V `cargo check` 通过。

以下是 AI 的具体分析，作为存档。

---

## 历史分析背景

这是一个很直接的冗余读取消除：一次 stat 不应该为了判断文件类型就把同一个磁盘 inode 读七遍。问题在 12-worker 并发 stat 时被放大，因为每次多余的 block-cache lookup 都要竞争锁。下面保留完整 perf 发现细节、被撤回的小对象缓存试验和回归环境。

## Problem

BuildStorm 慢探针阶段的无 `-perfmap` `perf record -F 99 -e cycles:u -g`
调用链多次落到 ext4 的 `Inode::find()`、`Inode::stat_snapshot()` 和 inode
size/type 读取。主机侧 QEMU 平均仍使用约 11.3 个 core，峰值 RSS 约 3.6 GiB，
最低 `MemAvailable` 约 20 GiB，因此不是 host OOM 或资源未释放。

对象 VFS 的 `Ext4VfsNode::metadata()` 已先调用 `stat_snapshot()`，一次读出
mode、uid、gid、nlink、size 和 rdev；但随后为了生成 `VfsNodeKind`，又依次调用
`is_dir()`、`is_symlink()`、`is_fifo()`、`is_chrdev()`、`is_blkdev()` 和
`is_socket()`。每个判断都会重新进入 ext4 block-cache 查找和锁。普通文件会走完
全部六次检查，所以一次 `newfstatat()` 最多重复读取同一个 disk inode 七次。

## Linux reference

参考：

- `exampleOs/linux/fs/ext4/inode.c::ext4_getattr()`；
- `exampleOs/linux/fs/stat.c::generic_fillattr()`。

Linux 的 ext4 getattr 从 `path->dentry` 取得同一个内存 inode，
`generic_fillattr()` 再从该 inode 的 `i_mode`、`i_uid`、`i_gid`、`i_nlink`、
`i_size` 等字段填充一次 `kstat`。文件类型也是 `i_mode` 的一部分，不会为了判断
dir/symlink/fifo/device/socket 再重复查 inode。

本轮没有引入全局 inode 缓存或伪造一套 reclaim/LRU。只让现有
`InodeStatSnapshot` 暴露 mode 类型判断，并让 ext4 VFS adapter 从已经取得的同一
快照生成 `VfsNodeKind`。其他需要类型判断的 adapter 路径也改为一次
`stat_snapshot()`，避免六次串行 block-cache lookup。

## Focused performance proof

新增 `vfs_stat_smp_perf_smoke`：12 个共享地址空间的 worker 同步开始，各对
`/glibc/buildstorm_testcode.sh` 执行 1,024 次 `newfstatat()`；共 12,288 次 stat，
同时校验 syscall 返回值和文件 size。测试路径直接覆盖 perf 命中的元数据读取。

两份内核除本轮单快照修复外一致，均为 LoongArch、12 hart、8 GiB、相同 root
image 和 `/user` image；每组连续运行 11 轮，全部 `errors=0`：

| implementation | guest elapsed median | host wall median |
| --- | ---: | ---: |
| repeated inode reads | 215468 us | 262 ms |
| one stat snapshot | 164075 us | 205 ms |

来宾中位耗时下降 **23.9%**，host wall-time 中位数下降 **21.8%**。原始证据：

- `.tmp/final-runs/20260806-vfs-stat-perf-before-small-cache-60b/results.csv`
- `.tmp/final-runs/20260806-vfs-stat-single-snapshot-64/results.csv`

独立 fork/thread 回归没有被牺牲：相同 11 轮测试的来宾中位数从 144965 us
变为 141705 us，host 中位数从 195 ms 变为 176 ms。原始证据：

- `.tmp/final-runs/20260806-fork-perf-before-small-cache-58b/results.csv`
- `.tmp/final-runs/20260806-fork-perf-single-snapshot-65/results.csv`

## Rejected experiment

perf 调用链还把较多 children 样本归到 buddy allocator 的 `dealloc()`。曾试验
两种 8 B..4 KiB 小对象缓存：额外 per-hart 锁版本的 VFS 中位数仅改善 4.3%，
同时 fork 中位数退化 5.5%；复用 arena 锁版本的 VFS/fork 中位数分别退化到
237267/158273 us。两种实现均已完整撤回，`heap_allocator.rs` 与试验前无差异。
这也说明 perf 的调用链归因仍需用 workload A/B 验证，不能只凭 children 百分比
保留复杂 allocator 改动。

## Regression

LoongArch 12-hart 回归 `20260806-vfs-single-snapshot-regressions-66e` 在逐项 60 秒
硬截止内通过：

- `vfs_stat_smp_perf_smoke`：12,288 次 stat，errors=0；
- `open_unlink_lifetime_smoke`：6 workers × 32 iterations；
- `vfs_pathwalk_smoke`：openat2/pathwalk errno 语义通过；
- `unix_vfs_path_smoke`：ext4 alias、tmpfs lifetime、readonly mount 和 Unix
  pathname 语义通过。

另外：

- `cargo test -p ext4-fs --target x86_64-unknown-linux-gnu`：13 passed；
- LoongArch release kernel 构建通过；
- RISC-V `cargo check --target riscv64gc-unknown-none-elf` 通过。

构建和测试只产生仓库已有 warnings。
