# 8-6 rename 覆盖目标时保留旧 inode 到最后一个引用消失

## 问题概述

`rename(source, target)` 覆盖已有普通文件时，旧实现直接 unlink target 的磁盘 inode。如果旧文件仍被 fd 或 file-backed VMA（虚拟内存区域，`mmap` 建立的文件映射）持有，旧对象就会立即失效：延迟 fault 读到零，旧 fd 的 `pread` 返回 EOF。

## 背景知识

**inode 是什么**。课上讲的"文件控制块"在 Linux 里叫 inode（index node，索引节点）。磁盘上每个文件或目录各占一个 inode，存着大小、权限、时间戳、数据块位置等所有元信息。内核把用到的 inode 从磁盘读到内存里，后续操作都在内存副本上进行。

**目录项和 inode 的关系**。目录本质上是一张"文件名 → inode 号"的映射表。一个文件可以有多个名字（硬链接），即多条目录项指向同一个 inode。目录项是"路"，inode 是"房子"——删一条路不等于拆房子，要等所有路都没了才算真正无人可达。

**i_nlink 和 i_count——两个不同的计数器**：

```text
┌────────────────────────────────────────────────┐
│  磁盘上的计数器 i_nlink（硬链接数）            │
│    有几个目录项指向这个 inode                  │
│    每 unlink/rmdir 一个名字就减一              │
│    减到零 = 文件名全没了                       │
│                                                │
│  内存里的计数器 i_count（内核引用数）          │
│    有几个 fd、mmap、目录遍历在使用这个 inode   │
│    每 open、mmap 加一，close、munmap 减一      │
│    减到零 = 内核里没人在用了                   │
└────────────────────────────────────────────────┘
```

**open-unlinked：删文件但不释放**。考虑这个场景：

```text
进程 A: fd = open("data.log")    ← i_count 变成 1
进程 B: unlink("data.log")       ← i_nlink 变成 0
进程 A: read(fd, buf, 4096)      ← 应该还能读！
```

这在 Linux 里是完全合法的。`unlink` 只是把目录项删了（i_nlink = 0），但文件内容还在磁盘上。只要还有 fd 或 mmap 引用着这个 inode（i_count > 0），内核就不能回收它的磁盘块。只有当最后一个引用也消失时，才真正释放空间。这就是"open-unlinked"语义。

**rename 覆盖和 unlink 的关系**。`rename("new.log", "data.log")` 如果 `data.log` 已存在，语义上等于：先把旧 `data.log` 的目录项替换成指向 `new.log` 的 inode，再让旧 inode 的 i_nlink 减一。效果就像对旧文件做了一次 unlink。所以 rename 覆盖时也必须遵守 open-unlinked 规则——如果旧文件还有人在用，不能立即销毁旧 inode。

**旧代码的问题**。本项目的 unlink syscall 已经正确实现了 open-unlinked 保护（有引用就不立即回收）。但 rename 路径里绕过了 unlink syscall，直接调用底层 `parent.unlink(name)` 把旧目录项连同 inode 一起干掉了。于是出现这种情况：

```text
1. mmap("old.txt") → VMA 持有旧 inode 的引用
2. rename("new.txt", "old.txt") → 旧 inode 被直接释放！
3. 访问 mmap 映射 → fault 时旧 inode 已经不在了，读到全零
```

**Linux 怎么做的**。`vfs_rename()` 在完成 dentry 替换后，旧 dentry 对应的 inode 并不立即释放。内核对这个 inode 调用 `iput()`：如果 i_count > 1（还有别的引用），只是减一并留着；只有 i_count 降到零时才执行 `iput_final()` → `evict()`，真正回收磁盘空间。整个过程中，VMA 的 `vm_file` 独立持有一份 file 引用，不会因为目录项消失而受影响。

**为什么 VMA 需要独立引用**。`mmap` 建立映射时，VMA 里的 `vm_file` 字段持有一个文件引用（相当于 i_count 加一）。即使用户关闭了 fd，VMA 还活着，旧 inode 就不能回收。后续 page fault 时内核通过 `vma->vm_file->f_mapping` 找到旧 inode 的数据，不依赖任何目录项。

