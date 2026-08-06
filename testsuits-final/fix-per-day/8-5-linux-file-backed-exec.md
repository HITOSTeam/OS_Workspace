# 8-5 Linux 式 file-backed exec 与并发 rustc 性能验证

## 问题概述

普通文件 mmap 已能共享 inode page cache，但 ELF `exec` 仍为每个 `PT_LOAD` 建私有
Framed area 并逐页复制。并发启动 rustc、rustup shim 和动态链接器时，相同的代码与
只读数据页被每个进程重复分配、读取和复制。

## 如何发现

12-worker 诊断超时期间 QEMU CPU 仍增长，RSS、host memory 和 swap 均未耗尽；把并发
降到 2 后基线稳定，说明成本集中在重复装载而非死锁。继续对照 mmap fault 与 exec
源码，发现两条加载路径语义不一致。Linux 参考为 `fs/binfmt_elf.c::elf_map()`、
`elf_load()`、`mm/filemap.c::filemap_fault()` 以及 private COW 路径。

原始 A/B 与回归日志：

```text
testsuits-final/.tmp/final-runs/20260805-exec-cache-baseline-2w-{1,2,3}/
testsuits-final/.tmp/final-runs/20260805-exec-cache-optimized-2w-{1,2,3}/
testsuits-final/.tmp/final-runs/20260805-exec-cache-regressions-2/serial.log
testsuits-final/.tmp/final-runs/20260805-exec-cache-riscv-regressions-2/serial.log
```

```sh
ARCH=loongarch64 SMP=12 MEM=8G IMAGE_MODE=snapshot \
  testsuits-final/run.sh shell
# guest：先 warmup，再由两个 child 并发执行 rustc -vV
/user/exec_file_page_cache_perf_smoke.bin
```

## 怎么解决

主 ELF 与 `PT_INTERP` 的完整文件页改为 inode-backed Lazy VMA，首次 fault 复用
`(dev, ino, page)` single-flight cache；可写私有页继续 COW。包含 BSS 的尾页单独
私有化并清零，完整 BSS 页保持匿名 lazy。更好的长期方案是继续收敛 exec/mmap 到同一
套 VMA、page-cache、writeback 与 reclaim 机制，消除剩余的重复缓存层。

映射 helper 将一个加载段拆成“可共享的页对齐文件区、必须私有清零的文件结尾页、
匿名 BSS 完整页”三段。这样没有把包含文件尾部旧字节的共享页直接暴露给 BSS。
Linux `elf_map()` 把可共享部分直接建立成文件虚拟内存区域，`padzero()` 单独处理文件
结尾与 BSS；本项目复用既有 Lazy VMA 和 inode page cache，没有复制 Linux 的 maple
tree、folio 和完整反向映射实现。

## 对应提交

- 状态：待提交，当前实现仍位于未提交工作树。
- 基线：顶层 `21332ba37bf1ba0efe8229e7f80eeffa3b99a239`；`os/`
  `b0185b3a4522c0ffc52599d73bd17b3d52320815`。
- 建议提交主题：`mm: share file-backed ELF load pages`。

## 对比提升

2 路并发 `rustc -vV` 的跨启动中位数由 `924652 us` 降至 `624980 us`
（-32.41%，吞吐 +47.95%）；QEMU 峰值 RSS 中位数由 `1,142,076 KiB` 降至
`865,628 KiB`（-24.21%）。18 个正式 batch 的 child 均返回 0。本条未证明完整
BuildStorm、完整 LTP 或正式 judge 通过。

---

## 1. 结论

本批次确认：普通文件 mmap 已经使用 inode page cache，但 ELF exec 仍把每个
`PT_LOAD` 建成私有 Framed area，再逐页从 inode 复制。并发启动相同的 rustc、rustup
shim 和动态链接器时，每个进程因此重复分配、读取和复制相同的代码/只读数据页。

本次参考 Linux `fs/binfmt_elf.c::elf_map()`、`elf_load()` 与既有
`filemap_fault()`/private COW 语义，完成以下通用修改：

