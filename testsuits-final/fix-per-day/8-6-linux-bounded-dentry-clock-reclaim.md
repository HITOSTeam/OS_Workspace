# 8-6 Linux 风格有界 dentry CLOCK 回收

## 问题概述

强引用、有界 positive dcache 解决了 BuildStorm 的 Weak 墓碑 OOM，但初版 CLOCK
回收仍有两个前台延迟和回收质量风险：

1. cache 满且全部带 `referenced` bit 时，一次 miss 会在全局写锁内扫描整个 32K
   clock，再多扫描一次才驱逐第一项；精确上界是 32,769 次，不是无限循环，但仍是
   不可接受的容量级临界区。
2. 候选没有区分 cache-only dentry 和仍被 child/external `Arc` 引用的 dentry。先驱逐
   parent 不会立即释放内存，还会让其所有 child 在 `Arc::ptr_eq` parent 校验中失效。

本修复把一次前台回收限制为最多 64 次扫描，并优先驱逐真正能释放的 cache-only
叶子。正常命中和未满 cache 路径不增加写锁操作。

## 如何发现

### 运行数据和源码审计

专家复核完整 BuildStorm 后指出初版 `make_clock_room()` 的形状与此前 block-cache
无界 clean scan 风险相同。重新核对诊断 run 116 的 `/proc/perf`：

```text
dcache_peak_entries=32721
dcache_evictions=0
dcache_clock_scans=0
```

因此完整 BuildStorm 虽已成功，但峰值比 32,768 上限少 47 项，完全没有进入本次
回收路径。300 秒快照同样只有 10,742 项。用这些运行声称“淘汰没有问题”没有证据。

初版逻辑在命中项上清掉 `referenced` 后重新排到队尾；32,768 项全热时，先扫描
32,768 次把所有 bit 清零，再扫描一次才遇到冷项，总计精确 32,769 次，并且全程
持 `PositiveDentryCacheInner` 写锁。

另一个问题来自对象关系：`Dentry.parent` 是 `Arc<Dentry>`，所以有缓存 child 的
parent 至少有两个 strong ref。旧算法只看 referenced bit，可能先从 map 摘除 parent；
它仍被 child 保活，内存没有释放，后续相同 child lookup 又会因 parent identity 不同
而 miss。

### perf 与强制满缓存基准

正常 workload 没有触发驱逐，所以新增宿主机定向 harness，直接 `include!` 生产
`os/src/fs/vfs/dentry.rs`，只用最小 VFS/锁桩替代架构依赖。测试固定执行：

1. 创建 32,769 个后端节点；
2. 填满 32,768 项 cache；
3. 再 lookup 全部已有项，将它们标成 hot；
4. 只计时第 32,769 项插入导致的一次回收。

旧、新优化二进制各运行 11 次；随后对相同完整 harness 各做 7 次 `perf stat`：

```text
perf stat -r 7 -e task-clock,cycles,instructions,branches,branch-misses -- \
  /tmp/congcore-dentry-clock-before --ignored benchmark_all_hot_clock_reclaim

perf stat -r 7 -e task-clock,cycles,instructions,branches,branch-misses -- \
  /tmp/congcore-dentry-cache-tests-after --ignored benchmark_all_hot_clock_reclaim
```

环境为 AMD Ryzen AI 9 H 365（10C/20T）、
`rustc 1.99.0-nightly (da80ed070 2026-07-14)`，两边均以 Rust 2024、`-O` 编译。
这组 perf 计数包含构造、填充、预热和单次回收，单次回收延迟另由 harness 内部
`Instant` 包围步骤 4 测量，二者不能混为一个指标。

原始证据：

```text
testsuits-final/.tmp/final-runs/20260806-dentry-clock-budget-before-122/results.csv
testsuits-final/.tmp/final-runs/20260806-dentry-clock-budget-after-123/results.csv
testsuits-final/.tmp/final-runs/20260806-dentry-clock-perf-stat-125/results.csv
```

### Linux 对照

对照本地 Linux `exampleOs/linux` commit
`4549871118cf616eecdd2d939f78e3b9e1dddc48`：

- `fs/dcache.c:793 retain_dentry()` 把可复用的零外部引用 dentry 留在 LRU，并用
  `DCACHE_REFERENCED` 提供 second chance；
- `fs/dcache.c:1230 dentry_lru_isolate()` 对 active count 非零的对象执行
  `LRU_REMOVED`，不会把仍在使用的对象当作有效回收成果；referenced 对象返回
  `LRU_ROTATE`；
- `fs/dcache.c:1297 prune_dcache_sb()` 把 `shrink_control.nr_to_scan` 传给
  `list_lru_shrink_walk()`，一次回收工作量由调用者显式限制；
- `fs/dcache.c:1203 shrink_dentry_list()` 在真正回收时从 LRU/hash 生命周期中完整
  摘除对象，不留下 Weak 墓碑。

Linux 的 7.1-rc7 实现并没有“固定扫描 64 项”这个常量；本内核尚无通用 shrinker，
64 是当前前台回收的局部预算。借鉴的是 `nr_to_scan` 的有界工作量、active 对象跳过
以及 referenced 二次机会语义，不是一比一移植。

