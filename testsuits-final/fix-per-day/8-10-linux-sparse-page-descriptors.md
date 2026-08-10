# 8-10 稀疏 PageDesc 消除逐帧 Arc 元数据

## 问题概述

CongCore 原来每次 `frame_alloc()` 都会额外执行一次：

```rust
Arc::new(FrameOwner {
    ppn,
    writable_uaccess_pins,
    user_buffer_access,
    icache_state,
})
```

这使每个物理页除 4 KiB 数据外，还在 512 MiB 内核堆中拥有一个独立 Arc 控制块、引用计数、
pin 计数和一到两把 ticket mutex。BuildStorm 的 file page cache、页表页、匿名页和 fork/COW
会同时保持数十万帧，因此这条逐帧小对象分配会造成明显的 slab 占用和分配/释放流量。

上一批已经把每 mm 的 `data_frames` 从逐页 BTree 改成 64 页 chunk；本批处理剩余的另一层
逐页元数据，即 `Arc<FrameOwner>`。目标不是为某个测试增加特例，而是建立与 Linux
`struct page` 相同的 PFN 索引公共元数据层。

## 如何发现

### 1. OOM 现场已经给出对象尺寸和数量

早期 BuildStorm OOM dump 中，order-6 档有 611,291 个 live allocation，平均 user size
47.96 B；它与当时的 `frame_refs=600,167` 一一对应。64 B actual allocation 合计约 39 MiB，
对象尺寸也与 `ArcInner<FrameOwner>` 相符。

### 2. 当前成功基线仍保持大量 FrameOwner

为避免只依赖旧 OOM，本批先在 `bf80098` 上临时导出 `frame_refcount_entries`，用同一
LoongArch 12 hart、8 GiB、相同 BuildStorm root backing 跑 360 秒等时基线：

```text
frame_refcount_entries sampled peak = 275,691
heap_peak_actual_bytes              = 144,338,944
heap_allocation_failures            = 0
```

LoongArch 的旧 owner 进入 64 B actual size class，仅 sampled peak 就约占：

```text
275,691 * 64 B = 17,644,224 B = 16.83 MiB
```

这说明 OOM 虽已解决，逐帧 Arc 仍是当前成功 BuildStorm 堆峰值中的可观固定税。

基线资产：

```text
testsuits-final/.tmp/final-runs/20260810-loongarch-frameowner-baseline-debugperf-598/
```

## Linux 对照

本地 Linux 参考树为 `exampleOs/linux` commit
`4549871118cf616eecdd2d939f78e3b9e1dddc48`。

- `include/linux/mm_types.h:79,184`：每个 PFN 的引用计数和 flags 位于持久的
  `struct page`，页分配不创建 Arc 或堆对象；
- `include/linux/page_ref.h:151-205`：`get_page`/`put_page` 最终是该固定描述符上的原子
  引用计数操作；
- `include/linux/bit_spinlock.h:28`：短小的页状态串行化可以使用嵌入 flags 的 bit lock，
  不需要为每页嵌入独立 mutex；
- `mm/sparse.c:308-448` 与 `mm/mm_init.c:1814`：稀疏内存模型按存在的物理内存建立
  vmemmap，而不是为最大可能 PFN 分配普通对象。

CongCore 尚无 Linux 完整的 NUMA section、hotplug 和 bootmem，因此本批采用较小的过渡实现：
固定 PFN 顶层索引，按首次使用的 2 MiB 物理范围惰性建立一页描述符。其生命周期和 PFN
索引语义与 vmemmap 一致，同时避免一次性占用完整 8 GiB RAM 对应的约 15 MiB描述符数组。

## 怎么解决

### 1. 8 字节 PageDesc

新增固定 8 B 的 `PageDesc`：

```text
AtomicU32 refcount
AtomicU32 state
```

`state` 的位布局统一承载：

- user-buffer access bit lock；
- RISC-V I-cache state bit lock；
- RISC-V `Untracked/Dirty/Clean` 状态；
- 28 位 writable uaccess pin count。