- 主 ELF 与 `PT_INTERP` 的完整文件页改成 inode-backed Lazy VMA；
- 首次执行/读取 fault 复用现有 `(dev, ino, page)` single-flight 页缓存；
- 私有可写 LOAD 段首次写入继续通过已有 COW 路径变成匿名页；
- `p_filesz` 与 BSS 共用的最后一页单独私有化并清零，完整 BSS 页保持匿名 lazy；
- ELF backing 持有稳定只读 `OSInode`，fd 关闭后仍可 fault；
- RISC-V 可执行页沿用 fault 提交时的 I-cache 发布路径；
- 非 ELF、shebang 和旧的内存镜像兼容入口保持不变。

同一 LoongArch snapshot、12 vCPU、8 GiB 环境中，新增的 2 路并发
`rustc -vV` 微基准做了三次独立启动 A/B：

| 版本 | 三次启动的批内中位数/μs | 跨启动中位数/μs |
| --- | --- | ---: |
| eager exec PT_LOAD | 924652, 899350, 930994 | 924652 |
| file-backed exec PT_LOAD | 607463, 636883, 624980 | 624980 |

耗时下降 `299672 μs`，即 **32.41%**；等价吞吐提高 **47.95%**。全部 18 个正式
测量 batch（A/B 各 3 次启动 × 每次 3 轮 × 2 worker）中的子进程均返回 0。

同期 host 资源采样中，QEMU 峰值 RSS 的三次启动中位数从 `1,142,076 KiB` 降到
`865,628 KiB`，减少 `276,448 KiB`（24.21%）。这与“干净 executable 页由多个 mm
共享，而不是每个 exec 私有复制”一致。采样期间 QEMU host 线程数为 15–18，host
`MemAvailable` 始终高于 24 GiB；各次运行内 `SwapFree` 保持不变，没有资源耗尽或
无进度卡死迹象。

本批次没有运行完整 BuildStorm、完整 LTP 或正式 judge，因此结论只覆盖 file-backed
exec 语义、聚焦回归和上述可复现微基准。

## 2. 版本与测试资产

| 资产 | 值 |
| --- | --- |
| 顶层分支 / 基线 | `dev_final` / `21332ba37bf1ba0efe8229e7f80eeffa3b99a239` |
| `os/` 基线 | `b0185b3a4522c0ffc52599d73bd17b3d52320815` |
| final test source | `final-2026` / `b5ec6ef8497e1818cbdec3b54bb722f036e57972` |
| 本地 Linux 参考树 | `exampleOs/linux` / `4549871118cf616eecdd2d939f78e3b9e1dddc48` |
| QEMU | 11.0.3 |
| LoongArch 性能环境 | 12 vCPU，8 GiB，snapshot |
| RISC-V 回归环境 | 8 vCPU，8 GiB，snapshot |
| LoongArch 镜像 | `testsuits-final/sdcard-la-pub.img` |
| LoongArch 镜像 SHA-256 | `2ad9d955684297abe9db48d94f1b7fcc488268fc8f481408c55b1ec27f520c6a` |
| RISC-V 镜像 | `testsuits-final/sdcard-rv-pub.img` |

所有 QEMU 均使用 `-snapshot`，没有修改 final 基准镜像。性能 driver 设置 120 秒硬
上限；回归单项设置 90 秒上限。运行期间每 2 秒采样 QEMU `/proc/<pid>/status`、
`stat`、`io` 和 host `MemAvailable/SwapFree`。收尾后没有残留 QEMU，
`user/.cargo/config.toml` 已恢复为 RISC-V 默认 target。

## 3. 根因

### 3.1 mmap 与 exec 走了两套加载策略

此前 `os/src/mm/memory_set/fault.rs` 已经能让普通文件的干净私有页共享 inode cache：

```text
(dev, ino, page)
      -> Loading + WaitQueue
      -> Ready(FrameTracker)
```

