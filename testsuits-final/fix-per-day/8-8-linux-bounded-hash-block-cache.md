# 8-8 Linux 风格有界哈希块缓存回收

## 问题概述

修复 dcache（目录项缓存）内存耗尽（out of memory，OOM）后，BuildStorm（决赛编译
压力测试）的主要开销
转到 ext4 块缓存。旧实现用 `BTreeMap`（有序树表）查找缓存项，每次命中还会向
LRU（最近最少使用）队列追加记录；缓存项越多，查找越慢，队列也会随命中次数增长。

```text
块访问
  -> 抢全局管理器锁
  -> BTreeMap 查找 O(log n)
  -> 命中也追加 LRU 记录
  -> 容量满时扫描并可能回写脏块
```

一次 LoongArch 完整诊断中，旧 8 MiB 缓存驱逐约 388 万块，其中约 153 万块需要
写盘。本批把索引改成哈希表，把每轮扫描限制为 64 项，并合并重复的 LRU 晋升记录。
缓存仍按内存缩放；8 GiB 配置的硬上限为 16,384 块，即 64 MiB。

## 背景知识

先把磁盘想成仓库，把内存想成工作台。每次都去仓库拿零件很慢，所以工人会把近期
用过的零件留在工作台；再次需要时直接拿，工作台满了再放回一件旧零件。

```text
磁盘上的块 = 仓库里的零件
块缓存     = 工作台上的零件
缓存命中   = 工作台上已经有，不必访问磁盘
缓存未命中 = 去磁盘读取，再放到工作台
回收       = 工作台满了，选一件移走
```

块缓存（block cache，按磁盘块保存数据的内存缓存）就是这张工作台。ext4 先把文件
偏移换算成磁盘块号，再用设备编号和块号查缓存。本项目每项保存一个 4 KiB 数据块。

容量必须有上限。缓存数据、索引和队列都占内存；本项目还可能在文件页缓存中保留同一
份文件数据。如果块缓存一直增长，最终会挤占进程页和内核堆，甚至触发内存耗尽。

但上限也不能只看“能装多少数据”。所有块当前仍共用一把管理器锁。缓存越大，锁内
查找和回收越贵，其他核等待越久。因此容量要同时服从内存预算和管理成本。

索引和淘汰顺序解决两个不同问题。`HashMap`（哈希表）回答“某块是否在缓存中”，
平均查找成本接近 O(1)；LRU 队列回答“满了以后先丢谁”。二者配合如下：

```text
                         哈希索引
(device_id, block_id) -> HashMap -> CacheEntry
                                      ^
                                      |
LRU 队首 [最久未用] -> ... -> [最近使用] 队尾
       回收从这里开始              命中后向这里晋升
```

旧索引是 `BTreeMap`。它按树高查找，成本为 O(log n)。这对需要顺序遍历的场景有用，
但块缓存的热路径主要是按完整块号精确查找，哈希表更合适。

旧 LRU 也不是“一项对应一个队列节点”。它在每次命中时追加带版本戳的记录，热块会
留下许多旧记录。于是缓存只有 n 项，队列却可能远大于 n；回收器还要花时间跳过旧戳。

新实现用 promotion（把刚访问的项移向热端）待处理标记合并重复命中。同一项已经等待
晋升时，后续命中不再追加记录。队列长度不再跟热点块的访问次数一起增长。

写缓存还要区分 clean（干净，内容与磁盘一致）和 dirty（脏，内存内容更新但尚未
落盘）。干净项可以直接丢弃；脏项必须先 writeback（回写，把新内容写回磁盘）。

回写比删除干净项慢得多，而且不能拿着全局管理器锁等待磁盘。因此回收先找干净项；
只有扫描窗口内没有合适的干净项时，才选择最旧的脏项并在锁外回写。

“有界回收”是指一轮最多检查固定数量的候选，而不是一直扫描到找到理想对象。本文的
预算是 64 项。若这些项都被引用或暂时上锁，就把它们移到队尾，让下一轮继续向后看。
这样既限制一次持锁时间，也不会永远卡在同一批忙项上。