## 怎么解决

`os/src/fs/vfs/dentry.rs::make_clock_room()` 改为以下有界策略：

1. 每轮最多扫描 `DENTRY_CLOCK_SCAN_BUDGET = 64` 个 clock record。
2. 将一个候选暂时留在队列外，既为新 entry 预留 metadata slot，也保证窗口内没有
   理想候选时仍有确定性 victim；同一 rank 保留最老项。
3. stale key 或 dentry-id record 只删除 metadata，立即结束本轮并给新 entry 腾出
   slot，不误杀其他 live entry。
4. `Arc::strong_count(&cached.dentry) == 1` 表示只有 cache 自己持有该 dentry；冷的
   cache-only 叶子立即驱逐。
5. 64 项内没有冷叶子时按以下顺序选择 fallback：hot cache-only 叶子、冷但仍被引用
   的 parent/external 对象、hot 且仍被引用的对象。这样优先实际释放内存，同时保留
   referenced 的二次机会倾向。
6. 容量为 1 时，fallback 会暂时让队列为空；循环同时检查队列非空，避免再次
   `pop_front()`。新增专门回归覆盖这一边界。

新增测试覆盖：

- 128 项全热 cache 强制走 64 项 fallback；
- child 在 parent 之前被回收，parent identity 保持不变；
- invalidation 留下的 stale record 不驱逐无关 live entry；
- 最小合法容量 1；
- 原有 strong ownership/second chance 和稳定 key/parent reconstruction 语义。

### 为什么不是只加 `strong_count == 1` 过滤

若只过滤 parent 而没有扫描预算，在大量 active/hot 项下仍可能扫描整个 cache；若只
加预算而没有 fallback，则 64 项都不可回收时无法保持 32K 硬上限。本实现把“优先
有效释放”和“任何输入下有界完成”同时作为不变量。

## Linux 做法与更好方案

当前修复是固定容量架构下的安全过渡，后续仍可继续 Linux 化：

1. 建立全局内存水位和 shrinker，让 dentry、inode、page cache 共享压力反馈；扫描
   预算由回收目标传入，而不是 dcache 固定 64。
2. 把 per-filesystem 32K 上限改成全局预算或按工作集动态分配，避免多个文件系统各自
   达到上限。
3. 用显式 cache/child 生命周期或 bottom-up LRU 代替 `Arc::strong_count == 1` 这个
   当前对象模型下的代理条件。
4. 将单个 `RwLock<BTreeMap>` 改为分桶 hash/RCU 读侧；本轮只限制最坏写锁持有时间，
   没有声称已经达到 Linux lockless lookup。
5. 64 项窗口内若没有 cache-only 叶子，确定性 fallback 最终仍可能摘除 parent 并造成
   后续 miss。这是硬容量与有界延迟下的显式取舍；应持续观察
   `dcache_clock_scans / dcache_evictions` 和命中率，再决定预算或后台回收策略。

ext4 目前仍采用 `DentryCachePolicy::Revalidate`，所以 positive hit 前仍会访问 backend；
本修复解决回收延迟和生命周期，不把它包装成目录 I/O 优化。

## 对应提交

- `os/` 修复提交：`62537ad5474aa1dbffded6a2be88324910d7d43d`
  (`vfs: bound positive dentry cache lifetime`)。
- 顶层仓库通过包含本报告的集成提交记录上述 `os/` 指针；顶层提交 hash 不在提交内容中
  自引用。
- 顶层工作树基线：`16d5daa3ab8301a41975b15c441678f346874f8b`。
- `os/` 工作树基线：`b0185b3a4522c0ffc52599d73bd17b3d52320815`。
- `os/` 提交范围：`src/fs/vfs/dentry.rs`、`src/fs/vfs/mod.rs`，以及 `src/perf.rs`
  中仅与 dcache 相关的计数器。

## 对因提升

### 单次全热回收

11 轮中位数：

| 指标 | 修改前 | 修改后 | 改善 |
| --- | ---: | ---: | ---: |
| clock scans / eviction | 32,769 | 64 | -99.805% |
| 单次回收延迟 | 2,811,844 ns | 10,570 ns | -99.624% |
| 等效加速 | 1.00x | 266.02x | 266.02x |
| evictions | 1 | 1 | 硬上限保持 |

每轮前后都精确产生一次 eviction，证明提升不是跳过回收或放宽 32K 上限得到的。

### perf stat：完整 harness

| 指标（7 轮均值） | 修改前 | 修改后 | 改善 |
| --- | ---: | ---: | ---: |
| task-clock | 53.43 ms | 45.49 ms | -14.86% |
| cycles | 191,583,503 | 176,407,103 | -7.92% |
| instructions | 647,556,547 | 607,842,082 | -6.13% |
| branches | 115,865,869 | 109,416,221 | -5.57% |
| branch misses | 627,773 | 571,588 | -8.95% |
| elapsed | 54.244 ms | 46.541 ms | -14.20% |

