# 8-3 Linux 式文件页缓存与 MAP_PRIVATE COW 修复

## 1. 结论

本批次针对 LoongArch BuildStorm 的 `tg-xtask` 聚焦构建极慢问题，确认主要放大器
不是普通死锁，而是普通文件 `MAP_PRIVATE` 缺少 Linux 式 inode page cache：每个
rustc 进程都会独立分配并重新读取同一份约 271 MiB 的 `librustc_driver.so`。

本次参考 Linux `filemap_fault()`、`do_read_fault()` 和 `do_cow_fault()`，完成了以下
通用修复：

- 普通文件 `MAP_SHARED` 与尚未写入的 `MAP_PRIVATE` 映射共用 inode 级干净页；
- `MAP_PRIVATE` 首次写入通过已有三阶段 COW 路径复制成匿名私有页；
- 同一 `(dev, ino, page index)` 的并发首次 fault 只有一个任务执行文件 I/O，其他
  任务睡眠等待发布；
- mmap 和 mremap 不再在系统调用内 eager 读取整个私有文件映射；
- fd write 更新 inode page cache，但不会覆盖已经 COW 的私有页；
- truncate 会清零 EOF 页尾、丢弃越界页，并使正在读入的页失效后唤醒等待者；
- 内存压力回收只移除没有 PTE/MapArea 引用的 Ready 干净页，不回收 Loading 页。

最终 12 核、8 GiB LoongArch snapshot 中，以下三个聚焦用例全部通过：

- `private_file_page_cache_smoke`；
- `private_file_madvise_dontneed_smoke`；
- `shared_file_alias_smoke`。

LoongArch64 与 RISC-V64 的内核 `cargo check`、三个用户态回归的双架构
`cargo check`、`git diff --check` 均通过。

本批次按用户要求**没有运行完整 BuildStorm**。`cargo build -p tg-xtask` 的 10 分钟
聚焦窗口仍以 `timeout` 退出，不能写成 BuildStorm 或 `tg-xtask` 已通过。不过块读取
放大已经显著下降：旧实现第一个约 180 秒窗口新增 `3,305,463,808` B 读取；新实现
一次 600 秒诊断窗口只新增 `69,685,248` B。另一次 120 秒聚焦运行能够启动 12 路
rustc 并生成 16 个依赖产物，启动以来总块读取为 `251,760,640` B。

因此本批次的严谨结论是：

> 重复读取大型 Rust DSO 的页缓存根因已修复，Linux 式 MAP_PRIVATE COW 语义已通过
> 聚焦回归；`tg-xtask` 仍未在限定时间内完成，完整 BuildStorm 就绪度仍然未知。

## 2. 工作树与资产

### 2.1 源码基线

本批次继续在独立工作树中完成：

```text
工作树：.tmp/worktrees/loongarch-linux-fix
分支：  loongarch-linux-fix
```

进入本轮修复前的版本：

| 资产 | commit |
| --- | --- |
| 顶层源码 | `63352ce73ce7499b5747da4b7e592c47fb44814d` |
| `os/` | `90625864bb7d6c9de62a9b96538bc0e84a3c078e` |
| final test source | `1eac61d3becaa592c8ef12a7535f0ec6bb9e3e36` |
| 本地 Linux 参考树 | `fc02acf6ac0ccde0c805c2daa9148683cdd01ba8` |

final test source 保持在 `final-2026` 分支。本次没有拉取、替换测试源码或修改评分
脚本。

### 2.2 运行资产

| 项目 | 值 |
| --- | --- |
| 架构 | LoongArch64 |
| vCPU / 内存 | 12 / 8 GiB |
| QEMU | 11.0.3 |
| 镜像 | `sdcard-la-pub.img`，14 GiB raw ext4 |
| 镜像 SHA-256 | `2ad9d955684297abe9db48d94f1b7fcc488268fc8f481408c55b1ec27f520c6a` |
| image mode | QEMU `-snapshot` |
| guest toolchain | nightly-2026-05-28 LoongArch GNU |

所有 guest 写入均在 snapshot 中丢弃，没有直接写入基准镜像。用于测试生成的
`user.ext4` 和串口日志位于 `testsuits-final/.tmp/`，不进入版本控制。最终收尾时
`user/.cargo/config.toml` 已恢复为原来的 RISC-V target，临时 `DEBUG_PERF` 已恢复
为 `false`。

## 3. 初始失败与量化证据