Linux 的 page cache（文件页缓存）用 `XArray`（按整数页号组织的树形索引）保存每个
`address_space`（一个文件在内存中的页集合）的 folio（一页或连续多页的内存对象）。
它按文件拆分索引，不让所有文件共同抢一棵全局树。

Linux 回收时也给扫描设置预算。直接回收常用的基本批量
`SWAP_CLUSTER_MAX` 是 32；回收器先在锁内隔离一批 folio，再到锁外做较慢的处理。
这能限制一次临界区的长度，并让其他核有机会继续访问缓存。

Linux 的完整回收比“只找干净页”复杂，但成本选择相同：干净页可以直接释放，脏页和
writeback 中的页要与回写线程配合。本文采用 clean-first（优先干净项）来避免前台
容量压力频繁变成单块同步写盘。

本文针对的差别可以压缩为：

| 旧实现 | 新实现 |
| --- | --- |
| `BTreeMap` 查找 O(log n) | `HashMap` 平均查找 O(1) |
| 命中可无限追加 LRU 记录 | 同一项只保留一个待晋升记录 |
| 回收可能反复碰到同一批忙项 | 每轮扫描 64 项并推进位置 |
| 容量不足时频繁驱逐单个脏块 | 先找干净项，必要时锁外回写脏项 |

CongCore 还没有 Linux 的分文件 XArray、分节点 LRU 和后台回写系统。本批只采用近似
常数时间查找、短扫描、优先干净项和锁外输入输出，并为当前全局管理器保留硬上限。

## 如何发现

先在一次完成的 LoongArch BuildStorm 中读取 `/proc/perf`。日志位于：

```text
testsuits-final/.tmp/final-runs/20260806-buildstorm-bounded-dcache-perf-full-116/run/
```

最终计数显示 2,048 项缓存一直满载，发生 3,882,567 次驱逐，其中 1,526,134 次为
脏块驱逐；平均每次写操作正好是 4096 B。随后用同一 16,384 项容量做 240 秒 A/B，
只替换索引和回收算法：

```text
testsuits-final/.tmp/final-runs/20260808-riscv-blockcache-capacity-ab-173/cap16k/
testsuits-final/.tmp/final-runs/20260808-riscv-blockcache-algorithm-ab-174/btree16k/
```

最后在相同哈希算法下比较 2,048 与 16,384 项，分开判断“算法变快”和“容量变大”。
这些运行都保留串口、探针和 host（宿主机）资源日志；短 A/B 均按计划在 240 秒停止。

## 怎么解决

**缩短查找**：把 `BlockCacheManager.entries` 改为 `hashbrown::HashMap`。键只含内部
`device_id` 和 `block_id`，使用确定性整数混合函数，不处理用户提供的字符串。

**限制队列增长**：一次命中最多登记一个待处理 promotion。回收器看到记录后才允许
下一次登记，热点块不会按命中次数制造队列节点。

**限制每轮回收**：每轮最多看 64 项，先清理过期记录，再选无人引用的干净项。窗口
全忙时把它移到队尾；专项测试要求第二轮能越过前 64 项并选到第 65 项。

**保护脏数据**：没有干净候选时选择最旧脏项，在管理器锁外 writeback。若回写期间
发生新查找或写入，查找路径会取消旧驱逐票据，完成回写的旧任务不能删除新数据。

**约束容量和诊断**：容量按 `physical_memory / 64 / 4096` 计算，最大 16,384 项；
8 GiB 得到 64 MiB。entries、capacity 和 clean/dirty eviction 使用原子快照，读取
`/proc/perf` 不必等待管理器锁。

这不是 Linux page cache 的完整移植。CongCore 仍使用一把全局管理器锁，也仍可能在
块缓存和文件页缓存中保存两份数据；本批先保证查找成本、持锁扫描和磁盘等待有明确边界。

