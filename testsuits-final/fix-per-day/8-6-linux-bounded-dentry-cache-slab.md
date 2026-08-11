# 8-6 有界 dentry cache + 小对象 slab 堵住 BuildStorm OOM

## 问题概述

全新官方镜像上的 BuildStorm 在 guest 约 1320 秒出现 CPU (活动核下降 )，约 1636 秒
输出冻结，1876 秒因 128-KiB 分配失败 panic。

根因：VFS dentry cache 没有容量限制、没有 LRU、没有清理死引用。具体来说：

- 缓存用 `BTreeMap<DentryKey, Weak<Dentry>>` 保存，最后一个强引用消失后
  Weak（弱引用，不阻止释放但自身仍占内存）继续钉住 72 字节的 Arc 分配；
- key 用的是每次重建都会变的临时 id，相同路径的下一次 lookup 不能覆盖旧 key，
  形成无限增长"；
- OOM 时 order-7 有 1,490,845 个 live block、平均 requested size 72.48 字节
  ——正好是 `ArcInner<Dentry>` 的大小，dentry 约占 500 MiB live heap 的
  70%–75%。

## 背景知识

**dentry（目录项缓存条目）是什么**。课上讲文件系统时说"打开文件要逐级查目录"
——先在根目录找 `home`，再在 `home` 下找 `alice`，以此类推。每一级查找都要
读磁盘上的目录数据块。操作系统把查过的"名字→inode 编号"结果缓存在内存里，
这就是 dentry cache（dcache）。下次走同一路径直接查内存，不用碰磁盘：
简而言之 就是目录的cache--- 路径 到 inode

```text
路径: /home/alice/code/main.c

没有 dcache 时（每次都读磁盘）：
  read dir "/"     → 找到 home,  inode 2
  read dir "/home" → 找到 alice, inode 105
  read dir "/home/alice" → 找到 code, inode 308
  read dir "/home/alice/code" → 找到 main.c, inode 712

有 dcache 时（直接查内存哈希表）：
  dcache["/" + "home"]         → inode 2    ✓ 命中
  dcache["/home" + "alice"]    → inode 105  ✓ 命中
  dcache["/home/alice" + "code"] → inode 308 ✓ 命中
  dcache["/home/alice/code" + "main.c"] → inode 712 ✓ 命中
  零次磁盘读取
```

**路径查找为什么开销大**。一条 5 级路径就是 5 次目录搜索。每次要取得目录
inode、读目录数据块、在块内查找目标名字。BuildStorm 编译几千个源文件，每个
文件的 open/stat/exec 都走这条路——百万次量级。没有 dcache 就要百万次磁盘
读取或 block-cache 查找。

**缓存不限容量会怎样**。每个查过的路径永远留在内存里，编译器产生的几万个
临时文件各自留一条 dentry，用完后再也不访问——变成"死缓存"。旧实现更糟：
用 Weak 指针，对象本体已释放但 Weak 本身（72 字节 Arc 头）仍占内存。这些
"墓碑"无限累积直到内存耗尽（正是之前 OOM 的根因）。

**CLOCK（二次机会）回收算法**。教科书讲页面置换通常先讲 LRU（最久未使用）：
维护完整的访问时间排序，最老的先淘汰。精确 LRU 开销大——每次访问都要更新
排序。CLOCK 是 LRU 的廉价近似：

```text
CLOCK 算法示意（环形队列 + 1-bit referenced 标记）：

     手指（clock hand）
       ↓
  ┌─ A(ref=1) ── B(ref=0) ── C(ref=1) ─┐
  │                                       │
  └─ F(ref=1) ── E(ref=0) ── D(ref=1) ─┘

需要驱逐一项时，手指顺时针扫描：
  1. 看到 ref=0 → 驱逐该项，结束
  2. 看到 ref=1 → 把 ref 清零（"给你第二次机会"），继续走

每次缓存命中时把 ref 设为 1。
效果：最近被用过的项至少能撑过一轮扫描，两轮都没被用过的才被驱逐。
比精确 LRU 便宜（只需 1 bit），效果接近。
```

对比课上的 LRU：LRU 要维护按时间排序的链表，每次访问都要移动节点。CLOCK
只需翻转一个 bit，代价更小，效果接近。

slab 是 小分配器
**slab 分配器为什么比通用堆快**。buddy allocator 按 2 的幂分配大块内存，
最小块 128 字节。72-byte dentry 用 buddy 浪费 44%。slab 从 buddy 申请一整个
4 KiB 页，切成固定大小的格子：

