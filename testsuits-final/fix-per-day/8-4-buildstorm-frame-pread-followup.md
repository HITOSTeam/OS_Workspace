# 8-4 BuildStorm 物理页回收与 pread 并发修复

## 问题概述

本批包含两个独立瓶颈。物理页分配器在全局、保存中断状态的自旋锁内维护
`BTreeSet` 来检查重复释放；每次页回收都可能为树节点再次进入内核堆。另一方面，
`pread64()` 虽然使用调用者提供的 offset，不需要共享文件位置，却仍取得
`OSInodeInner` 自旋锁并在锁内等待 ext4/块输入输出；其他核心会围绕一个已经让出
处理器的 owner 持续自旋。

## 背景知识

先打个比方：这项测试像一间突然接到大单的厨房。很多厨师同时开火、叫帮手、
换工具、翻配方、取原料，任何一处共用设施卡住，整间厨房都会慢下来。

`BuildStorm`（并发编译压力负载）就是这样的场景。它会同时编译许多 Rust crate
（Rust 软件包），频繁执行 `fork`（复制进程）、`exec`（替换进程映像）和文件读写。
因此它不是只压处理器：进程创建与退出、文件系统访问、物理页分配和回收会一起受压。

再看读文件。可以把文件当前位置想成夹在书里的一个共用书签。两个人共用这本书，
如果“移动书签”和“开始读”是两个动作，中间就可能被另一个人插进来：

```text
两个线程共用 fd（文件描述符），共用书签开始在文件开头

线程甲: lseek（移动共享文件位置）到偏移甲
                 | 发生切换
线程乙: lseek（移动共享文件位置）到偏移乙
线程乙: read（从当前位置读取）       -> 从偏移乙读取，书签继续向后移动
线程甲: read（从当前位置读取）       -> 本想读偏移甲，实际从乙留下的位置读
```

这就是 `lseek` 加 `read` 的两步问题。两次独立的系统调用之间没有保护；只要线程共享
同一个文件位置，后一次 `lseek` 就能改掉前一个线程准备使用的位置。

`pread`（按指定偏移读取）把位置直接当参数传入，不碰共用书签：

```text
线程甲: pread（按指定偏移读取，偏移甲） -> 只读偏移甲
线程乙: pread（按指定偏移读取，偏移乙） -> 只读偏移乙
共用书签保持不变
```

从调用者角度看，指定位置和完成读取是一次 atomic operation（原子操作），不会在
“先定位、后读取”的缝隙里被别的线程改掉位置。

接着看 `fork` 后为什么会共用书签。可以把它想成父子各拿到一把钥匙：钥匙编号写在
各自的钥匙清单里，但两把钥匙打开的是同一个带书签的柜子。

```text
父进程的 fd table（文件描述符表）: [fd] -----\
                                                   +--> open file description（打开文件描述对象）
子进程的 fd table（文件描述符表）: [fd] -----/         即 struct file（Linux 内核中的打开文件对象）
                                                        |
                                                        +--> file offset（文件偏移量）
                                                        +--> inode（索引节点）
```

正式术语是：`fork` 会让子进程得到新的文件描述符表，但对应的 file descriptor
（文件描述符）表项仍指向同一个 open file description。这个对象在 Linux 中就是
`struct file`（Linux 内核中的打开文件对象），共享的 file offset 就保存在其中。
所以父进程读完后，子进程看到的位置也会变化，反过来也一样。

`dup()`（复制文件描述符）也指向同一个 open file description，因此同样共享位置。
但用 `open()`（重新打开路径）再次打开同一路径，会创建新的 open file description；
两次打开看到的是同一个文件内容，却各有自己的 file offset，互不移动对方的书签。

回到这次问题：CongCore 的 `pread64`（带显式偏移的读取系统调用）明明不用共用书签，
却仍会取得覆盖共享文件位置和 read buffer（读取缓冲区）的 `spinlock`（自旋锁）。
一旦持锁线程在块读取中等待，其他线程即使带着各自的偏移量，也只能围着这把锁自旋。
修复的关键就是把不需要共享位置的 pread 路径从这把大锁里拆出来，同时保留文件内容
层面的读写协调。

## 如何发现

frame 路径由 fork 微基准与源码审查定位：最后一个 `FrameOwner`、写时复制、进程退出
和 page-cache 回收都会在 allocator 锁内修改 `BTreeSet`。positional read 则由冷
`rustc -vV` 和 `tg-xtask` 现场定位；旧日志 180 秒仍为 0 行/0 deps，调用链停在共享
read buffer owner。命令和证据：