但 `os/src/mm/memory_set.rs::map_elf_segments_from_reader()` 仍执行：

1. 为整个 `PT_LOAD.p_memsz` 建立 Framed area；
2. 每个 mm 立即分配全部物理页；
3. 用 4 KiB 临时缓冲逐页 `read_at()`；
4. 再 `try_copy_to_user_unchecked()` 到新 mm。

动态解释器还通过 `map_elf_segments_into()` 从完整 `Vec<u8>` 再复制一次。因此 prior
inode page cache 能消除 DSO mmap 的重复 I/O，却不能共享通过 exec 装入的主程序和
`ld.so` LOAD 页。

### 3.2 现场不是资源耗尽或普通死锁

最初 12 worker 微基准在 120 秒内连 warmup 都未完成，但 QEMU CPU 时间持续增长，
host 线程仍在调度，RSS/host memory/swap 没有耗尽。把 worker 降到 2 后，基线稳定
完成在约 0.9 秒/批。这说明旧路径主要是被并发重复页分配、文件读取、用户复制和后续
回收放大的 CPU/内存成本，不是一个静止的锁死现场。

12 worker 的超时只作为诊断证据，不纳入正式 A/B；正式测试始终使用 2 worker。

## 4. Linux 参考语义

本地 Linux 参考对应关系：

| Linux 机制 | 本地参考 | 本次实现 |
| --- | --- | --- |
| LOAD 文件页映射 | `fs/binfmt_elf.c::elf_map()` | page-aligned inode-backed private VMA |
| 文件页 + BSS | `fs/binfmt_elf.c::elf_load()` / `padzero()` | 私有末页 + 匿名完整 BSS 页 |
| 首次文件 fault | `mm/filemap.c::filemap_fault()` | inode page cache Loading/Ready |
| 干净私有页 | `mm/memory.c::do_read_fault()` | 多个 mm 映射同一只读 cache frame |
| 私有写入 | `mm/memory.c::do_cow_fault()` | 既有三阶段 COW |
| 动态解释器 | `load_elf_interp()` 调用同一 `elf_load()` | 主程序和 PT_INTERP 共用映射 helper |

关键边界是：`p_offset` 与 `p_vaddr` 必须 page-congruent；文件整页可以共享；包含 BSS
零区的末页不能直接保留共享文件内容；BSS 后续完整页不应产生文件 I/O。这里复制的是
Linux 的可观察语义和锁边界，不是移植 Linux 的 VMA/XArray 内部结构。

## 5. 实现

### 5.1 统一 file-backed PT_LOAD helper

`MemorySet::map_elf_segments_file_backed()` 先校验大小、溢出和 page congruence，再把
每个 LOAD 拆成最多三段：

```text
page-aligned file bytes    -> Lazy private file VMA
filesz/BSS shared tail     -> one private Framed zero-padded page
remaining BSS full pages   -> Lazy private anonymous VMA
```

文件 VMA 使用 `VmRegionKind::Elf`，保存 `file_dev/file_ino/file_offset`，并通过
`MmapBacking` 持有只读文件对象。`MmRef::new()` 会像普通 mmap 一样把 inode 身份注册
到弱反向索引，因此 write/truncate/cache invalidation 继续走同一套通用机制。

### 5.2 主程序与 PT_INTERP 使用相同机制

`execve_with_inode()` 为主 inode 和动态解释器 inode 分别建立只读 `OSInode` backing，
调用新的：

- `from_elf_info_file_backed()`；
- `from_elf_with_interp_info_file_backed()`。

解释器元数据仍经过独立 ELF header/program-header 解析和 ABI 校验。主程序与解释器
各自以 `(dev, ino, page)` 作为 cache 身份，不会把不同文件误合并。

`LoadedExecImage` 现在保留 `Arc<Inode>`，使解释器完整镜像、exec reservation 和新的
file backing 都指向同一已解析对象。exec 建 mm 时不持有 inode rwsem 跨 process lock
或 vfork 唤醒；真正 page-cache miss 的 I/O 发生在 mm lock 之外。