```text
Slab page（4 KiB，96-byte class，共 42 格）：
┌────┬────┬────┬────┬────┬─...─┬────┐
│ 96B│ 96B│ 96B│ 96B│ 96B│     │ 96B│
└────┴────┴────┴────┴────┴─...─┴────┘
  分配 = 从空闲链表摘一格，O(1)
  释放 = 挂回链表，O(1)
  整页空 = 还给 buddy
```

72-byte 对象放 96-byte 格子只浪费 24 字节。不需要搜索、合并、拆分。每个 CPU
还可以有自己的空闲链表，大多数分配不用抢全局锁。

**缓存失效为什么需要稳定 key 而不是靠临时 id**。旧实现用 parent dentry 的
内存地址作为缓存 key。同一目录的 dentry 对象被释放后重建地址就变了，产生新
key，旧 key 永远不会被覆盖。正确做法是用磁盘上不变的身份——
`(filesystem id, parent inode 号, 文件名)` 作为 key。同一路径不管重建多少次
都映射到同一个缓存槽位，旧的被新的覆盖，不会无限增殖。

## 如何发现

BuildStorm run `20260806-buildstorm-buddy-order-stats-full-112` 的 OOM order
统计直接定位到 dentry 未回收。host 当时仍有约 20 GiB swap free，前台探针也能
在数百毫秒内返回，排除了 host OOM 和全局死锁。

```text
testsuits-final/.tmp/final-runs/20260806-buildstorm-buddy-order-stats-full-112/run/serial.log
testsuits-final/.tmp/final-runs/20260806-buildstorm-buddy-order-stats-full-112/run/host-metrics.log
testsuits-final/.tmp/final-runs/20260806-buildstorm-buddy-order-stats-full-112/run/probe-latency.csv
```

修复后的 perf 热点主要是 QEMU TCG 翻译相关函数，不是 guest 锁等待：

```text
testsuits-final/.tmp/final-runs/20260806-buildstorm-bounded-dcache-perf-full-116/run/perf-stall-check.data
testsuits-final/.tmp/final-runs/20260806-buildstorm-bounded-dcache-perf-full-116/run/perf-slow-probe.data
```

Linux 对照（`exampleOs/linux` commit `4549871118cf`）：
- `retain_dentry()`：最后一个外部引用消失后保留完整 dentry 而非 Weak 墓碑；
- `d_lru_add()` / `prune_dcache_sb()` / `shrink_dentry_list()`：unused dentry
  进 LRU，内存压力下从 hash 和 LRU 一起摘除；
- `__d_lookup_rcu()`：热路径用 RCU（读侧不加锁的并发保护）lookup；
- slab 小对象用有限 size class，空 slab 整页还给页分配器。

## 怎么解决

**有界强引用 positive dcache**：改为保存强 `Arc<Dentry>`，真正保留可复用
对象。加 per-filesystem 32,768 项硬上限和 CLOCK 二次机会回收：

- key 改为稳定的 `(parent filesystem id, parent node id, name)`，相同目录
  和名称覆盖旧项，不再随临时 id 无限增殖；
- 命中时设 referenced bit，提供一次二次机会；前台淘汰每轮最多扫 64 项，优先
  回收只有 cache 持有的叶子（详见 `8-6-linux-bounded-dentry-clock-reclaim.md`）；
- clock queue 只保存 key 和 dentry id 元数据，不保存 Arc/Weak；invalidate
  删除 map entry 后 allocation 立即可释放；
- 重建 parent 时用 `Arc::ptr_eq` 校验 parent chain，rename 后不会接回过期
  dentry；
- 并发 race 时在写锁内复查并复用已发布的同一 dentry identity。

**4-KiB page-backed 小对象 slab**：在每个 heap shard 的 buddy 前增加有限
size class：

```text
8, 16, 32, 64, 96, 128, 192, 256, 512, 1024, 2048 bytes
```

每个 class 从 buddy 取 4-KiB backing page；free object 自身保存 one-based
link；full page 第一次 free 后重回 partial list；in-use 归零整页还 buddy。
72-byte Arc 从 128-byte buddy block 降到 96-byte class。大于 2048 bytes 或
对齐不适合的仍走 buddy。

**诊断计数器**：dcache lookup/hit/insert/evict/clock-scan/current/peak 和
heap actual/peak/failure 计数器，只在 `DEBUG_PERF=true` 时启用，正式内核关闭。

Linux 用 shrinker 和内存水位决定回收规模；本轮是 per-filesystem 固定上限，
是有界过渡方案。ext4 当前仍返回 `Revalidate`，等所有 mutation 统一走
invalidation 后才能切到 Stable（见 `8-7-linux-versioned-ext4-dcache.md`）。

## 对应提交

