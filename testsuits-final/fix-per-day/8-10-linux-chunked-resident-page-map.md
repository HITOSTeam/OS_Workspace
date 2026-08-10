# 8-10 分块 resident page map 降低每-mm 页元数据

## 问题概述

ready-only file fault-around 已把多个 Ready 文件页合并到一次 fault，但每个新地址空间仍为每个
4 KiB resident page 在 `MapArea::data_frames` 插入一个独立
`BTreeMap<VirtPageNum, FrameTracker>` 项。BuildStorm 会反复启动 rustc/linker；物理文件页
通常已经在 inode page cache 中，新的 mm 却仍逐页承担 BTree 节点、重平衡和 slab 分配成本，
fork/COW 还会复制这棵 ownership shadow。

这不是继续调整 shared high-order buddy 能消除的成本。buddy 只负责首次物理页分配，而
`data_frames` 是每个 mm、每个已映射页都会产生的元数据。

## 如何发现

### 1. perf 精确定位 K/V 类型

带 `-perfmap` 的 BuildStorm profile 出现了以下专门化热点：

```text
BTreeMap<VirtPageNum, FrameTracker>
VacantEntry::insert
insert_recursing
split
```

全 `os/src` 只有 `MapArea::data_frames` 使用这个精确 K/V 类型。profile 同时显示 fault、
slab 和 runqueue 路径较热，而 shared high-order buddy 很低，说明主要成本是热文件页在新 mm
中重复创建 ownership 元数据。

归因资产：

```text
testsuits-final/.tmp/final-runs/20260809-riscv-shared-highorder-perfmap-full-226/
```

### 2. 等时 `/proc/perf` 基线

使用同一 LoongArch BuildStorm 根盘、12 hart、8 GiB，在计时段运行约 362 秒：

```text
file_fault_events       =   191,002
file_fault_ptes_mapped  =   652,583
mm_data_frame_inserts   = 2,294,687
heap_peak_actual_bytes  = 168,067,072
```

`mm_data_frame_inserts` 是 file-fault PTE 数的 3.52 倍。额外部分主要来自 fork/COW 和其他
匿名映射，证明 per-mm shadow 不只是 fault-around 的局部尾账，而是整个进程生命周期都会放大
的公共成本。

```text
testsuits-final/.tmp/final-runs/20260810-loongarch-rq-lock-timed-debugperf-578/
```

## Linux 对照

本地 Linux 参考树为 `exampleOs/linux` commit
`4549871118cf616eecdd2d939f78e3b9e1dddc48`。

`include/linux/xarray.h:1142-1179` 说明 XArray 使用按 shift/mask 索引的 chunk；普通 64 位
配置的 `XA_CHUNK_SHIFT=6`，即一个节点覆盖 64 个 slot。页缓存和大量页索引结构因此不会为
每个相邻 page index 单独建立一棵通用有序树节点。

CongCore 目前还没有 Linux 的全局 `struct page`/mapcount、XArray/RCU 和 folio 生命周期，
不能直接复制 Linux XArray。本批采用一个过渡性、由现有 `MemorySet` 锁保护的 64 页 sidecar，
先删除 profile 已确认的逐页 BTree 节点；长期仍应由固定 `PageDesc[]` 和 PTE/mapcount 取代
整个 per-mm ownership shadow。

## 怎么解决

### 1. 64 页稀疏 chunk

新增 `ResidentPageMap<T>`：

- 顶层 `BTreeMap` 的 key 是 `vpn >> 6`，一个节点覆盖 64 个 VPN；
- chunk 内用一个 `u64 present` bitmap 表示存在性；
- 值放在只包含 present 项的紧凑 `Vec<T>` 中，rank 由低位 bitmap 的 popcount 得到；
- 稀疏 chunk 不预留 64 个值，避免为只触碰一页的 VMA 固定浪费 512 字节；
- chunk 为空时立即从顶层树删除。