旧实现有一个全局文件页缓存，但仅服务普通文件 `MAP_SHARED`。普通文件
`MAP_PRIVATE` 在 `mmap()` 内建立 Framed area，并用 512 B 临时缓冲 eager 读取整个
映射。结果是：

1. 每个 rustc 都为同一 DSO 分配独立物理页；
2. 每个进程都从 ext4/块设备重新读一份映射内容；
3. 12 路 rustc 同时启动时，块缓存容量不足以保留 271 MiB DSO；
4. 大文件读取在进程之间相互驱逐，形成持续重复 I/O。

基线日志：

```text
testsuits-final/.tmp/final-runs/
  20260802-234242-loongarch64-shell/serial.log
```

该日志在 `cargo build -p tg-xtask` 前记录：

```text
block_read_bytes: 6176768
```

约 180 秒后的第一个采样为：

```text
block_read_bytes: 3311640576
```

增量为：

```text
3,311,640,576 - 6,176,768 = 3,305,463,808 B
```

同一采样中有 12 个 rustc 同时编译不同 crate，命令行都加载相同 Rust 工具链。
这说明“串口暂时无输出”并不是唯一问题；底层已经在用约 18 MB/s 的速率重复读取
文件内容。

## 4. Linux 参考语义

参考树为本地 Linux commit：

```text
fc02acf6ac0ccde0c805c2daa9148683cdd01ba8
```

主要对应关系如下：

| Linux 机制 | 本地参考 | 本次实现 |
| --- | --- | --- |
| 文件 fault 查找/创建 folio | `mm/filemap.c:filemap_fault()`、`filemap_get_folio()` | `(dev, ino, file_page)` 全局 cache |
| folio 锁与并发 miss 合并 | `FGP_CREAT | FGP_FOR_MMAP`、locked folio | `Loading/Ready` slot + `WaitQueue` |
| 干净私有文件页 | `mm/memory.c:do_read_fault()` | 多个 mm 映射同一只读 cache frame |
| 私有写 fault | `mm/memory.c:do_cow_fault()` | 三阶段 snapshot/allocate/recheck COW |
| truncate 失效 | address_space page invalidation | Loading invalidation、EOF 清零和越界删除 |
| clean page reclaim | page-cache reclaim | OOM 前回收 refcount 仅为 cache 自身的 Ready 页 |

这里复制的是 Linux 的用户可观察语义与同步边界，不是逐行移植 Linux 内部结构。

## 5. 实现

### 5.1 inode 级文件页缓存

`os/src/mm/memory_set/backing.rs` 将旧的 shared-only cache 泛化为普通文件页缓存：

```text
FilePageCacheKey(dev, ino, file_page)
    -> Loading(frame, state)
    -> Ready(frame)
```

首次 miss 在进行 I/O 前先发布 Loading slot。并发 fault 看到 Loading 后进入调度器
感知的 WaitQueue，不持有 cache lock 或 mm lock 自旋。owner 在 mm lock 之外完成
整页 `pread_at()`，然后以 Release 顺序发布 Ready 并唤醒全部等待者。

缓存始终填充完整文件页；`pread_at()` 在 EOF 停止，frame 的剩余区域保持零填充。
这避免让同一 inode page 的内容取决于第一个触发 miss 的 VMA。

### 5.2 MAP_PRIVATE 干净页与 COW

`os/src/mm/memory_set/fault.rs` 对普通文件私有映射安装只读、带 `PTEFlags::COW` 的
cache frame。即使 VMA 当前只读，也保留 COW 标记，以便后续
`mprotect(PROT_WRITE)` 仍不会直接写 inode cache page。

写 fault 复用已有三阶段 COW：

```text
mm lock 下快照 VMA/PTE/旧 frame
        ↓
释放 mm lock，分配并复制新 frame
        ↓
重新取得 mm lock，校验 VMA/PTE 后替换映射
```

只有校验仍匹配时才提交新 PTE；旧 frame 保持 pin，直到对应 TLB invalidation 完成。

### 5.3 mmap 与 mremap 改为 lazy

`os/src/syscall/memory/mmap.rs` 删除普通文件私有映射的 eager populate。普通文件的
`MAP_SHARED` 和 `MAP_PRIVATE` 都建立 Lazy area，mremap 扩展部分同样等到 fault 再
通过 inode cache 具现化。

这不仅减少系统调用延迟，也避免把未访问的大型 DSO 全部读入内存。

