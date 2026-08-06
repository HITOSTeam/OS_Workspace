# 8-6 Linux-style single inode snapshot for `stat`

## 问题概述

`Ext4VfsNode::metadata()` 已通过 `stat_snapshot()` 读取 inode 元数据，却又为判断
dir/symlink/fifo/device/socket 最多重复读取同一磁盘 inode 六次；普通
`newfstatat()` 因此最多执行七次相同 block-cache 查找和加锁。

## 如何发现

BuildStorm 慢探针阶段的 perf 调用链多次落到 `Inode::find()`、`stat_snapshot()` 与
size/type 读取，同时 QEMU 约使用 11.3 核、host 内存充足，排除资源不足。Linux 对照
为 `ext4_getattr()` 和 `generic_fillattr()`：同一个内存 inode 的 `i_mode` 同时给出
属性和类型，不重复查盘。

```text
.tmp/final-runs/20260806-vfs-stat-perf-before-small-cache-60b/results.csv
.tmp/final-runs/20260806-vfs-stat-single-snapshot-64/results.csv
.tmp/final-runs/20260806-fork-perf-before-small-cache-58b/results.csv
.tmp/final-runs/20260806-fork-perf-single-snapshot-65/results.csv
```

```sh
perf record -F 99 -e cycles:u -g -p <qemu-pid> -o perf.data -- sleep 15
# guest
/user/vfs_stat_smp_perf_smoke.bin
```

## 怎么解决

让 `InodeStatSnapshot` 直接暴露 mode 类型判断，ext4 VFS adapter 从同一快照生成
`VfsNodeKind`；其他 adapter 类型判断也收敛到单次 snapshot。更完整的 inode cache
可以进一步避免查盘，但必须配套 Linux 式生命周期与 reclaim，本轮不引入无界强引用。

`InodeStatSnapshot` 暴露 `mode` 的类型判断，`Ext4VfsNode::metadata()` 从一次快照同时
构造大小、权限、owner、link count、设备号和 `VfsNodeKind`；普通文件不再依次调用
六个 `is_*()` helper。

## 对应提交

- 状态：待提交，当前实现仍位于未提交工作树。
- 基线：顶层 `21332ba37bf1ba0efe8229e7f80eeffa3b99a239`；`os/`
  `b0185b3a4522c0ffc52599d73bd17b3d52320815`。
- 建议提交主题：`vfs: derive inode type from one stat snapshot`。

## 对比提升

12,288 次 stat 的 guest 中位数 `215468 -> 164075 us`（-23.9%），host wall-time
`262 -> 205 ms`（-21.8%），全部 errors=0；独立 fork/thread 回归也未退化。曾试验的
小对象 cache 经 A/B 退化后已撤回，没有把 perf children 百分比当成提交依据。

---

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