`FrameTracker` 现在只保存 `ppn + &'static PageDesc`。Clone 在描述符上增加 refcount；最后一次
Drop 使用 Release decrement + Acquire fence，确认没有 pin/lock 后清空状态并把 PFN 归还 frame
allocator。物理页引用、uaccess pin 和 RISC-V deferred I-cache clean 的外部 API 均未改变。

### 2. 稀疏 PFN 索引

- 启动时按 DTB 物理内存范围建立只包含 `AtomicPtr` 的顶层表；
- 一个 `PageDescChunk` 恰好 4 KiB，包含 512 个描述符，对应 2 MiB 物理内存；
- 首次触碰该范围时分配 chunk，用 CAS 发布；CAS 失败者立即释放未发布副本；
- 已发布 chunk 在内核生命周期内不释放，符合 PFN metadata 的持久语义；
- `/proc/perf` 增加 `frame_metadata_chunks` 和 `frame_metadata_bytes`，可直接核对元数据账。

### 3. 紧凑 bit lock

旧实现中每页的 `spin::Mutex<()>` 和 RISC-V `Mutex<FrameIcacheState>` 改为 `state` 内两个独立
bit lock。Acquire CAS 获取、Release clear 释放；pin 和 I-cache 状态更新用 CAS 保留其他位。
锁顺序仍是 user access 在前、I-cache state 在后，没有改变原有并发协议。

引用/pin 的增量操作在修改原子值之前检查 0、上溢或下溢，避免错误路径先污染状态再 panic。

## 对应提交

| 项目 | 值 |
| --- | --- |
| `os/` 基线 | `bf800980545c6d707f9708c95bf75ce893f73c46` |
| `os/` 修复 | `2e0c1d1ea72a46187b2ec5a29408366322712e9c` |
| 提交标题 | `mm: move frame references into sparse page descriptors` |
| 顶层集成 | 本说明文档所在提交 |

## 对因提升

### 1. 等时 DEBUG_PERF 内存 A/B

两边使用相同 LoongArch root/user backing、12 hart、8 GiB、关闭 perfmap，从 `xtask` 已存在的
同一快照开始，在计时段约 362 秒主动停止。两边完成的工作量接近，候选还略多完成约 1% 的
per-mm insert：

| 指标 | `Arc<FrameOwner>` 基线 | 稀疏 PageDesc | 变化 |
| --- | ---: | ---: | ---: |
| `mm_data_frame_inserts` | 2,188,961 | 2,209,974 | +0.96% |
| `file_fault_events` | 186,622 | 187,150 | +0.28% |
| `file_fault_ptes_mapped` | 624,799 | 627,066 | +0.36% |
| block read bytes | 359,645,184 | 359,604,224 | -0.01% |
| block write bytes | 136,851,456 | 137,089,024 | +0.17% |
| sampled live frame peak | 275,691 | 274,147 | -0.56% |
| `frame_metadata_chunks` | — | 602 | — |
| `frame_metadata_bytes` | Arc 内含于 heap | 2,497,536 | **2.38 MiB** |
| `heap_peak_actual_bytes` | 144,338,944 | 128,626,688 | **-10.89%** |
| peak heap 减少 | — | 15,712,256 B | **-14.98 MiB** |
| `heap_allocation_failures` | 0 | 0 | 不变 |

候选做了相同或略多的 fault/PTE/I/O 工作，堆峰值仍下降 14.98 MiB，因此不是“少做工作”的
假收益。测得的减少量也与旧 owner 16.83 MiB sampled peak 减去 2.38 MiB 新 metadata 的静态
估算相符。

```text
testsuits-final/.tmp/final-runs/20260810-loongarch-frameowner-baseline-debugperf-598/
testsuits-final/.tmp/final-runs/20260810-loongarch-pagedesc-debugperf-602/
```

### 2. 双架构 B-C-C-B exec/file-page-cache A/B

每段独立启动，顺序固定为 baseline -> candidate -> candidate -> baseline，每次取 5 个外层
median，共 10 个 baseline 与 10 个 candidate 样本：