### 5.4 fd write 与私有快照

首次语义回归发现：第一个私有映射写成 `B` 后，`pwrite(C)` 又把它覆盖成 `C`。
根因不是 COW PTE 提交失败，而是旧的 fd-write 虚拟地址镜像路径会遍历所有文件
VMA，包括已经 COW 的 `MAP_PRIVATE`。

修复后：

- inode page cache 接收 fd write 更新；
- 未 COW 的干净私有映射因为引用 cache frame 而看到更新；
- 已 COW 的私有页保持匿名快照；
- 旧的按用户虚拟地址复制路径只处理 `MAP_SHARED`。

这与 Linux 的私有映射可观察语义一致。

### 5.5 truncate、并发与回收

truncate 与正在进行的 cache fill 并发时，resize 会移除 Loading slot、发布
Invalidated 并唤醒等待者。owner 完成 I/O 后发现自己已不再拥有 slot，也返回
Invalidated。fault 随后重建 VMA/EOF 快照，而不是把过期页重新发布。

当前还没有完整 LRU。为避免无界 pin 住未使用页，frame allocator 分配失败前会
回收只由全局 cache 持有的 Ready 页；Loading 页和仍被 PTE/MapArea 引用的页不会
被回收。

## 6. 聚焦回归

新增 `private_file_page_cache_smoke` 覆盖：

1. 同一文件建立两个 `MAP_PRIVATE | PROT_READ | PROT_WRITE` 映射；
2. 两个映射首先读到相同文件内容；
3. 写第一个映射后，第二个映射与文件保持不变；
4. `pwrite()` 更新文件后，已 COW 的第一个映射保持私有值，干净的第二个映射看到
   page-cache 新值；
5. 写第二个映射的另一个字节不会回写文件或污染第一个映射。

同时注册并执行已有的：

- `private_file_madvise_dontneed_smoke`；
- `shared_file_alias_smoke`。

最终运行命令的等价形式为：

```sh
make -C os run_final \
  ARCH=loongarch64 SUBMIT=0 BASH_SHELL=1 LOG=warn \
  SMP=12 MEM=8G EXT4_REBUILD=0 USER_EXT4_SIZE=256M \
  FINAL_IMG=/Users/bytedance/projects/OS_Workspace/testsuits-final/sdcard-la-pub.img \
  QEMU_TIMEOUT=0 QEMU_EXTRA_ARGS=-snapshot
```

guest 中只执行：

```sh
./private_file_page_cache_smoke
./private_file_madvise_dontneed_smoke
./shared_file_alias_smoke
```

最终串口日志：

```text
testsuits-final/.tmp/final-runs/
  20260803-loongarch-page-cache-focused/final-smoke.log
```

结果：

```text
private_file_page_cache_smoke passed
private_file_madvise_dontneed_smoke passed
shared_file_alias_smoke passed
```

## 7. `tg-xtask` 聚焦性能验证

没有执行 `buildstorm_testcode.sh`，也没有执行后续
`cargo xtask arceos build -p arceos-helloworld`。本次只运行：

```sh
cd /work/tgoskits
export RUSTUP_TOOLCHAIN=nightly-2026-05-28
export CARGO_NET_OFFLINE=true
timeout 600 cargo build -p tg-xtask
```

临时启用现有 `DEBUG_PERF` 计数器后，起点与终点分别为：

```text
start block_read_bytes:  3,395,584
end   block_read_bytes: 73,080,832
delta:                 69,685,248 B
```

命令退出码为 124，不能算通过。该现场中超时后留下一个 rustc `-vV` 子进程；随后
在全新 snapshot 中用动态加载器日志验证，rustc 能完成 relocation、进入程序、打印
版本并执行所有 fini。再运行 120 秒 Cargo 聚焦窗口时，12 路 rustc 正常启动并生成
16 个 `target/debug/deps` 文件，但仍未完成全部 `tg-xtask`。

第二个窗口结束时，从启动起累计：

```text
block_read_bytes: 251,760,640
ext4_cache_hit_pct: 94
```

旧实现 180 秒读取 3.305 GB，新实现即使包含一次 rustc 冷启动与 12 路编译，块读取
仍保持在数百 MB 量级，支持“重复 DSO I/O 已被消除”的判断。但不同窗口的编译进度
不完全相同，因此本报告不把它包装成严格的端到端加速比。