完整 harness 的大部分工作是共同的节点构造和 65,536 次 lookup，因此这里仍能观察到
6.13% 指令下降；真正被修复的步骤 4 用内部计时显示 266.02x，二者互相印证。

### QEMU 语义与资源回归

没有再跑一轮约 55 分钟的 BuildStorm：已通过的 run 121 证明完整工作负载和 OOM
闭环，而诊断 run 116 的 `evictions=0/scans=0` 证明它不会覆盖本次唯一修改的回收
路径。强制满 cache 基准对本问题更直接；另以官方镜像做短时真实 guest 回归。

环境：

```text
kernel artifact sha256: 94e19d405dfed78c456d5d9d105acb89df74ba0693fe27eefdc4fd146c820f32
os base commit:          b0185b3a4522c0ffc52599d73bd17b3d52320815 (dirty)
final tests commit:      b5ec6ef8497e1818cbdec3b54bb722f036e57972
sdcard-la-pub.img:       2ad9d955684297abe9db48d94f1b7fcc488268fc8f481408c55b1ec27f520c6a
QEMU:                    11.0.3
guest:                   LoongArch64, 12 harts, 8 GiB, -snapshot
DEBUG_PERF:              false
```

完整启动命令：

```text
expect testsuits-final/.tmp/run_vfs_snapshot_regressions.expect \
  target/loongarch64-unknown-none-softfloat/release/os \
  testsuits-final/.tmp/final-runs/20260806-dentry-clock-bounded-regressions-124
```

脚本展开后的 QEMU 参数为：

```text
qemu-system-loongarch64 -machine virt \
  -kernel /home/shiyicong/temp/CongCore/target/loongarch64-unknown-none-softfloat/release/os \
  -m 8G -smp 12 -nographic -rtc base=utc -no-reboot \
  -drive file=/home/shiyicong/temp/CongCore/testsuits-final/sdcard-la-pub.img,if=none,format=raw,id=x0 \
  -device virtio-blk-pci,drive=x0 \
  -drive file=/home/shiyicong/temp/CongCore/ext4-fs-packer/target/user.ext4,if=none,format=raw,id=x1 \
  -device virtio-blk-pci,drive=x1 \
  -device virtio-net-pci,netdev=net -netdev user,id=net -snapshot \
  -pidfile testsuits-final/.tmp/final-runs/20260806-dentry-clock-bounded-regressions-124/qemu.pid
```

| guest 测试 | host 耗时 | 结果 |
| --- | ---: | --- |
| `vfs_stat_smp_perf_smoke`（12,288 stat） | 207 ms；guest 139,585 us | errors=0 |
| `open_unlink_lifetime_smoke` | 176 ms | PASS |
| `vfs_pathwalk_smoke` | 54 ms | PASS |
| `unix_vfs_path_smoke` | 48 ms | PASS |

QEMU 监控采到峰值 RSS 675,524 KiB、15 线程（短测仅有 1 个两秒采样点）；宿主最低 MemAvailable
23,940,256 KiB，SwapFree 20,971,516 KiB。测试结束后已确认无 QEMU 进程残留。
串口、资源和逐项耗时：

```text
testsuits-final/.tmp/final-runs/20260806-dentry-clock-bounded-regressions-124/serial.log
testsuits-final/.tmp/final-runs/20260806-dentry-clock-bounded-regressions-124/host-metrics.log
testsuits-final/.tmp/final-runs/20260806-dentry-clock-bounded-regressions-124/test-timings.csv
```

## 验证汇总

- exact-source dcache host harness：6/6 通过，另有 1 个手工性能基准；
- slab/buddy host harness：9/9 通过；
- ext4-fs：13/13 通过；
- RISC-V `riscv64gc-unknown-none-elf` 与 LoongArch
  `loongarch64-unknown-none-softfloat` `cargo check` 均通过；
- LoongArch release kernel 和 `/user` image 构建通过；
- QEMU VFS/生命周期 4/4 通过，每项 60 秒硬上限，未发生卡住；
- `cargo fmt --check`、`git -C os diff --check` 通过，`DEBUG_PERF=false`。

仓库内置测试尚不能直接执行：native x86_64 target 受既有架构耦合影响；本轮额外尝试
`cargo check --tests --target loongarch64-unknown-none-softfloat` 也以 61 个错误退出，
错误均为 bare-metal target 缺少 `test` crate，另有 slab/buddy host test 引用 `std`。
上述 harness 直接编译生产 `dentry.rs` 并执行等价测试。没有把“测试已写但未运行”
描述成通过，后续仍应建立真正可运行的 host VFS test target。

## AI 使用说明

专家提供了对初版 dcache 的 R1/R2 审核意见；Codex 重新核对本地 Linux 与 CongCore
源码、纠正“无限/65,536 次”为精确最坏 32,769 次，实施修复并运行 perf、定向测试、
双架构检查和受监控 QEMU 回归。所有表格来自上述原始日志/CSV；没有伪造 perf、
guest 时间、`/proc/perf`、OOM 或测试输出。
