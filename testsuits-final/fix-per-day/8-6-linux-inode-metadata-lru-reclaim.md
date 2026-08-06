# 8-6 Linux 式 inode 元数据 LRU 候选（验证后回滚）

## 问题概述

`DIR_INDEX_CACHE` 和 `EXTENTS_CACHE` 达到阈值后整表 `clear()`，会在全局 mutex 内同步
析构冷热对象，并在后续路径查找中集中重建，形成 cache cliff。

## 如何发现

BuildStorm `perf` 中 allocator deallocation 是最大稳定热点，目录索引
`BTreeMap<String, ...>::drop` 也可独立解析；同时 host 内存充足、串口仍有进度，排除
host OOM 与完全死锁。候选参考 Linux dentry/inode 的 unused-object LRU 与按内存压力
回收语义，再以同源 qcow2、相同内核外配置的 300 秒严格 A/B 验证。

```text
.tmp/final-runs/20260806-buildstorm-clean-xtask-85/run/
.tmp/final-runs/20260806-buildstorm-metadata-lru-87/run/
.tmp/final-runs/20260806-metadata-lru-buildstorm-ab-before-94b/run/
.tmp/final-runs/20260806-metadata-lru-buildstorm-ab-after-95/run/
```

```sh
perf record -F 99 -e cycles:u -g -p <qemu-pid> -o perf.data -- sleep 15
# guest
timeout 300 cargo build -p tg-xtask
```

## 怎么解决

候选实现用访问 stamp 选择最旧且仅由 cache 持有的对象，活动对象跳过，map 删除和昂贵
析构分离到锁外。语义测试通过，但 BuildStorm 主进度退化，因此实现与新增单测已完整
回滚。若重做，应先建立统一内存压力/shrinker，再让 inode/dentry 共享有界 LRU，而不是
为两个小 cache 单独增加泛型 bookkeeping。

候选代码只在 map 锁内选择 `(key, Arc)` 并删除 entry，随后释放锁再 drop `Arc`；
这避免把 `BTreeMap<String, ...>` 的递归析构放在全局 cache mutex 内。
Linux 把未使用 dentry/inode 接入全局内存回收器，由 shrinker 按压力和冷热链表分批
回收；本候选只有访问 stamp 和固定 soft capacity，无法共享全局压力信息，这也是语义
虽正确但 bookkeeping 成本未被端到端收益覆盖的重要差别。

## 对应提交

- 状态：无实现提交；候选经 A/B 否决并回滚，本文是负结果记录。
- 对照基线：顶层 `21332ba37bf1ba0efe8229e7f80eeffa3b99a239`；`os/`
  `b0185b3a4522c0ffc52599d73bd17b3d52320815`。

## 对比提升

候选把探针中位数从 `1315 ms` 降至 `1199 ms`（-8.82%），但 300 秒最终输出
`2984 -> 2890 bytes`（-3.15%），按 guest uptime 归一化仍退化 2.65%。主成功指标
没有提升，所以按性能证据回滚，不把“热点符号消失”误写成有效优化。

---

## 问题

BuildStorm 的 `cargo build -p tg-xtask` 会连续访问大量 crate 目录和文件。原来的
`DIR_INDEX_CACHE` 与 `EXTENTS_CACHE` 达到 64/256 项后直接 `clear()`：

- 一次阈值命中会同步析构整张目录索引或 extent 表；
- 刚访问过的热点与长期未用项一起丢失；
- 随后的 pathname lookup 又集中重建相同元数据，形成周期性 cache cliff；
- `clear()` 在全局 cache mutex 内执行，析构 `BTreeMap<String, ...>` 的时间也位于锁内。

旧内核的 BuildStorm `perf` 样本中，guest allocator deallocation 是最大的稳定热点，
目录索引 `BTreeMap<String, ...>::drop` 也能独立解析到；串口仍有缓慢进度，host
`MemAvailable` 约 21 GiB、QEMU RSS 约 3.7 GiB，因此证据指向同步回收抖动，而不是
宿主内存耗尽或完全死锁。

原始运行：

- `.tmp/final-runs/20260806-buildstorm-clean-xtask-85/run`
- 对应 `perf.data` 的总 period 为 `333204519362`，17,749 samples，lost=0；
- 最大 guest PC 为 allocator deallocation，约占总 period 25.1%。