### 5.3 COW、I-cache 与生命周期

现有 fault 路径会把干净私有文件页安装为只读 `PTEFlags::COW`。LOAD 段带写权限时，
首次写 fault 复制成匿名页；只读/可执行访问继续共享 cache frame。RISC-V 新 executable
PTE 提交沿用 batch 的 I-cache stale/fence 发布，LoongArch 沿用本地 missing-PTE MMU
cache 更新。

file backing 的 `Arc<OSInode>` 与 mm 同寿命，所以关闭原 exec fd 不会使后续 fault
失去文件对象。主 executable 的既有 `ExecInodeReservation`/process identity 保持不变；
PT_INTERP 在构建地址空间期间也保留 reservation，避免解析/映射窗口内与 writable open
竞态。

### 5.4 诊断可见性

此前瓶颈调查补充了 ext4 cache 当前 entries/capacity，以及 block queue 的 submitted、
completed、in-flight、retry、interrupt 和 fallback-poll 计数到现有 perf dump。
`DEBUG_PERF` 默认仍为 `false`，正常路径不打印周期日志；这些字段用于后续区分 page
cache、块队列和调度瓶颈，不参与微基准判定。

## 6. 单一性能证明测试

新增 `user/src/bin/smoke_archive/exec_file_page_cache_perf_smoke.rs`，并在
`user/Cargo.toml` 显式注册。测试固定执行：

1. 2 个 child 并发 `execve("/root/.cargo/bin/rustc", ["rustc", "-vV"], env)`；
2. stdout/stderr 重定向到 `/dev/null`，避免串口吞吐进入计时；
3. 固定 `HOME`、`RUSTUP_HOME`、`CARGO_HOME`、`PATH` 和
   `RUSTUP_TOOLCHAIN=nightly-2026-05-28`；
4. 先做 1 个 warmup batch；
5. 用 guest `CLOCK_MONOTONIC` 测 3 个 batch，输出中位数；
6. wait 每个 child，任何非零状态都计入 `failures` 并使测试失败。

测试只依赖通用 fork/exec/wait 和真实 rustc，不读取内核私有计数、不识别测试版本，也
没有针对路径在内核中硬编码优化。

### 6.1 正式耗时结果

| 启动 | eager/μs | file-backed/μs | eager failures | file-backed failures |
| --- | ---: | ---: | ---: | ---: |
| 1 | 924652 | 607463 | 0 | 0 |
| 2 | 899350 | 636883 | 0 | 0 |
| 3 | 930994 | 624980 | 0 | 0 |
| 跨启动中位数 | **924652** | **624980** | **0** | **0** |

计算：

```text
latency reduction = (924652 - 624980) / 924652 = 32.41%
throughput gain   = 924652 / 624980 - 1          = 47.95%
```

正式日志：

```text
testsuits-final/.tmp/final-runs/20260805-exec-cache-baseline-2w-{1,2,3}/
testsuits-final/.tmp/final-runs/20260805-exec-cache-optimized-2w-{1,2,3}/
```

每个目录均保留 `serial.log` 与 `host-metrics.log`。

### 6.2 host 资源结果

2 秒采样的每次启动峰值 RSS：

| 启动 | eager/KiB | file-backed/KiB |
| --- | ---: | ---: |
| 1 | 1142076 | 866804 |
| 2 | 1155204 | 865628 |
| 3 | 1123660 | 865356 |
| 中位数 | **1142076** | **865628** |

RSS 中位数减少 `276448 KiB`，即 24.21%。基线每次有 3 个资源样本，优化后每次 2 个，
原因是相同 benchmark 更早结束；因此 CPU tick 只用于确认持续进度，不把不同长度的
采样窗口包装成精确 CPU 加速比。

## 7. 正确性与兼容回归

### 7.1 LoongArch

12 vCPU、8 GiB、snapshot，以下 6 项全部出现成功标记并返回 shell：