这保留了 `FrameTracker` 的现有所有权和 Drop 语义，没有改变 frame refcount、COW、dirty、
truncate 或 shootdown 顺序。

### 2. 保持全部 MapArea 操作语义

新 sidecar 支持：

- `get`、insert/replace、remove；
- VPN 升序 iterator 和 consuming iterator；
- 任意 VPN 的 `split_off`，整 chunk 直接移动，只拆一个边界 chunk；
- `move_by_delta` 时按升序消费并重建。

因此 `munmap`、`mprotect`、`mremap`、fork/COW、lazy fault 和 area 三分仍走同一公共实现，
没有加入 RISC-V/LoongArch `cfg` 策略分叉。

### 3. 增加因果计数器

`DEBUG_PERF=true` 时 `/proc/perf` 新增 `mm_resident_chunk_allocations`，可直接与
`mm_data_frame_inserts` 比较。正式源码和 production BuildStorm 均恢复
`DEBUG_PERF=false`。

## 对应提交

| 项目 | 值 |
| --- | --- |
| `os/` 基线 | `270b8afa9ad69e8864d4370a4fac6f9ab756e77d` |
| `os/` 修复 | `bf800980545c6d707f9708c95bf75ce893f73c46` |
| 提交标题 | `mm: chunk per-mm resident page metadata` |
| 顶层集成 | 本说明文档所在提交 |

## 对因提升

### 1. 等时 DEBUG_PERF 计数

两侧使用同一 LoongArch BuildStorm 根盘、12 hart、8 GiB，均在计时段约 362 秒主动停止；
I/O 工作量接近，且两侧 allocation failures/large fallback 都为 0：

| 指标 | BTree 基线 | 64 页 chunk | 变化 |
| --- | ---: | ---: | ---: |
| `file_fault_events` | 191,002 | 187,490 | -1.84% |
| `file_fault_ptes_mapped` | 652,583 | 627,199 | -3.89% |
| `mm_data_frame_inserts` | 2,294,687 | 2,215,906 | -3.43% |
| `mm_resident_chunk_allocations` | — | 60,972 | 36.34 inserts/chunk |
| `heap_peak_actual_bytes` | 168,067,072 | 143,040,512 | **-14.89%** |
| peak heap 减少 | — | 25,026,560 B | **-23.87 MiB** |
| block read bytes | 360,669,184 | 359,620,608 | -0.29% |
| block write bytes | 146,169,856 | 138,567,680 | -5.20% |

候选把“每页一个顶层节点”压成平均 36.34 次 insert 才建立一个 chunk，等价顶层节点分配数
比逐页 insert 少 97.25%。由于 chunk 允许稀疏，实际比例没有假设所有 chunk 都填满。

```text
testsuits-final/.tmp/final-runs/20260810-loongarch-rq-lock-timed-debugperf-578/
testsuits-final/.tmp/final-runs/20260810-loongarch-resident-chunk-timed-debugperf-592/
```

### 2. 双架构 B-C-C-B exec/file-page-cache A/B

每段独立启动，顺序为 baseline -> candidate -> candidate -> baseline，每次取 5 个外层样本：

| 架构 | baseline 中心中位数 | candidate 中心中位数 | 延迟变化 | failures |
| --- | ---: | ---: | ---: | ---: |
| RISC-V 8 hart | 210,349 us | 178,667.5 us | **-15.06%** | 0 |
| LoongArch 12 hart | 364,683 us | 368,236 us | +0.97% | 0 |

RISC-V 等价吞吐提升 17.73%；LoongArch 的 0.97% 波动低于 5% 双架构回退门槛，没有复现
此前 allocator cliff。四段原始运行：

```text
.tmp/fault-around/20260810-resident-chunk-riscv-bccb-{b1-584,c1-585,c2-586,b2-587}/
.tmp/fault-around/20260810-resident-chunk-loong-bccb-{b1-588,c1-589,c2-590,b2-591}/
```