| 架构 | baseline 中位数 | candidate 中位数 | 延迟变化 | 等价吞吐变化 | failures |
| --- | ---: | ---: | ---: | ---: | ---: |
| RISC-V 8 hart | 183,774.5 us | 171,836.0 us | **-6.50%** | **+6.95%** | 0 |
| LoongArch 12 hart | 366,009.0 us | 363,641.5 us | **-0.65%** | **+0.65%** | 0 |

两架构均未触发 5% 回退门禁，且顺序反转后结果仍成立。

```text
.tmp/fault-around/20260810-riscv-pagedesc-bccb-{b1-603,c1-604,c2-605,b2-606}/
.tmp/fault-around/20260810-loongarch-pagedesc-bccb-{b1-607,c1-608,c2-609,b2-610}/
```

### 3. 完整官方 BuildStorm

production 内核关闭 `DEBUG_PERF` 和 perfmap；candidate 与基线使用各架构完全相同的
root/user backing SHA，脚本完整执行官方 timed `cargo xtask arceos build`：

| 架构 | 基线 | candidate | 变化 | 结果 |
| --- | ---: | ---: | ---: | --- |
| LoongArch 12 hart | 768.70 s | 760.76 s | **-1.03%** | rc=0，200/200 |
| RISC-V 8 hart | 929.67 s | 937.94 s | +0.89% | rc=0，200/200 |

RISC-V 完整轮的 0.89% 回退与 LoongArch 的 1.03% 提升都小于长测噪声门槛；因此不把完整轮
宣称为确定的吞吐提升。保留改动的主要因果证据是等时堆峰值下降 10.89% 和 B-C-C-B 中两架构
均无回退，其中 RISC-V 微基准稳定改善 6.50%。两架构完整功能和评分均通过。

```text
testsuits-final/.tmp/final-runs/20260810-loongarch-resident-chunk-production-timed-594/
testsuits-final/.tmp/final-runs/20260810-loongarch-pagedesc-production-full-613/
testsuits-final/.tmp/final-runs/20260810-riscv-resident-chunk-production-timed-595/
testsuits-final/.tmp/final-runs/20260810-riscv-pagedesc-production-full-614/
```

## 回归验证

- RISC-V production focused：7/7，包括 mmap page cache、truncate、跨 hart I-cache publish、
  exec；
- LoongArch production focused：9/9，包括 ASID wrap、lazy new-PTE、本地/跨核 TLB shootdown、
  4 MiB mprotect、mmap/exec；
- RISC-V 与 LoongArch softfloat `cargo check` 通过；
- 两架构 release build、`cargo fmt --check`、`git diff --check` 通过；
- 两架构完整 BuildStorm 均为 `rc=0`、scripted 180/180、总分 200/200；
- 提交后重新链接的 RISC-V/LoongArch ELF SHA 分别与 run 614/run 613 归档内核逐字节一致；
- 最终源码 `DEBUG_PERF=false`。

最终 focused 资产：

```text
.tmp/fault-around/20260810-riscv-pagedesc-production-focus-611/
.tmp/fault-around/20260810-loongarch-pagedesc-production-focus-612/
```

## 当前边界与下一步

- PageDesc chunk 目前由内核 heap 分配并永久保留；长期应迁到 boot-time vmemmap/专用 metadata
  区，并补 NUMA、内存热插拔和 section online/offline；
- 当前只有 refcount、pin、短锁和 RISC-V I-cache 状态；后续可承载 mapcount、dirty、LRU、
  writeback 等真正的统一 page state；
- 全局 frame allocator 仍是一把 irq-save 锁，下一步应做共享 zone 前面的可 refill/drain
  per-hart PCP，而不是永久分片；
- 每 mm 的 resident sidecar 虽已 chunk 化，仍保存 `FrameTracker` ownership shadow；最终应由
  PTE/PFN + PageDesc mapcount 完成 unmap/COW，删除这层 shadow；
- read/pread/mmap/exec 仍需统一到 inode `address_space`，per-open 只保存 readahead 状态；
- 公共 MM 改动继续要求 RISC-V 与 LoongArch runtime gate，不接受单架构完整运行。