**rename 时的加锁顺序**。rename 涉及至少两个 inode（source 的父目录和 target 的父目录），跨目录时还要防止循环。Linux 用 `lock_rename()` 取得两个目录的锁（按 inode 地址排序避免死锁），本项目用 `(device_id, inode_num)` 排序取锁，再加一把全局 topology mutex 防止并发 rename 形成环。详见 `7.31-ext4-virtio-concurrency.md` 中 inode 锁一节。

## 如何发现

新增最小复现测试"mmap 后不触页、rename 覆盖后再 fault"，旧内核首轮稳定失败。源码审计发现 rename 绕过了 unlink syscall 已有的 open-unlinked 保护。

```sh
ARCH=loongarch64 SMP=12 MEM=8G IMAGE_MODE=snapshot \
  testsuits-final/run.sh shell
# guest
/user/rename_over_mmap_lifetime_smoke.bin
```

修复前首轮失败输出：

```text
RENAME_OVER_MMAP_LIFETIME_FAIL stage=verify iteration=0 \
expected_old=0x31 mapped=0x0 fd=None expected_new=0xc1 replacement=Some(193)
```

证据：

```text
testsuits-final/.tmp/final-runs/20260806-rename-lifetime-baseline-28/serial.log
```

Linux 对照：
- `vfs_rename()`：锁定 source/target inode 后调用文件系统 rename，再移动或交换 dentry；
- `iput()` / `iput_final()`：link count 归零不等于有引用的 inode 立即释放，最终引用归零才进入 eviction（回收）；
- VMA 的 `vm_file` 独立持有 file 引用，关闭 fd 不应破坏后续 fault；
- `filemap_fault()`：fault 通过 `vma->vm_file->f_mapping` 找回旧 inode 的 page cache。

## 怎么解决

**普通文件目标复用 deferred-unlink 兼容层**：

1. `remove_rename_target()` 接收已锁定的 target inode，调用 `defer_unlink_open_file()`；
2. 若有打开引用，先取 `DeferredUnlinkReservation`，关闭 last-close 与 rename 登记之间的竞态窗口；
3. 把旧目录项原子改为隐藏名 `.ltp_orphan.<pid>.<seq>`；
4. source 按原路径改到 target，可见命名空间立即得到新对象；
5. fd、epoll/SCM_RIGHTS 引用或 VMA 的最后一个 `Arc<OSInode>` 消失时，清理隐藏目录项并回收旧 inode；
6. 没有打开引用时仍直接 unlink，不增加正常 rename 的持久对象。

**为什么用隐藏名而不是真正的 inode 生命周期分离**：当前 ext4 后端把"最后目录项删除"和"磁盘 inode 回收"耦合在一起，没有原生拆开。隐藏名是兼容层，最终应由后端原生支持 link count 为零但 inode 引用非零的状态来取代。

该修改位于通用 rename 目标删除路径，同时覆盖同目录与跨目录 rename，不识别测试名，也不改变目录目标、未打开普通文件、`RENAME_NOREPLACE` 或 `RENAME_EXCHANGE` 的既有分支。

## 对应提交

- 状态：待提交，当前实现仍位于未提交工作树。
- 基线：顶层 `21332ba37bf1ba0efe8229e7f80eeffa3b99a239`；`os/` `b0185b3a4522c0ffc52599d73bd17b3d52320815`。
- 建议提交主题：`vfs: preserve overwritten inode lifetime across rename`。

## 对比提升

这是语义正确性修复，不是吞吐优化：

- 旧版第 0 轮即失败；
- 新版 32/32 轮通过（16 轮仅由 VMA 持有旧 inode、16 轮由 fd+VMA 持有）。

```text
RENAME_OVER_MMAP_LIFETIME_PASS iterations=32 vma_only=16 fd_and_vma=16
```

聚焦组 3/3 通过：

| 测试 | host elapsed | 结果 |
| --- | ---: | --- |
| `open_unlink_lifetime_smoke` | 186 ms | PASS，6 workers × 32 iterations |
| `rename_over_mmap_lifetime_smoke` | 67 ms | PASS，16 VMA-only + 16 fd/VMA |
| `shared_file_truncate_cache_smoke` | 35 ms | PASS |