```text
file_mmap_lazy_fault_smoke passed
private_file_page_cache_smoke passed
private_file_madvise_dontneed_smoke passed
shared_file_cross_mm_smoke passed
shared_file_truncate_cache_smoke passed
EXIT_GROUP_PREPARED_WAIT_PASS
PASS
```

日志：

```text
testsuits-final/.tmp/final-runs/20260805-exec-cache-regressions-2/serial.log
```

### 7.2 RISC-V

8 vCPU、8 GiB、snapshot，以下 4 项全部通过，driver 退出码为 0：

```text
file_mmap_lazy_fault_smoke passed
private_file_page_cache_smoke passed
riscv_icache_smp_smoke passed: 128 updates x 7 remote harts
EXIT_GROUP_PREPARED_WAIT_PASS
PASS
```

日志：

```text
testsuits-final/.tmp/final-runs/20260805-exec-cache-riscv-regressions-2/serial.log
```

这些测试共同覆盖普通文件 lazy fault、MAP_PRIVATE COW、discard/refault、跨 mm cache
共享、truncate invalidation、exec de-thread 和 RISC-V 跨 hart I-cache 一致性。每个
用户测试自身也通过新的 file-backed exec loader 启动，因此同时覆盖静态 ELF 的
纯 BSS LOAD 段；并发 rustc 测试覆盖动态 ELF、PT_INTERP 和可写 LOAD/BSS 边界页。

## 8. 静态验证

以下命令通过，仓库原有 warning 保留：

```sh
TMPDIR=$PWD/.tmp ARCH=riscv64 cargo check \
  --manifest-path os/Cargo.toml \
  --target riscv64gc-unknown-none-elf

TMPDIR=$PWD/.tmp cargo check \
  --manifest-path user/Cargo.toml \
  --target riscv64gc-unknown-none-elf \
  --bin exec_file_page_cache_perf_smoke

rustfmt --edition 2024 --check \
  os/src/mm/memory_set.rs \
  os/src/syscall/process/exec.rs

git -C os diff --check
git diff --check
```

LoongArch release 内核与全部已注册用户程序由 `make -C os run_final ARCH=loongarch64`
实际构建并启动；RISC-V 同样由 `run_final ARCH=riscv64` 构建并启动。

## 9. 明确边界与下一步

- 动态解释器当前仍会为既有 LoongArch glibc symbol-table workaround 暂时保留完整
  `Vec<u8>`；LOAD 页已经 file-backed，但后续可把该 workaround 改成窄范围 positional
  read，进一步降低 exec 临时内存。
- inode page cache 仍只有 OOM 前 clean-unused reclaim，没有完整 LRU/readahead。
- 本批没有改变 ASLR 策略、LOAD 地址选择或 legacy in-memory loader。
- 没有运行完整 BuildStorm、完整 LTP、CAgent、unixbench、libcbench 或正式 judge。
- 12 worker 旧实现超过 120 秒只说明并发放大严重；因为它没有完成 warmup，不能用于
  计算 A/B 百分比。

下一步应使用同一 final 环境重新跑有硬上限的 `tg-xtask`/BuildStorm 聚焦窗口，确认
剩余耗时是在编译计算、wait/reparent、page-cache reclaim 还是块 I/O；不要从本次
rustc exec 微基准外推完整 BuildStorm 已经通过。

## 10. 建议提交范围

| 仓库 | 建议 subject | 内容 |
| --- | --- | --- |
| `os/` | `mm: back exec load pages with inode cache` | ELF file mapping、exec inode backing、perf diagnostics |
| 顶层 | `test(exec): benchmark shared file-backed loads` | 并发 rustc 微基准、Cargo 注册、ext4 diagnostics |
| 顶层 | `docs(final): record file-backed exec results` | 本报告与 `os/` submodule 指针 |

生成的 user image、串口日志、host metrics 和 expect driver 都位于 ignored
`testsuits-final/.tmp/`，不进入提交。