- 状态：当前修改仍在未提交工作树中。
- 顶层基线：`16d5daa3ab8301a41975b15c441678f346874f8b`。
- `os/` 基线：`b0185b3a4522c0ffc52599d73bd17b3d52320815`。
- 建议拆分提交：
  - `mm: add page-backed small-object slabs`
  - `vfs: bound positive dentry cache lifetime`

## 对比提升

**完整 BuildStorm**：

| 指标 | 修改前 run 112 | 修改后生产版 run 121 |
| --- | ---: | ---: |
| 结果 | 1876 s OOM，`tg-xtask` 未生成 | 完整 `rc=0` |
| `tg-xtask` 生成 | 无 | 2306.88 s |
| 完整宿主耗时 | 1918.77 s 后失败退出 | 3321.681 s（55 分 21.7 秒） |
| dentry 项数 | 1,490,845 墓碑 | 峰值 32,721 |
| 最终产物 | 无 | `arceos-helloworld`, 1,714,568 B |

新 dcache 项数比旧 Weak 墓碑数少约 97.8%。

**同条件 VFS stat A/B**（11 轮去首轮后中位数）：

| 指标 | 修改前 | 修改后 | 改善 |
| --- | ---: | ---: | ---: |
| guest elapsed | 147,386 us | 135,449.5 us | 8.10% |
| host elapsed | 188 ms | 173 ms | 7.98% |
| errors | 0 | 0 | — |

原始数据：

```text
testsuits-final/.tmp/final-runs/20260806-bounded-dcache-vfsstat-before-119/results.csv
testsuits-final/.tmp/final-runs/20260806-bounded-dcache-vfsstat-after-120/results.csv
```

该 A/B 测的是 slab + bounded dcache 整体，不能把 8.10% 全部归因给其中一个
机制。聚焦回归 VFS 4/4、lifetime 3/3、exec/page-cache 6/6 通过。没有把
"测试已写但 bare-metal target 无法执行"描述成通过。

以下是 AI 的具体分析，作为存档。

---

## 历史分析背景

这个 OOM 跨 VFS dentry 缓存、buddy allocator 和 Rust Arc 生命周期三个子系统。
旧实现为简化正确性用了无界 Weak map，而 Weak 墓碑的增长只有在长时间运行
BuildStorm 时才表现出来。下面保留完整 OOM 诊断数据、BuildStorm 运行日志、
slab 设计细节和验证环境。

### OOM 详情

- order-7 有 1,490,845 个 live block，平均 requested size 72.48 bytes；
- 8/16/32-byte String 档合计 1,481,563 个，与 dentry 数量只差约 0.6%；
- panic 时 `user=350758929`、`actual=499945704`，free 仍有 36,925,208 bytes——除小对象取整外还有外部碎片无法提供 128-KiB 连续块。

### BuildStorm 生产版证据

```text
testsuits-final/.tmp/final-runs/20260806-buildstorm-bounded-dcache-production-full-121/run/serial.log
testsuits-final/.tmp/final-runs/20260806-buildstorm-bounded-dcache-production-full-121/run/host-metrics.log
testsuits-final/.tmp/final-runs/20260806-buildstorm-bounded-dcache-production-full-121/run/probe-latency.csv
testsuits-final/.tmp/final-runs/20260806-buildstorm-bounded-dcache-production-full-121/images/sdcard-working.qcow2
```

最终输出包含：

```text
Finished `release` profile [optimized] target(s) in 16m 35s
BUILDSTORM_COMPILE mode=multi ok=true elapsed_s=1019.54 cores=12 bytes=1714568
#### OS COMP TEST GROUP END buildstorm ####
```

host 监控记录的 QEMU 峰值 RSS 6,654,352 KiB，最低 MemAvailable 17,869,924 KiB，SwapFree 全程未低于 20,971,516 KiB。

### 验证汇总

- dcache host harness：6/6 通过；
- slab/buddy host harness：9/9 通过；
- RISC-V 与 LoongArch `cargo check` 均通过；
- VFS 4/4、lifetime 3/3、exec/page-cache 6/6 聚焦回归通过；
- `cargo fmt`、`os/` whitespace check 通过，`DEBUG_PERF=false`。

### 后续方向

1. 建立统一 shrinker，让 dentry、inode、page cache 接入同一压力反馈。
2. ext4 切到 `DentryCachePolicy::Stable`，减少 backend lookup。
3. 单个 `RwLock<BTreeMap>` 改为分桶 hash/RCU 读侧，减少 12-hart 争用。
4. 每个 fd 的 128-KiB `read_buf`/`write_buf` 应复用 page-cache frame。
5. 每个物理页的 `Arc<FrameOwner>` 长期应改为静态 PageInfo/refcount 数组。