### 3. 完整 BuildStorm 计时段

production 内核关闭 `DEBUG_PERF` 和 perfmap；每轮脚本都会删除对应
`target/$AXTGT`，再完整执行官方 `cargo xtask arceos build`：

```text
BUILDSTORM_TOOLCHAIN ok
BUILDSTORM_MINIBUILD ok
BUILDSTORM_COMPILE mode=multi ok=true elapsed_s=768.70 cores=12 bytes=1714568 arch=loongarch64

BUILDSTORM_TOOLCHAIN ok
BUILDSTORM_MINIBUILD ok
BUILDSTORM_COMPILE mode=multi ok=true elapsed_s=929.67 cores=8 bytes=1681000 arch=riscv64
```

LoongArch 父版本成功轮为 977.32 秒，本轮为 768.70 秒，墙钟缩短 21.35%、等价吞吐提高
27.14%。候选复用了成功轮结束后的根盘作为只读 backing，虽然目标架构目录会被删除，仍可能
保留其他 Cargo/文件缓存状态；因此该完整结果是稳定性和总体收益 gate，不把全部 21.35%
单独归因给本提交。严格因果收益以上述等时计数和 B-C-C-B 为准。

RISC-V 旧成功快照从首次观察到 `xtask=1` 至完成约 1,143 秒，本轮官方计时为 929.67 秒；
旧采集器没有保存精确官方 elapsed，因此这里只作为跨架构完整通过证明，不报告伪精确百分比。

```text
testsuits-final/.tmp/final-runs/20260810-loongarch-rq-lock-production-full-576/
testsuits-final/.tmp/final-runs/20260810-loongarch-resident-chunk-production-timed-594/
testsuits-final/.tmp/final-runs/20260809-riscv-fault-around-production-full-231/
testsuits-final/.tmp/final-runs/20260810-riscv-resident-chunk-production-timed-595/
```

一次 LoongArch `-perfmap` production 尝试在 76 秒内把 map 从约 220 MiB 增长到 670 MiB，
探针延迟从 6.5 秒升至 17.5 秒，因此按监控规则提前停止。该诊断资产保留在 run 593；正式
耗时关闭 perfmap，后续只在短窗口开启它。

## 回归验证

- `ResidentPageMap` host harness：2 passed / 0 failed，覆盖 63/64 边界、replace/remove、
  任意 VPN split 和有序迭代；
- 最终重建内核 RISC-V focused：7/7；
- 最终重建内核 LoongArch focused：9/9；
- RISC-V 额外 mprotect/mremap/stack gate：7/7；
- RISC-V、LoongArch64 softfloat `cargo check` 通过；
- 两架构 release build、`cargo fmt --check`、`git diff --check` 通过；
- `DEBUG_PERF=false`。

最终 focused 日志：

```text
.tmp/fault-around/20260810-riscv-resident-chunk-final-focus-596/
.tmp/fault-around/20260810-loongarch-resident-chunk-final-focus-597/
```

LoongArch 用户镜像中三个额外测试二进制仍是 RISC-V ELF，因此未把它们计为 LoongArch
失败；有效的 9 个 LoongArch 二进制全部通过。

## 当前边界与下一步

- 这是 Linux XArray 思路的过渡 sidecar，不是完整 XArray，也没有 RCU/marks/folio；
- 每个映射仍持有一个 `FrameTracker`，每个 frame 仍有 `Arc<FrameOwner>`；
- 长期应引入启动期固定 `PageDesc[]`、refcount/mapcount，并从 PTE/PFN 完成 unmap/COW，
  最终删除 per-mm ownership shadow；
- read/pread/mmap/exec 仍需收敛到统一 inode `address_space`，per-open 只保存 readahead 状态；
- 公共 MM 改动继续要求 RISC-V 与 LoongArch runtime gate，不接受只在一个架构完整运行。