## 8. 尚未解决与下一步

### 8.1 `tg-xtask` 仍未完成

10 分钟和 120 秒两个聚焦窗口都由 timeout 结束，没有成功退出码。完整 BuildStorm
仍未运行，主工作区编译成功分与耗时分都未知。

### 8.2 lazy fault 的逐页 TLB 成本

第二个 120 秒窗口累计出现约 31 万次 page batch。普通文件私有映射从 eager 改为
lazy 后，首次访问每页都会建立 PTE；当前 LoongArch 路径把这类“从 invalid 到
present”的更新也纳入 mm 级 shootdown。Linux 通常只对真正替换/降权的旧映射执行
昂贵失效，新建 PTE 主要保证 faulting CPU 的本地 walker/TLB 一致性。

下一批应单独证明 LoongArch paired negative-TLB 的最小失效范围，再优化，不能直接
删除跨 hart 同步而重新引入旧的 cached-invalid 故障。

### 8.3 page cache 尚无完整 LRU/writeback

当前机制满足本轮普通文件 mmap 和 BuildStorm 读取场景，但仍是过渡实现：

- 只有 OOM 前 clean-unused reclaim，没有冷热 LRU；
- MAP_SHARED dirty/writeback 仍沿用现有 mmap backing 机制；
- 没有 readahead window；
- 没有统一覆盖 tmpfs/memfd/SysV shm。

这些边界不影响本轮已验证的只读 Rust DSO 共享与 private COW，但需要后续继续向统一
VFS page cache 演进。

### 8.4 回归范围

- 没有运行完整 BuildStorm；
- 没有运行完整初赛/LTP；
- 没有运行 RISC-V QEMU runtime；
- 没有正式 BuildStorm judge 结果；
- 没有端到端 arceos 编译时间。

## 9. 静态验证

以下命令均通过，仓库原有 warning 保留：

```sh
TMPDIR=$PWD/.tmp/cargo cargo check --quiet \
  --manifest-path os/Cargo.toml \
  --target loongarch64-unknown-none-softfloat

TMPDIR=$PWD/.tmp/cargo cargo check --quiet \
  --manifest-path os/Cargo.toml \
  --target riscv64gc-unknown-none-elf

TMPDIR=$PWD/.tmp/cargo cargo check --quiet \
  --manifest-path user/Cargo.toml \
  --target loongarch64-unknown-none-softfloat \
  --bin private_file_page_cache_smoke \
  --bin private_file_madvise_dontneed_smoke \
  --bin shared_file_alias_smoke

TMPDIR=$PWD/.tmp/cargo cargo check --quiet \
  --manifest-path user/Cargo.toml \
  --target riscv64gc-unknown-none-elf \
  --bin private_file_page_cache_smoke \
  --bin private_file_madvise_dontneed_smoke \
  --bin shared_file_alias_smoke

git -C os diff --check
git diff --check
```

全仓 `cargo fmt --check` 仍会报告 vendored virtio/smoltcp 以及既有
`os/src/main.rs`、`perm_utils.rs` 的格式差异；本次没有机械改写这些无关文件。所有
本次触及的 Rust 文件已单独格式化。

## 10. 提交范围

本批次按职责拆分为多次提交：

| 仓库 | commit | 内容 |
| --- | --- | --- |
| `os/` | `54d0a199878bd81e32a9aa5bb4382ce888b2a1cd` | `mm: share clean private file pages` |
| 顶层 | `283321a1b200a705837f8ab529bc2fb1c681f147` | `test(mm): cover private file page-cache COW` |
| 顶层 | `c545b885632622f1e455025f6ee46fa4be2d5cd7` | `chore(os): update file page-cache kernel revision` |
| 顶层 | 本提交 | `docs(final): record file page-cache validation` |

串口日志、生成镜像、Cargo target 和性能临时文件均不进入提交。

## 11. AI 使用说明

AI 用于：审查本地 Linux `filemap_fault`/COW 参考实现、分析块 I/O 与进程现场、设计
inode page cache single-flight、实现和复审 mmap/COW/truncate/reclaim 改动、编写
聚焦回归、执行用户授权的短时验证并整理本报告。

所有结论来自本地源码、实际 QEMU 串口与性能计数。没有修改 judge、伪造输出、针对
测试名硬编码、篡改 `/proc/uptime`，也没有把超时的 `tg-xtask` 或未运行的完整
BuildStorm 写成通过。