```sh
rg -n 'BTreeSet|StackFrameAllocator|recycle' os/src/mm/frame_allocator.rs
rg -n 'pread_at|read_buf|OSInodeInner|try_lock' os/src/fs/inode.rs
ARCH=loongarch64 SMP=12 MEM=8G IMAGE_MODE=snapshot ./run.sh shell
ARCH=riscv64 SMP=8 MEM=8G IMAGE_MODE=snapshot ./run.sh shell
```

```text
.tmp/final-runs/20260804-022549-loongarch64-shell/serial.log
.tmp/final-runs/20260804-024729-loongarch64-shell/serial.log
.tmp/final-runs/20260804-024923-loongarch64-shell/serial.log
.tmp/final-runs/20260804-040625-loongarch64-shell/serial.log
.tmp/final-runs/20260804-043028-riscv64-shell/serial.log
.tmp/iozone/8-4-heap-sharded-3run.log
.tmp/iozone/8-4-frame-bitmap-only-3run.log
```

## 怎么解决

frame allocator 保留后进先出 free stack，但把重复释放检查改为启动期预分配的一位/
物理页号 bitmap：push 前检查并设置 bit，pop 时检查并清除 bit，不一致仍 panic。
bitmap 只在 `init/add_range` 扩容，运行期 recycle 不分配堆内存。8 GiB guest 的绝对
页号 bitmap 约 320 KiB。

只读 positional read 的边界改为：

```rust
let _inode_read = inode.read_semaphore();
if let Some(mut inner) = self.inner.try_lock() {
    // 无竞争：继续使用已有 128 KiB 有界预读缓存
    inner.pread_cached(offset, buf)
} else {
    // 缓存 owner 正在等待输入输出：从稳定 Arc<Inode> 直接读
    self.inode.read_at(offset, buf)
}
```

可写描述仍取得 inode 写信号量、刷新写缓冲并串行化，保持读写一致性。Linux
`ksys_read()` 通过 `fdget_pos()` 串行化 `file->f_pos`，`ksys_pread64()` 则把私有
offset 直接传入 `vfs_read()`；文件页协调在 address-space/page-cache 层完成，不用
文件位置锁覆盖块输入输出。CongCore 尚未完全统一 inode page cache，因此保留原缓存
作为无竞争快路，竞争时直读是最小正确过渡。

整把 `OSInodeInner` 改可睡眠锁、所有 pread 永远直读、额外第二个 PreadCache 都经
IOZone 或运行现场证明退化后回滚，没有进入提交。

## 对应提交

- `6641e0a mm: avoid heap allocation while recycling frames`。
- `b0185b3 fs: avoid spinning on contended positional reads`。
- 用户回归：`a2a4d1c7 test: cover concurrent positional reads`。
- 顶层集成：`13140f63 kernel: integrate frame and positional-read fixes`。
- 文档提交：`634f4546b44a25824f0b18c887d13938f33d1e63`。

## 对比提升

frame bitmap 使 fork 微基准中位数 `139321 -> 131397 us`（-5.7%）；RISC-V
IOZone 中位总耗时 `19.15 -> 18.79 s`。最终冷 `rustc -vV` 为 1.09 s、rc=0；
`tg-xtask` 在 803 秒达到 154 行/419 deps，超过旧 900 秒窗口后的 130/371，但仍超时。
8 线程共享文件描述符的 `pread64` 回归校验 4 MiB，返回 0。完整 BuildStorm 未运行。

以下是 AI 的具体分析，作为存档。

---

## 历史分析背景

下面保留本批次当时的完整长分析，因为它记录了候选方案、运行证据和仍未解决的边界。
这些细节方便后续复盘，也能避免把阶段性改善或超时结果误写成完整通过。

## 1. 结论

本批次没有运行完整 BuildStorm，也没有把超时的 `tg-xtask` 写成通过。最终保留两项
彼此独立、均有运行态证据的内核修改：

1. frame allocator 用预分配 bitmap 记录 free-stack 成员，删除持有全局 irq-save
   allocator 锁时的 `BTreeSet` 节点分配/释放；
2. 只读 `pread64()` 在共享 open-file-description 的小型 readahead cache 发生竞争时，
   直接从稳定 inode 读取，不再让其他 hart 自旋等待一个正在 ext4/block I/O 中让出的
   cache owner；无竞争路径仍复用原缓存，没有引入第二份缓存。

结果摘要：

