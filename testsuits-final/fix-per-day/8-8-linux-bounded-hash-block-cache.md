# 8-8 Linux 风格有界哈希块缓存回收

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