## 对应提交

| 项目 | 值 |
| --- | --- |
| 顶层分支 / 基线 | `dev_final` / `b6f2099a39cfa629c2fc76d5beb58c65b796b990` |
| `os/` 基线 | `19ef1c8e9c31bb84aedc19f57a56d32f0342ecae` |
| `os/` 容量接线 | `a24b950e6013e6a3d6da26ddb72b676cccaf3052`（`fs: scale ext4 block cache to memory`） |
| ext4 算法与顶层集成 | 本报告所在提交（建议主题：`fs: bound ext4 block cache reclaim`） |

共享工作树里还有内存映射输入输出高半区、slab（小对象分配器）、inode（索引节点）
元数据锁和其他报告的修改，它们不属于本批提交。

## 对比提升

相同 16,384 项容量下，新算法的归一化输出速率在三个探针分别提高：

| 采样 | BTree 对照 | Hash 候选 | 提升 |
| ---: | ---: | ---: | ---: |
| 1 | 60.99 s / 518 B | 60.78 s / 644 B | +24.75% |
| 2 | 121.49 s / 862 B | 121.18 s / 1,107 B | +28.75% |
| 3 | 195.79 s / 1,227 B | 194.14 s / 1,536 B | +26.25% |

相同哈希算法下，2,048 项增到 16,384 项只在第三个探针领先 4.12%，所以短测的主要
收益来自算法，不是多占 56 MiB 内存。`ext4-fs` 单测为 `15 passed; 0 failed`，两种
架构的静态检查通过。

两组短 A/B 都以预期的 `rc=124` 截止。新算法尚未完成一轮完整 RISC-V BuildStorm，
也没有重采完整运行中的脏块驱逐次数；全局管理器锁仍是下一步要拆的瓶颈。

以下是 AI 的具体分析，作为存档。

---

## 历史分析背景

这个问题同时牵涉缓存索引、LRU 记账、脏块回写和驱逐期间的并发写入，任何一处处理
不当都可能把性能问题变成数据丢失。只看一次完整运行又无法分清容量和算法各自的作用，
所以保留下面的三组消融、Linux 源码对照、竞态分析和完整复现记录。

## 问题概述

修复 dcache OOM 后，完整 BuildStorm 的瓶颈已经转到重复的 ext4 块缓存工作。已有的
LoongArch 完整诊断轮中，8 MiB 块缓存发生了 388 万次驱逐，其中 153 万次需要回写
脏块；与此同时，所有缓存项都由一把全局 manager 锁保护，锁内再使用
`BTreeMap` 做 `O(log n)` 查找并维护可能随命中次数增长的 LRU 记录。

本批没有声称把 CongCore 变成 Linux page cache。它先收敛这个过渡缓存中最昂贵、且
可以独立验证的部分：使用内部 ID 的有界哈希索引、合并重复 promotion、优先回收干净
项，并让每轮回收扫描严格有界且持续推进。容量按物理内存缩放，但考虑到块数据仍会和
frame-backed file cache 重复保存，8 GiB 机器只给 64 MiB，而不是照搬 Linux 可使用
大部分空闲内存的策略。

## 如何发现

### 1. `/proc/perf` 先确认旧缓存的驱逐和锁内工作量

完整诊断日志：

```text
testsuits-final/.tmp/final-runs/20260806-buildstorm-bounded-dcache-perf-full-116/run/
```

该轮是 LoongArch64、12 hart、8 GiB，最终
`BUILDSTORM_RESUME_DONE rc=0`。guest uptime `3475.36 s` 的最终计数为：

| 计数器 | 数值 |
| --- | ---: |
| `ext4_cache_hits` | 100,738,945 |
| `ext4_cache_misses` | 2,603,305 |
| `ext4_cache_evictions` | 3,882,567 |
| `ext4_cache_clean_evictions` | 2,356,433 |
| `ext4_cache_dirty_evictions` | 1,526,134 |
| `ext4_cache_entries / capacity` | 2,048 / 2,048 |
| `block_write_ops` | 1,528,994 |
| `block_write_bytes` | 6,262,759,424 B |

