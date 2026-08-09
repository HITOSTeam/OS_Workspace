# 8-9 共享 heap zone 与可回填 per-hart slab cache

## 问题概述

固定 96 MiB costly arena 虽然解决了 RISC-V 高阶分配被小对象碎片化的问题，但它仍把
512 MiB kernel heap 永久切成两级容量：96 MiB shared arena 与按 `MAX_HARTS` 分割的
local shard。LoongArch 12-hart BuildStorm 在并发波峰达到 arena 临界点：

```text
heap_shared_peak_actual_bytes = 100,597,760 / 100,663,296 B
heap_large_shard_fallbacks    = 462
```

随后 128 KiB 分配要先争 shared lock，失败后在关本地中断状态下扫描 12 个永久 shard。
allocator、runqueue 和 wakeup 临界区互相放大，形成高 CPU、零 I/O、构建进度近乎停止的
吞吐悬崖。没有 OOM/panic 不代表 allocator 拓扑正确。

## 如何发现

### 1. 固定 arena 与 cliff 时间闭合

run 235 在到达 `strum` 附近之前速度正常；425 秒时 shared peak 已接近 96 MiB 上限并
出现 462 次 large fallback，此后计数和构建输出几乎冻结：

```text
testsuits-final/.tmp/final-runs/20260809-loongarch-cliff-ab-fault1-235/serial.log
```

shared actual 长期落在 128 KiB 的整数倍，结合 `OSInode` 首次读无条件扩展 128 KiB
read buffer，确认大对象是容量压力的主要来源。direct mmap fill 能改善前半程但仍出现
尾部平台期，证明 per-open buffer 是放大器，固定 arena/永久分区才是需要先移除的结构。

### 2. 提交级 A/B 排除 fault-around

旧 parent（无固定 shared arena）能越过同一并发波峰；包含固定 arena 的提交则复现
cliff。fault-around 的 1 页/16 页消融没有给出同方向结果，因此没有用架构专属窗口
常数掩盖 allocator 问题。

## Linux 对照

本地 Linux 参考树为 `exampleOs/linux` commit
`4549871118cf616eecdd2d939f78e3b9e1dddc48`。

Linux per-CPU pageset/slab magazine 是共享 zone 前面的缓存，不拥有永久物理范围：

- 低水位从共享 buddy 批量 refill；
- 高水位把空闲页 drain 回共享 buddy；
- costly allocation 直接进入共享 zone；
- CPU 数量改变的是缓存数量，不会把每个 CPU 可用的连续内存硬性缩小。

本实现采用同一结构边界，不移植完整 NUMA zone、migratetype、compaction 或 SLUB。

## 怎么解决

### 1. 一个 512 MiB shared buddy zone

整个 `HEAP_SPACE` 由一把 `BuddyHeap` 管理，取消固定 96 MiB arena、永久 local shard 和
失败后的全 shard 扫描。大对象直接从完整 zone 获取连续块。为覆盖 512 MiB zone，packed
buddy 的 one-based 8-byte link 从 25 bit 扩为 27 bit。

### 2. per-hart cache 只借用 slab page

小对象仍按 hart 分锁，但 cache 不拥有地址范围：

```text
local class miss -> shared zone 分配一个 4 KiB slab page
object alloc/free -> 只持 owner hart cache lock
第二个同 class 空 slab -> 从 cache 摘除并归还 shared zone
```

每个 hart/class 最多保留一个完全空的 slab，既保留热路径局部性，又使闲置容量可以回到
所有 hart 和 costly allocation 共享的后备区。对象随任务迁移释放时，根据 page metadata
回到稳定的原 owner cache。

### 3. 固定页元数据与锁序

启动期为 heap 中每个 4 KiB 页准备 16-byte `SlabPageMeta`，记录 class、owner、free list、
in-use 数和 partial-list 链。活跃 metadata 始终由 owner cache lock 保护；锁序唯一为：

```text
owner cache lock -> shared zone lock
```

不存在 `zone -> cache` 的反向路径。直接大对象只持 zone lock，统计路径也不会同时反向
持有两类锁。

## 对应提交

| 项目 | 值 |
| --- | --- |
| 旧固定 arena | `89458995a7a504598b960885548b95b8b2bcef1c` |
| 当前基线前序 | `9cbde1a48ae50693be0775271beac5722c04673e` |
| 修复 | `014eb34c7fcf9dbea96a63de4301b3432bd99cef` |
| 提交标题 | `mm: make heap caches refillable` |

## 对因提升

production B-C-C-B 使用相同用户镜像、独立启动、perfmap 关闭、`DEBUG_PERF=false`：

| 架构 | 固定 arena 基线 | refillable zone | 延迟变化 | 等价吞吐 |
| --- | ---: | ---: | ---: | ---: |
| RISC-V 8 hart | 192,693 us | 196,349 us | +1.90% | -1.86% |
| LoongArch 12 hart | 397,702.5 us | 378,383 us | **-4.86%** | **+5.11%** |

RISC-V 变化低于 5% 回退门禁；LoongArch 消除了原本的容量 cliff，并在官方 12-hart
配置下有净收益。四轮原始日志：

```text
.tmp/fault-around/20260809-pagezone-riscv-bccb-{baseline-296,candidate-297,candidate-298,baseline-299}/
.tmp/fault-around/20260809-pagezone-loong-bccb-{baseline-300,candidate-301,candidate-302,baseline-303}/
```

LoongArch BuildStorm cliff gate 使用 12 hart、8 GiB、raw snapshot、perfmap 关闭，约 490 秒
到达 `strum`，之后继续观察 300 秒仍持续启动后续 crate，共记录 62 个 progress event；
没有重现固定 arena 在并发波峰后的冻结：

```text
testsuits-final/.tmp/final-runs/20260809-loongarch-pagezone-cliff-291/
```

## 回归验证

- RISC-V MM/page-cache 运行态 7/7；
- LoongArch MM/TLB/page-cache 运行态 9/9；
- RISC-V、LoongArch `cargo check` 通过；
- `cargo fmt -- --check`、whitespace check 通过；
- 两架构所有 focused run 均为 `heap_allocation_failures=0`，无 OOM/panic。

日志：

```text
.tmp/fault-around/20260809-pagezone-riscv-focus-289/serial.log
.tmp/fault-around/20260809-pagezone-loong-focus-290/serial.log
```

## 当前边界与下一步

- shared buddy lock 仍是 refill、drain 与大对象的共同后备锁，但热小对象不进入该锁；
- 每 open description 的 128 KiB read/write buffer 仍会制造不必要的大对象，应由统一
  inode `address_space` 与独立 readahead state 替代；
- 每 frame `Arc<FrameOwner>`、每-mm `data_frames` BTree 和无水位 page-cache reclaim
  仍是下一阶段 MM 主体开销；
- 不应重新扩大固定 arena 或恢复永久 shard。后续容量优化应在 shared zone 前增加可
  refill/drain 的 batch cache，而不是重新切割物理地址范围。