跨架构 `cargo check`（LoongArch + RISC-V）通过。没有以测试耗时声称性能收益。

最终证据：

```text
testsuits-final/.tmp/final-runs/20260806-fs-lifetime-vma-only-final-40/
```

以下是 AI 的具体分析，作为存档。

---

## 历史分析背景

这是一个经典的"rename 覆盖 + open-unlinked"语义问题。旧实现在 rename 路径里直接调用 `parent.unlink(name)`，绕过了 unlink syscall 里已有的 deferred-unlink 保护。问题只有在"先 mmap 不触页、rename 后再 fault"的时序下才稳定复现。下面保留测试设计、Linux 对照细节和 BuildStorm perf 边界说明。

## 1. 结论

`rename(source, target)` 覆盖已有普通文件时，旧实现直接对 `target` 调用
`ext4-fs::Inode::unlink()`。当前 ext4 实现把最后目录项删除与磁盘 inode 回收耦合，
因此仍由打开的 fd 或 file-backed VMA 引用的旧对象也会立即失效。延迟 fault 的
`MAP_PRIVATE` 映射读到零，旧 fd 的 `pread` 返回 EOF，而新路径已经正确指向 source。

本次按 Linux 的 dentry/inode/file/VMA 分层语义修复：rename 只替换命名空间中的目录项；
旧 inode 必须由打开的 file description 或 VMA 的 `vm_file` 引用继续存活。由于当前
ext4-fs 尚未原生拆开 dentry 删除和 inode 回收，普通文件目标复用已有的 deferred-unlink
兼容层：先把旧目录项原子地改为隐藏名，最后一个打开引用消失后再真正 unlink。

该修改位于通用 rename 目标删除路径，同时覆盖同目录与跨目录 rename，不识别测试名，
也不改变目录目标、未打开普通文件、`RENAME_NOREPLACE` 或 `RENAME_EXCHANGE` 的既有分支。

## 2. Linux 参考语义

本地 Linux 参考树为 `exampleOs/linux`，版本 `4549871118cf`。

| Linux 位置 | 可观察边界 | 本次实现 |
| --- | --- | --- |
| `fs/namei.c:vfs_rename()` | 锁定 source/target inode 后调用文件系统 rename，再移动或交换 dentry | rename 仍在现有父/子 inode 锁序内完成 |
| `fs/inode.c:iput()` / `iput_final()` | link count 归零不等于有引用的 inode 立即释放；最终引用归零才进入 eviction | 打开引用存在时先保留隐藏目录项，最后引用关闭时清理 |
| `mm/vma.c` 的 `vm_file` get/fput | file-backed VMA 独立持有 file 引用，关闭 fd 不应破坏后续 fault | VMA 持有的 `OSInode` 继续计入同一 open-description 生命周期 |
| `mm/filemap.c:filemap_fault()` | fault 通过 `vma->vm_file->f_mapping` 找回旧 inode/page cache | rename 后的延迟 fault 仍通过原 backing 读取旧对象 |

这里复制的是 Linux 的对象生命周期和锁边界，不是照搬 VFS 内部结构。隐藏名只是
ext4 过渡期兼容机制；当 ext4-fs 原生支持 link count 为零但 inode 引用仍非零时，应把
该兼容层替换为真正的 dentry/inode 分离。

## 3. 根因与实现

旧的 `remove_rename_target(parent, name)` 无条件执行 `parent.unlink(name)`。它没有接收
已锁定的 target inode，也无法查询现有 `InodeLifetimeState`，所以绕过了 unlink syscall
已经具备的 open-unlinked 生命周期保护。

现在该 helper 同时接收 target：

1. 对普通文件调用 `defer_unlink_open_file(parent, name, target)`；
2. 若存在打开的 file description，先取得 `DeferredUnlinkReservation`，关闭
   last-close-vs-rename 竞争窗口；
3. 把可见目标改名为唯一 `.ltp_orphan.<pid>.<seq>`，提交 deferred cleanup；
4. source 随后按原路径改名到 target，可见命名空间立即得到新对象；
5. fd、epoll/SCM_RIGHTS 引用或 VMA backing 的最后一个 `Arc<OSInode>` 消失时，清理
   隐藏目录项并回收旧 inode；