`block_write_bytes / block_write_ops = 4096 B`，而 dirty eviction 和 write op 只差
2,860 次，说明旧的 8 MiB 缓存几乎一直满载，并以单个 4 KiB 脏块为单位驱逐。这个
计数器现场说明容量与回收策略确有问题，但它本身不能证明“直接放大容量”就是主要
收益，所以随后做容量和算法两个消融。

### 2. 同容量 A/B 证明算法包是收益项

两边均使用 RISC-V、8 hart、8 GiB、同一基准镜像的独立 qcow2 overlay，运行同一条
240 秒 `cargo build -p tg-xtask`。关闭 `DEBUG_PERF` 和 QEMU `-perfmap`，探针约每
60 秒读取 uptime、输出文件大小和 `tg-xtask` 是否生成。唯一变量是：

- 候选：`HashMap + promotion 合并 + 有界 clean-first 回收`；
- 对照：恢复旧 `BTreeMap/LRU`；
- 两边容量都固定为 16,384 项。

原始数据：

```text
testsuits-final/.tmp/final-runs/20260808-riscv-blockcache-capacity-ab-173/cap16k/
testsuits-final/.tmp/final-runs/20260808-riscv-blockcache-algorithm-ab-174/btree16k/
```

| 采样 | BTree uptime / output | Hash uptime / output | Hash 归一化输出速率提升 |
| ---: | ---: | ---: | ---: |
| 1 | 60.99 s / 518 B | 60.78 s / 644 B | +24.75% |
| 2 | 121.49 s / 862 B | 121.18 s / 1,107 B | +28.75% |
| 3 | 195.79 s / 1,227 B | 194.14 s / 1,536 B | +26.25% |

三个采样方向一致。Cargo tail 也显示 Hash 候选已进入 `aws-lc-sys`、`serde_json`、
`zerocopy` 和 `getrandom`，而 BTree 对照仍在 `num-traits`，所以领先不是单纯 stdout
缓冲差异。两轮都在 240 秒硬截止以预期的 `rc=124` 结束，探针全程可响应，没有将
“尚未编完”误报为通过。

这个 A/B 证明的是整组索引和回收算法有用，不能仅凭它把约 26% 全部分配给
`HashMap`、promotion 合并或 clean-first 中的某一个子项。

### 3. 容量消融说明 64 MiB 不是短测收益的主要来源

相同 Hash 算法只改变容量 2,048 与 16,384：

```text
testsuits-final/.tmp/final-runs/20260808-riscv-blockcache-capacity-ab-173/cap2k/
testsuits-final/.tmp/final-runs/20260808-riscv-blockcache-capacity-ab-173/cap16k/
```

前两个探针输出完全相同，第三个探针 16K 版本的归一化输出速率只领先 **4.12%**。
因此同容量算法 A/B 的 24.75%--28.75% 提升不是容量放大伪造出来的。64 MiB 上限仍有
价值：它针对完整 BuildStorm 后段工作集，且相对旧 8 MiB 只额外允许按需使用
56 MiB；但短测尚未证明它能减少多少完整运行 I/O，后续必须用完整计数器复核。

## Linux 对照

本批对照本地 Linux `exampleOs/linux` commit
`4549871118cf616eecdd2d939f78e3b9e1dddc48`：

- `mm/filemap.c:1882-1920` 的 `filemap_get_entry()` 在每个
  `address_space::i_pages` 的 XArray 中通过 RCU 查找 folio。Linux 不会让所有文件的
  热路径共同抢一把全局 BTree 锁；
- `mm/vmscan.c:1665-1778` 的 `isolate_lru_folios()` 接受 `nr_to_scan`，在 LRU 锁下
  只隔离一个批次，再到锁外处理；源码还明确说明必须避免不断重扫同一批跳过项；