- 16 线程进程执行 128 次 `fork()+waitpid()` 的五轮中位数从 `139321 us` 降到
  `131397 us`，改善 5.7%；
- RISC-V 聚焦 IOZone 的三轮总耗时中位数从 `19.15 s` 降到 `18.79 s`；
- LoongArch 冷启动 `/root/.cargo/bin/rustc -vV` 用时 `1.09 s`，返回 0；
- `tg-xtask` 在启动后约 601 秒已达到 141 行 Cargo 进度、409 个 deps 文件，超过旧
  最佳窗口在 900 秒超时后采集到的 130/371；最后一个明确位于 900 秒窗口内的样本
  为 154/419；
- `tg-xtask` 仍在 900 秒超时，不能声称 BuildStorm 已完成；
- 新增 8 线程共享 FD 的 `pread64` 回归，合计读取并逐字节校验 4 MiB，RISC-V 运行态
  返回 0；另 7 项文件缓存、lazy mmap、wait/exit 和 open-unlinked 回归在 LoongArch
  均返回 0。

## 2. 版本与资产

| 资产 | 值 |
| --- | --- |
| `os/` 基线 | `d0679eb386789824b66f8bc7988e8699f85e9f9c` |
| frame bitmap 提交 | `6641e0a` (`mm: avoid heap allocation while recycling frames`) |
| positional-read 提交 | `b0185b3` (`fs: avoid spinning on contended positional reads`) |
| final 测试分支/commit | `final-2026` / `1eac61d3becaa592c8ef12a7535f0ec6bb9e3e36` |
| 本地 Linux 参考树 | `exampleOs/linux` / `fc02acf6ac0ccde0c805c2daa9148683cdd01ba8` |
| QEMU | 11.0.3 |
| LoongArch 运行 | 12 vCPU，8 GiB，snapshot |
| RISC-V final 回归 | 8 vCPU，8 GiB，snapshot |
| RISC-V IOZone | 8 vCPU，2 GiB，snapshot |
| LoongArch 镜像 SHA-256 | `2ad9d955684297abe9db48d94f1b7fcc488268fc8f481408c55b1ec27f520c6a` |
| RISC-V 镜像 SHA-256 | `d899fe43d333d1d17ad8a5f8a8b74b68117b8c1ceacfc3843bfeadb1ca705bd1` |

两次 final 启动均由 `run.sh` 重新扫描并确认 14 GiB 镜像校验值。所有 QEMU 运行使用
snapshot，没有修改基准镜像。测试结束后 `user/.cargo/config.toml` 和 `/user` 镜像已
恢复为 RISC-V 默认构建。

## 3. frame recycle 根因与 Linux 对照

旧 `StackFrameAllocator` 同时维护：

- `Vec<usize>`：实际 free stack；
- `BTreeSet<usize>`：检查重复释放。

每次 frame recycle 都在全局 `FRAME_ALLOCATOR` irq-save spinlock 内创建或释放一枚
`BTreeSet` heap node。frame owner 的最后一个 `Arc` 释放、COW、进程退出和 page-cache
回收都会走这条路径，因此多核 fork/编译会把内存释放再次嵌套成 heap allocator 热点。

Linux 不会为每次 buddy free 临时分配一枚树节点；页的 buddy/free 状态保存在预分配的
`struct page` 元数据中，可参考：

- `exampleOs/linux/mm/page_alloc.c`；
- `exampleOs/linux/include/linux/mm_types.h`；
- `exampleOs/linux/include/linux/page-flags.h` 的 buddy page 状态。

本项目尚不需要完整 zone/buddy/PCP。保留原 LIFO free stack，只增加一 bit/PPN 的
预分配 bitmap：

- push 前原子地检查并设置 free bit；
- pop 时检查并清除 free bit；
- 重复释放和 free-stack/bitmap 不一致继续 panic，不降低诊断能力；
- bitmap 扩容只发生在启动期 `init/add_range`，运行期 recycle 不分配 heap。

按当前物理地址布局，8 GiB guest 的绝对 PPN bitmap 约 320 KiB；这是固定、可预估的
元数据成本。

## 4. positional read 根因与 Linux 对照

`OSInodeInner` 同时保存共享文件位置和 128 KiB read buffer。原 `pread_at()` 即使不
访问共享 offset，也会拿 `spin::Mutex<OSInodeInner>`，并在持锁期间进入 ext4
`Inode::read_at()`。块 I/O 会协作让出 CPU；此时其他 hart 对同一 description 的
page fault/`pread64()` 只能持续自旋。