6. 没有打开引用时仍直接 unlink，不增加正常 rename 的持久对象。

复用现有 reservation 还避免恢复早期每次 unlink 扫描所有进程和 fd 的
`O(processes * fds)` 实现。

## 4. 单一证明测试

新增并注册：

```text
user/src/bin/smoke_archive/rename_over_mmap_lifetime_smoke.rs
```

每轮创建内容不同的 source/target，打开并 mmap 旧 target，但在 rename 前不触碰映射，
确保读取发生在目录项被覆盖后的 lazy file fault。32 轮分为两类：

- 16 轮保留旧 fd，同时验证旧 fd、旧 VMA 和新路径三个视图；
- 16 轮在 mmap 后立即关闭旧 fd，只让 VMA 的 file 引用维持旧 inode。

修复前首轮稳定失败：

```text
RENAME_OVER_MMAP_LIFETIME_FAIL stage=verify iteration=0 \
expected_old=0x31 mapped=0x0 fd=None expected_new=0xc1 replacement=Some(193)
```

证据：

```text
testsuits-final/.tmp/final-runs/20260806-rename-lifetime-baseline-28/serial.log
```

修复后及加强后的最终结果：

```text
RENAME_OVER_MMAP_LIFETIME_PASS iterations=32 vma_only=16 fd_and_vma=16
```

最终证据：

```text
testsuits-final/.tmp/final-runs/20260806-fs-lifetime-vma-only-final-40/
```

## 5. 聚焦回归与静态检查

LoongArch64、12 vCPU、8 GiB、snapshot 的最终聚焦组 3/3 通过：

| 测试 | host elapsed | 结果 |
| --- | ---: | --- |
| `open_unlink_lifetime_smoke` | 186 ms | PASS，6 workers × 32 iterations |
| `rename_over_mmap_lifetime_smoke` | 67 ms | PASS，16 VMA-only + 16 fd/VMA |
| `shared_file_truncate_cache_smoke` | 35 ms | PASS |

测试均有单项硬超时；QEMU 在组结束后正常终止，没有残留实例。跨架构检查通过，输出
只有仓库既有 warning：

```zsh
TMPDIR=$PWD/.tmp ARCH=loongarch64 cargo check --manifest-path os/Cargo.toml
TMPDIR=$PWD/.tmp ARCH=riscv64 cargo check --manifest-path os/Cargo.toml \
  --target riscv64gc-unknown-none-elf
cargo check --manifest-path user/Cargo.toml \
  --target loongarch64-unknown-none-softfloat \
  --bin rename_over_mmap_lifetime_smoke
cargo check --manifest-path user/Cargo.toml \
  --target riscv64gc-unknown-none-elf \
  --bin rename_over_mmap_lifetime_smoke
```

## 6. BuildStorm 与 perf 边界

rename 修复后的 BuildStorm 诊断仍严格区分“性能定位”和“功能结论”。带 `-perfmap` 的
15 秒 perf 样本共约 3K、lost 0，热点主要是 `gelf_getshdr` 14.66%、`tb_gen_code`
10.29%、`gelf_getsymshndx` 8.96%，表明长时间 perfmap 会显著放大 QEMU JIT 符号查询，
不能作为正式耗时基准。使用方式和硬截止规则已写入 `testsuits-final/AGENTS.md`。

无 perfmap 的一次运行曾在 rustc 安装 ctrl-c 线程时报间歇性 code 11；后续两次精确
BuildStorm 窗口、一次 128 次双路 rustc exec 回归都未复现。诊断期间同时覆盖了线程栈
`mmap`、`mprotect` 和 thread-like `clone` 的失败分支，未捕获失败来源；宿主机最低仍有
约 21 GiB 可用内存。所有运行均在 20 秒探针或 120/180 秒 workload 硬截止内停止。

因此本批次不把 code 11 归因于某个未经证明的内核分支，也不声称完整 BuildStorm 已经
通过；临时诊断代码已全部撤除。后续若再次出现，应保留现场、先对准确 QEMU PID 采集
短 perf，再依据实际失败分支参考 Linux 修复。
