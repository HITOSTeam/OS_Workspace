# 8-9 共享 costly-order heap arena 隔离大块分配

## 问题概述

内核 512 MiB heap 原先被永久切成 `MAX_HARTS` 个独立 slab/buddy shard。小对象走
per-hart 快路能减少锁竞争，但它与 Linux 的 per-CPU page cache 有一个关键差异：Linux
的 PCP 是共享 zone buddy 前面的缓存，而 CongCore 的 shard 是不可跨越的物理分区。

BuildStorm 会同时制造大量 Arc、BTree 节点、小 Vec 和 64--128 KiB 连续缓冲。小对象能
把每个 shard 打碎；此时所有 shard 的空闲量相加可能很大，却没有任何一个 shard 能满足
高阶请求。历史 OOM 现场正是还有 36.9 MiB 空闲、但 order >= 17（128 KiB）没有可用块。

本批不是把 shared buddy 当作 BuildStorm 主吞吐优化，而是修复这种分区放大的外部碎片，
让后续 file fault、page cache 和链接阶段优化不会再次被偶发高阶 OOM 掩盖。

## 如何发现

`20260806-buildstorm-buddy-order-stats-full-112` 的 OOM dump 给出了完整 allocator 账：

- heap actual 约 500 MiB，剩余约 36.9 MiB；
- 最大空闲块只有 64 KiB；
- 失败请求为 128 KiB；
- 当时每个 hart shard 只有约 42.7 MiB，高阶块必须在单一 shard 内成立。

后续 perf profile 里 shared high-order buddy 本身占比很低，这反而说明不应继续把它当作
吞吐热点微调。这里采用 Linux 的分层原则，只解决高阶分配的可用性。

原始资产：

```text
testsuits-final/.tmp/final-runs/20260806-buildstorm-buddy-order-stats-full-112/
testsuits-final/.tmp/final-runs/20260809-riscv-shared-highorder-smoke-223/
testsuits-final/.tmp/final-runs/20260809-riscv-shared-highorder-gate-224/
testsuits-final/.tmp/final-runs/20260809-riscv-shared-highorder-perfmap-full-226/
```

## Linux 对照

本地 Linux 参考树为 `exampleOs/linux` commit
`4549871118cf616eecdd2d939f78e3b9e1dddc48`。

- `include/linux/mmzone.h` 定义 `PAGE_ALLOC_COSTLY_ORDER = 3`；
- `mm/page_alloc.c::pcp_allowed_order()` 只允许 order 0--3 使用 PCP；
- `rmqueue()` 对更高 order 直接进入共享 zone 的 `rmqueue_buddy()`；
- PCP 不够时也会回到共享 buddy，而不是把 per-CPU 容量做成硬分区。

CongCore 使用 4 KiB 页，因此第一个 costly order 是 order 4，即 64 KiB。实现复制的是
“小阶本地快路、大阶共享 zone、失败时可跨层回退”这一结构，不移植 Linux 的完整 zone、
migratetype 或 compaction。

## 怎么解决

### 1. 从 heap 尾部保留共享高阶 arena

512 MiB heap 中保留 96 MiB 给一把共享 `BuddyHeap`，其余 416 MiB 继续分给 per-hart
`SlabHeap`。布局在启动时一次确定，范围互不重叠。

- `max(size, align) >= 64 KiB`：优先 shared buddy；
- 更小对象：优先本 hart shard，并保留原有跨 shard 查找；
- shared 满时大对象可退回 shard；所有本地 shard 满时小对象可紧急退回 shared；
- free 按地址范围路由到原 arena，因此任务迁移不影响释放正确性。

这保留了小对象无全局锁的常见路径，同时让高阶块不再被每个 shard 的 slab 页共同切碎。

### 2. 扩大 packed buddy link

共享 arena 比原单 shard 大。free-list 的 one-based 8-byte slot index 从 24 bit 扩为
25 bit，可覆盖 256 MiB；其余 order 元数据仍装在一个 64-bit `FreeNode` 中。

### 3. 增加对因计数

`DEBUG_PERF=true` 时新增：

```text
heap_shared_actual_bytes
heap_shared_peak_actual_bytes
heap_shared_allocations
heap_shared_small_fallbacks
heap_large_shard_fallbacks
```

正式内核保持 `DEBUG_PERF=false`。

## 对应提交

| 项目 | 值 |
| --- | --- |
| `os/` 基线 | `71da5616ef74cec66a04a8f08ba241c144317458` |
| `os/` 修复 | `89458995a7a504598b960885548b95b8b2bcef1c` |
| 提交标题 | `mm: isolate costly heap allocations` |
| 顶层集成 | 本说明文档所在提交 |

## 对因提升

这是可靠性隔离，不声明独立吞吐提升。诊断结果证明路由按设计工作：

| 场景 | shared allocations | shared peak | small fallback | large fallback | alloc failure |
| --- | ---: | ---: | ---: | ---: | ---: |
| exec smoke 结束 | 200 | 25,755,648 B | 0 | 0 | 0 |
| BuildStorm gate，uptime 920 s | 97,545 | 71,827,456 B | 0 | 0 | 0 |

BuildStorm 中近十万次 costly allocation 被共享 arena 正常承接，peak 仍低于 96 MiB
预算，且没有把小对象挤进 shared，也没有让大对象退回碎片化 shard。随后包含本修复的
production 内核完成完整 RISC-V BuildStorm，`heap allocation error`、OOM 和 panic 均未
出现。

代码还增加了一个 allocator 模型测试：故意把四个 local heap 交错碎片化，使其聚合空闲
量足够但都无法分配 256 KiB；独立 shared heap 仍能成功分配。当前 `os` crate 的 host
test harness 有既有架构编译错误，因此不把这个未执行的单测写成“已通过”。运行态证据
来自上述 RISC-V smoke、gate 和完整 BuildStorm。

## 回归验证

- RISC-V `cargo check`：通过；
- LoongArch64 softfloat `cargo check`：通过；
- RISC-V release build：通过；
- `cargo fmt --all -- --check`、`git diff --check`：通过；
- 完整 RISC-V BuildStorm：通过，无 OOM/panic。

## 当前边界

- 96 MiB 是当前 512 MiB heap 的静态预算，不是 Linux 水位驱动的动态 zone；
- 没有实现 compaction、migratetype 或后台 reclaim；
- per-frame `Arc<FrameOwner>` 和 per-mm `data_frames` 仍制造大量小对象，它们应由后续
  `PageDesc[]`/PTE-derived ownership 重构消除；
- 本修复只保证 costly allocation 不再受所有 local shard 的共同碎片影响，不把它包装成
  BuildStorm 的主要性能收益。