本地 Linux 参考树体现了两个关键边界：

- `fs/read_write.c::ksys_read()` 使用 `CLASS(fd_pos, ...)`，由 `fdget_pos()` 串行化
  `file->f_pos`；
- `fs/read_write.c::ksys_pread64()` 使用普通 `CLASS(fd, ...)`，把调用者私有 `pos`
  传给 `vfs_read()`，不拿 `f_pos_lock`；
- `mm/filemap.c::filemap_read()` 在 address-space/page-cache 层协调缺页，而不是用
  open-file-description position lock 包住块 I/O。

本项目的最小对应实现是：

1. `OSInode` 在共享 position/cache 锁之外保存稳定 `Arc<Inode>`；
2. 只读 positional read 仍先取得 per-inode read semaphore，阻止与 truncate/write
   冲突；
3. `inner.try_lock()` 成功时走原有 bounded readahead cache；
4. cache 已被另一个 positional reader 占用时，直接 `inode.read_at(offset, buf)`；
5. writable description 仍取得 inode write semaphore、flush buffer 并串行化，不改变
   write/read coherence。

该实现不复制 Linux 内部 folio/xarray 结构，但保持 Linux 可观察的 positional-read
语义，并避免在 spinlock owner 让出 CPU 时制造跨 hart 自旋。

## 5. 被否决的方案

本批次先后实测并回退了下列候选，最终提交不包含它们：

| 候选 | 结果 | 决定 |
| --- | --- | --- |
| 整个 `OSInodeInner` 改 sleeping mutex | 冷 rustc 可运行，但 IOZone 明显退化，锁范围仍覆盖无关 offset/目录状态 | 回退 |
| 所有 `pread` 永远直读 | 删除 readahead 后 IOZone 退化 | 回退 |
| 额外 `PreadCache` + try-lock | LoongArch 有进度，但相邻 IOZone 三轮总耗时中位数为 19.72 s，且多项吞吐下降 | 回退 |
| 单一原缓存 + 竞争时直读 | A/B/A 总耗时中位数 19.03 / 19.03 / 19.57 s，无可归因回退；真实编译进度显著领先 | 保留 |

IOZone 各阶段吞吐在相邻 QEMU 启动之间波动较大，因此这里只用 A/B/A 排除明显回退，
不把单次 phase 数字包装成 positional-read 的 IOZone 加速比。

## 6. 性能证据

### 6.1 frame recycle：fork 微基准

LoongArch，12 vCPU，8 GiB，五轮，每轮由 16 线程进程执行 128 次
`fork()+waitpid()`：

| 版本 | 五轮 `elapsed_us` | 中位数 |
| --- | --- | ---: |
| 仅既有 fork live-count 优化 | 144428, 141608, 139321, 135391, 133206 | 139321 |
| 加 frame bitmap | 137448, 131397, 131830, 131043, 130945 | 131397 |

中位数改善 `(139321 - 131397) / 139321 = 5.7%`。

日志：

```text
.tmp/final-runs/20260804-022549-loongarch64-shell/serial.log
.tmp/final-runs/20260804-024729-loongarch64-shell/serial.log
```

### 6.2 frame recycle：IOZone

RISC-V，8 vCPU，2 GiB，4 worker × 4 MiB，1 KiB record，sequential workload
三轮中位数：

| 版本 | 总耗时/s | initial write | rewrite | read | reread |
| --- | ---: | ---: | ---: | ---: | ---: |
| bitmap 前 | 19.15 | 5154.16 | 17082.61 | 14715.09 | 14573.42 |
| bitmap 后 | 18.79 | 5341.10 | 17334.35 | 15249.01 | 15485.81 |

吞吐单位为 KiB/s。日志：

```text
.tmp/iozone/8-4-heap-sharded-3run.log
.tmp/iozone/8-4-frame-bitmap-only-3run.log
```

### 6.3 LoongArch rustc 与 tg-xtask

最终日志：

```text
.tmp/final-runs/20260804-040625-loongarch64-shell/serial.log
```

冷 `rustc -vV`：

```text
RUSTC_VV_MINTRY start=20.97 end=22.06 rc=0
```

旧的持锁 I/O 现场曾让 Cargo 的启动探针在 180 秒内保持 0 行/0 deps；该现场位于：

```text
.tmp/final-runs/20260804-024923-loongarch64-shell/serial.log
```

同一 final 镜像、12 vCPU、8 GiB、snapshot 下的进度对比：