- `include/linux/swap.h:219` 把直接回收基本批量定义为
  `SWAP_CLUSTER_MAX = 32`，`mm/shrinker.c:420-448` 同样按有界 batch 调用 shrinker；
- Linux 会区分 clean、dirty、writeback folio，并由 flusher 与回收路径协作，而不是
  每次容量不足都在 manager 锁内无界寻找“理想”候选。

CongCore 当前仍是一把全局 manager 锁，也仍保存第二份 4 KiB 块数据；直接移植 XArray、
per-mapping folio 和完整 vmscan 超出本批范围。这里采用相同的约束：查找近似常数时间、
临界区扫描有预算、锁外执行 I/O、跳过忙项后推进扫描位置，并在没有 clean 候选时允许
dirty fallback 保证前向进展。

## 怎么解决

### 哈希索引与有界元数据

`BlockCacheManager.entries` 从 `BTreeMap` 改为 `hashbrown::HashMap`。key 仅包含经过
文件系统映射得到的 `block_id` 和内部 `device_id`，使用确定性 mixer，不接受用户字符串，
因此不为这个内核内部表引入随机种子。命中时最多追加一个待处理 promotion；在回收器
观察该记录前，后续命中被合并，避免队列长度与热点命中次数一起增长。

### 有界 clean-first 回收与前向进展

每次容量压力最多检查 64 个队列记录：

1. 丢弃 stamp 已经过期的记录；
2. 只选择 manager 独占、当前未在驱逐且能立即取得 cache lock 的项；
3. 窗口内优先选 clean 项，找不到时回退到最旧 eligible dirty 项，并在 manager 锁外
   writeback；
4. 如果整个窗口都是外部引用或暂时被锁的项，把窗口旋转到队尾，让下一次扫描继续向前。

审查其他工作人员的初稿时发现第 4 点原本是“原样放回队首”。当最前 64 项全忙、但
第 65 项可回收时，每次重试都会看到同一窗口，形成理论活锁。本批改为推进 scan cursor，
并新增 `bounded_reclaim_scan_advances_past_busy_window`：第一次扫描返回无候选，第二次
必须选中第 65 项。该修正不拿尚未重跑的性能数字邀功，只作为采用修改前必须补齐的
并发前向进展保证。

审查还发现另一个数据正确性竞态：块被选中驱逐后若发生新 lookup，调用者可能写脏后
很快释放 `Arc`；如果只在最终删除时检查引用计数，引用已经回落，旧 writeback target
之后的新数据可能被连同 cache 一起删除。现在任意 lookup 都会立即清除 `evicting`、
安装新的 promotion stamp；旧 eviction ticket 完成 I/O 后只能取消，不能删除该项。
`lookup_cancels_eviction_before_racing_write_is_dropped` 精确覆盖“旧 target 已完成 ->
lookup 写脏 -> 引用释放 -> finish eviction”的顺序，并验证新数据最终可以落盘。

### 按内存缩放但保留硬上限

启动时按 `physical_memory / 64 / 4096` 计算块数，并限制在架构默认下限与 16,384 项
之间。决赛 8 GiB 配置得到 16,384 项，即 64 MiB。缓存按需分配，不在启动时预留；
这一上限是在完整诊断曾出现 347 MiB / 512 MiB 内核堆峰值的条件下选定，不能继续无界
扩大。后续正确方向仍是把 ext4 block cache 与 frame-backed file page cache 合并，而
不是长期保留两份数据。

诊断中的 entries、capacity、clean/dirty eviction 改为原子快照，使 `/proc/perf`
在 manager 锁拥塞时仍可读取，避免诊断命令本身看似卡死。

## 对应提交