## Linux 参考与候选实现边界

参考本地 Linux 源码中的 dentry/inode reclaim：Linux 把未使用对象放在可回收 LRU，
按压力选择旧对象；有活动引用的对象不能仅因容量阈值被同步释放。当前内核没有完整的
shrinker、RCU dcache 和全局内存回收器，因此本轮只实现此处需要的最小性质：

1. hit 更新单调访问 stamp；
2. 达到 soft capacity 时只选择一个最旧、且 `Arc::strong_count == 1` 的 cache-only
   对象；
3. 活动对象被跳过，允许暂时超过 soft bound，后续插入再重试；
4. 从 map 删除后先释放 cache mutex，再执行可能昂贵的析构；
5. 并发构建相同 inode 元数据时复用已经插入的对象；显式失效也在锁外 drop。

容量仍是目录索引 64、extent 表 256，没有为了单一测试任意放大缓存，也没有伪造
lookup 结果。目录 mutation、inode 删除或复用继续走原有失效路径。

## 语义验证

host 单测显式使用带 `std` 的 target：

```zsh
TMPDIR=$PWD/.tmp cargo test --manifest-path ext4-fs/Cargo.toml \
    --target x86_64-unknown-linux-gnu
```

结果为 15 passed，新增用例覆盖：

- hit 后只淘汰最旧 unused entry，不再清空全部对象；
- 被操作持有的 entry 不会被回收，释放引用后后续插入可以回收它。

LoongArch 12-hart 聚焦回归目录：

```text
.tmp/final-runs/20260806-metadata-lru-regressions-86/
```

`vfs_stat_smp_perf_smoke`、open/unlink lifetime、pathwalk errno 和 Unix pathname 四项
全部通过。

## BuildStorm 诊断与严格 A/B

只加入本轮 LRU 的恢复运行：

```text
.tmp/final-runs/20260806-buildstorm-metadata-lru-87/run/
```

在 guest uptime 470 秒达到 115 行 Cargo 输出、396 个 `target/debug/deps` 文件；
旧实现达到相同 deps 数量约需 566 秒。新 profile 中目录索引整表析构不再出现在主要
条目，但 allocator deallocation 仍占 29.1%，并出现 per-mm mmap backing tree 的
复制/析构成本，成为下一轮候选。

这不是严格性能结论：旧运行使用 raw 工作副本，新运行使用 qcow2 overlay，而且旧
监控探针会执行递归 `find`。新运行在 uptime 536 秒时探针自身超过 20 秒，driver 以
126 及时停止，`tg-xtask` 尚未生成；QEMU 没有留在后台。

随后改用不递归遍历 target tree 的浅探针，建立两个同源 qcow2 overlay，在相同
LoongArch 12-hart、8 GiB 和 300 秒 outer limit 下只替换 kernel ELF：

| kernel | guest uptime | 最终输出 bytes | bytes/guest-second | 探针中位数 |
| --- | ---: | ---: | ---: | ---: |
| 原始 threshold `clear()` | 307.20 s | **2984** | **9.714** | 1315 ms |
| inode metadata LRU | 305.63 s | 2890 | 9.456 | **1199 ms** |

原始结果：

- `.tmp/final-runs/20260806-metadata-lru-buildstorm-ab-before-94b/run/`
- `.tmp/final-runs/20260806-metadata-lru-buildstorm-ab-after-95/run/`

LRU 的浅探针中位延迟较低，但主要成功指标没有提升：相同截止的输出少 3.15%，按
guest uptime 归一化后仍低 2.65%。中间采样也一致：约 183 秒为 2020 对 2084 bytes，
约 244/246 秒为 2496 对 2571 bytes。两边每次探针都在 2.5 秒内完成、每轮都有前进，
没有 perf/stall cutoff；差异不能归因于某一边卡死。

## 决策

该候选语义正确，也消除了整表析构符号，但没有证明 BuildStorm 性能提升，且为两个
小 cache 引入约一层泛型 LRU bookkeeping。按照“性能候选必须经受控 A/B 证明、不要
过度抽象”的约束，代码与新增单测已经完整回滚；本文保留为负结果，避免后续重复尝试。
原有 threshold `clear()` 仍可能需要改善，但下一方案必须先针对 allocator/drop 热点
提出更小且可测的机制。完整 BuildStorm 和官方 judge 仍待验证。