| 版本 | 采样时间（从 cargo 启动） | Cargo 行 | deps 文件 | 结果 |
| --- | ---: | ---: | ---: | --- |
| 旧最佳（仅 fork live-count） | 超时后约 26 s | 130 | 371 | timeout |
| 最终版本 | 500 s | 131 | 358 | running |
| 最终版本 | 601 s | 141 | 409 | running |
| 最终版本最后一个明确在窗口内的样本 | 803 s | 154 | 419 | running |
| 最终版本超时后诊断采样 | 937 s | 163 | 459 | rc=143 |

旧最佳日志：

```text
.tmp/final-runs/20260804-020444-loongarch64-shell/serial.log
```

最后一行采样发生在 timeout 后，仅用于诊断，不能当作 900 秒内的精确成绩。即便只看
803 秒的窗口内样本，最终版本也已超过旧版本超时后的 130/371。`tg-xtask` 本身仍未
完成，完整 BuildStorm 未运行。

## 7. 正确性回归

新增 `concurrent_pread_smoke`：

- 创建并填充 1 MiB ext4 文件；
- 关闭 writable FD，再以 read-only 方式打开；
- 8 个同线程组、共享 FD 的线程同时从不同 readahead window 做 128 次 4 KiB
  `pread64()`；
- 合计 4 MiB 数据逐字节校验，不设置性能阈值。

RISC-V 8 hart 输出：

```text
CONCURRENT_PREAD_PASS workers=8 reads=128 bytes=4194304
CONCURRENT_PREAD_RC=0
```

日志：

```text
.tmp/final-runs/20260804-043028-riscv64-shell/serial.log
```

LoongArch `tg-xtask` 后执行以下 7 项，全部返回 0：

```text
fork_thread_group_perf_smoke.bin
exec_epoll_thread_smoke.bin
wait_wakeup_race_smoke.bin
private_file_page_cache_smoke.bin
file_mmap_lazy_fault_smoke.bin
shared_file_truncate_cache_smoke.bin
open_unlink_lifetime_smoke.bin
```

RISC-V、LoongArch 内核与新增用户测试均通过 `cargo check`；`cargo fmt --check`、
`rustfmt --check` 和 `git diff --check` 通过。

## 8. 尚未解决的 blocker

`tg-xtask` 仍未在 900 秒内完成。timeout 后现场留下两组：

```text
opt cgu.0  S  -> cc Z
opt cgu.0  S  -> cc Z
```

两个 `opt cgu.0` 各有 4 个 sleeping thread，子 `cc` 已成为 zombie。杀死 orphan
compiler 后，这些进程又被 reparent 到 PID 0 并保持 zombie。该现象提示下一步应聚焦：

1. 在不先杀死 Cargo 的窗口中确认 compiler 是否已阻塞在 `wait4()`，避免把 timeout
   产生的 orphan 误判成原始根因；
2. 对照 Linux `kernel/exit.c::exit_notify()`、`forget_original_parent()` 与
   `kernel/exit.c::do_wait()`，检查子进程退出发布、parent wait queue 唤醒、线程组
   wait owner 和 reparent-to-reaper；
3. 增加“多线程父进程并发 spawn/exec/wait、子进程快速退出”的通用回归；
4. 该路径稳定后重新跑 `tg-xtask`，只有它完整返回 0 后才进入完整 BuildStorm。

目前证据只能说明 wait/reparent 是下一处高价值调查点，尚不能断言它就是唯一根因。

## 9. 验证命令摘要

```sh
TMPDIR=$PWD/.tmp cargo check --manifest-path ../os/Cargo.toml \
  --target riscv64gc-unknown-none-elf
TMPDIR=$PWD/.tmp cargo check --manifest-path ../os/Cargo.toml \
  --target loongarch64-unknown-none-softfloat

cargo check -p user --bin concurrent_pread_smoke \
  --target riscv64gc-unknown-none-elf
cargo check -p user --bin concurrent_pread_smoke \
  --target loongarch64-unknown-none-softfloat

ARCH=loongarch64 SMP=12 MEM=8G IMAGE_MODE=snapshot ./run.sh shell
ARCH=riscv64 SMP=8 MEM=8G IMAGE_MODE=snapshot ./run.sh shell

cargo fmt --manifest-path ../os/Cargo.toml -- --check
rustfmt --edition 2024 --check \
  ../user/src/bin/smoke_archive/concurrent_pread_smoke.rs
git -C ../os diff --check
git diff --check
```

没有运行完整 BuildStorm、完整 LTP、CAgent、unixbench、libcbench 或完整 IOZone 套件。