| 项目 | 值 |
| --- | --- |
| 顶层分支 / 基线 | `dev_final` / `b6f2099a39cfa629c2fc76d5beb58c65b796b990` |
| `os/` 基线 | `19ef1c8e9c31bb84aedc19f57a56d32f0342ecae` |
| `os/` 容量接线 | `a24b950e6013e6a3d6da26ddb72b676cccaf3052`（`fs: scale ext4 block cache to memory`） |
| ext4 算法与顶层集成 | 本报告所在提交（建议主题：`fs: bound ext4 block cache reclaim`） |

共享工作树中的 MMIO 高半区、slab、inode metadata lock 和其他报告没有加入本批
暂存内容。

## 对因提升与当前边界

能直接证明的提升是：相同 16,384 项容量、相同 RISC-V `tg-xtask` 240 秒 workload
下，新算法三个采样的归一化输出速率分别提高 **24.75% / 28.75% / 26.25%**。
容量 2K -> 16K 在第三个探针只有 **4.12%**，因此主要收益来自算法而不是简单吃内存。

当前不能声称：

- 完整 RISC-V BuildStorm 已经通过；短 A/B 都按设计在 240 秒停止；
- 完整运行的 153 万次 dirty eviction 已经按某个比例下降；新计数必须在下一次完整轮
  重新采集；
- 全局锁问题已经解决。HashMap 缩短了锁内工作，但 per-device/per-inode 分桶仍是后续项；
- 该批约 26% 可以与其他批次的百分比直接相加。

因此下一次候选长测仍应先跑 240--360 秒同源 gate。若进度明显低于旧内核，按约定及时
停止；只有短 gate 不回退后才进入完整 BuildStorm，并持续记录 guest 进度、QEMU
CPU/RSS/I/O、探针延迟和 `/proc/perf`。

## 验证与复现

静态与单元测试：

```sh
TMPDIR=$PWD/.tmp cargo test -p ext4-fs \
  --target x86_64-unknown-linux-gnu
# 15 passed; 0 failed

TMPDIR=$PWD/.tmp ARCH=riscv64 cargo check \
  --manifest-path os/Cargo.toml \
  --target riscv64gc-unknown-none-elf

TMPDIR=$PWD/.tmp ARCH=loongarch64 cargo check \
  --manifest-path os/Cargo.toml \
  --target loongarch64-unknown-none-softfloat
```

受控 A/B 在独立 overlay 中执行相同 guest 命令：

```sh
rm -f /work/riscv-tg-perfmap.out /work/riscv-tg-perfmap.result
read t0 _ </proc/uptime
timeout 240 cargo build -p tg-xtask \
  >/work/riscv-tg-perfmap.out 2>&1
rc=$?
read t1 _ </proc/uptime
echo "$rc $t0 $t1" >/work/riscv-tg-perfmap.result
```

每轮从同一只读基准镜像创建新的 qcow2 overlay；host 侧保留 `serial.log`、
`probe-latency.csv`、`host-metrics.log`、`result.txt` 和 `perf.data`。生产配置确认
`DEBUG_PERF=false`、`DEBUG_SCHED=false`。

| 资产 | 版本 |
| --- | --- |
| final source | `b5ec6ef8497e1818cbdec3b54bb722f036e57972` |
| RISC-V 镜像 SHA-256 | `d899fe43d333d1d17ad8a5f8a8b74b68117b8c1ceacfc3843bfeadb1ca705bd1` |
| QEMU | 11.0.3 |
| perf | 7.1.6 |
| Linux 参考源码 | `4549871118cf616eecdd2d939f78e3b9e1dddc48` |

## AI 使用说明

本批使用 AI 辅助整理 `/proc/perf` 与探针时间线、设计同容量/同算法消融、对照本地
Linux 源码，并审查其他工作人员的候选实现。采用前由源码审查发现“有界窗口反复回到
队首”的前向进展缺陷，以及“驱逐期间的短暂写引用释放后可能丢数据”的竞态，补了
实现和专门回归测试。所有性能百分比均由上列原始
`probe-latency.csv` 的 guest uptime 与 output bytes 重新计算；AI 结论未替代编译、
单测、串口日志或受控 A/B 证据。
