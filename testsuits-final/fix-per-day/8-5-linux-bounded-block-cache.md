# 8-5 Linux 式有界 block cache 回收与 BuildStorm 进度验证

## 问题概述

ext4 全局 block cache 同时存在树索引、命中即追加 LRU 记录、锁内 I/O 和容量过大等
问题。BuildStorm 并未卡在块请求上，却在 cache 元数据维护和集中回收中消耗了大量
时间；简单扩大 cache 反而降低了真实编译进度。

## 如何发现

先用串口、host 资源日志和 `/proc/perf` 排除了 host OOM、swap 抖动及块请求不完成；
再用 `perf record` 与 QEMU `-perfmap` 定位候选热点，并以不带 `-perfmap` 的固定 300 秒
BuildStorm A/B 决定是否保留修改。设计参考 Linux 的 `mm/filemap.c`、
`folio_mark_accessed()`、folio batch、`SWAP_CLUSTER_MAX` 和 `mm/vmscan.c` 中的
索引、访问标记、批量回收及 clean-first 原则。

代表性原始数据和复现命令：

```text
testsuits-final/.tmp/final-runs/20260805-tg-xtask-profile-adaptive-cache-1/
testsuits-final/.tmp/final-runs/20260806-tg-xtask-profile-coalesced-lru-short-1/
testsuits-final/.tmp/final-runs/20260805-tg-xtask-responsive-coalesced-lru-1/
testsuits-final/.tmp/final-runs/20260806-tg-xtask-responsive-coalesced-lru-2k-1/
```

```sh
QEMU_EXTRA_ARGS=-perfmap ARCH=loongarch64 IMAGE_MODE=copy \
  testsuits-final/run.sh shell
perf record -F 99 -e cycles:u -g -p <qemu-pid> -o perf.data -- sleep 45
# guest
timeout 300 cargo build -p tg-xtask 2>&1 | tee /work/tg-xtask-responsive.out
```

每个运行目录同时保存 `serial.log`、`host-metrics.log`、`probe-latency.csv` 和块缓存
计数，便于区分“仍在做输入输出”与“卡在管理器锁”。

## 怎么解决

将索引改为 `HashMap`，保留 miss 的 single-flight；把读取、预读和回写移到 manager
lock 外；命中只设置一次待晋升状态；每轮最多扫描 64 个候选并优先回收 clean entry。
容量按当前单 manager、三层缓存的实际代价保守限制，8 GiB LoongArch 使用 2048 块。
更完整的长期方案是像 Linux 一样统一 page cache，并引入分片索引、folio/LRU batch 和
全局内存压力驱动的 shrinker，而不是继续放大全局 ext4 block cache。

代码上的关键边界是 manager 锁内只选择候选并取得票据，设备输入输出在锁外执行，
最后重新加锁核对 stamp、指针身份和引用数后提交；这对应 Linux 在 folio/LRU 锁外
进行阻塞输入输出的原则。

## 对应提交

- 状态：待提交，当前实现仍位于未提交工作树，不能填写虚构哈希。
- 基线：顶层 `21332ba37bf1ba0efe8229e7f80eeffa3b99a239`；`os/`
  `b0185b3a4522c0ffc52599d73bd17b3d52320815`。
- 建议提交主题：`fs: bound ext4 block-cache reclaim`。

## 对比提升

相同 300 秒 workload 中，31744 块缩减为 2048 块后，Cargo 输出 `69 -> 82` 行
（+18.84%），deps 文件 `190 -> 211`（+11.05%），探针中位数
`1.534 s -> 0.913 s`（-40.46%），最大值 `5.871 s -> 4.771 s`（-18.74%）。
两边 hit rate 均为 98%，且无 block stall/stuck；这证明提升来自更低的管理成本，
不是隐藏 I/O。完整 BuildStorm 与正式 judge 尚未由本条记录证明。

---

## 1. 结论

本批次用 `perf record`、QEMU `-perfmap`、guest `/proc/perf`、30 秒响应探针和
host 资源采样共同分析 BuildStorm 的前置 `cargo build -p tg-xtask`。日志可以排除
块设备静止、内存耗尽和 swap 抖动，但单靠累计计数不能把耗时归因到某个函数；
`perf` 用于提出候选，固定 workload A/B 用于决定是否保留修改。

最终保留的是适合当前架构的 Linux 式原则，而不是照搬 Linux 页缓存的规模：

- 用近似 O(1) 的 `HashMap` 替代全局 cache 中的 `BTreeMap`；
- cache miss、预读和回写 I/O 都在 manager lock 外完成；
- 同一冷块继续使用 `Loading -> Ready` single-flight，避免重复读盘；
- 回收每轮最多检查 64 个 LRU 记录，优先回收 clean entry，找不到时才回写最老的
  dirty candidate；
- 高频命中只排入一个待处理 promotion，避免每次 hit 都扩张 LRU 队列并周期性重建；
- cache 容量按物理内存有上下界地计算，但在当前单一全局 manager、每块又保存一份
  4 KiB 副本的条件下采用保守预算：`RAM / 1024 / 4096`，LoongArch 最低 2048 块，
  RISC-V 最低 512 块，最高 32768 块；
- entries/capacity、clean/dirty eviction 和块设备队列状态通过无阻塞计数器暴露，
  `/proc/perf` 不再为读取 cache 长度等待 manager lock。

在同一份代码中只改变容量公式，LoongArch 12 vCPU、8 GiB、干净 raw 副本、固定
300 秒 workload 的最终 A/B 为：

| cache 容量 | Cargo 输出行 | `target/debug/deps` 文件 | 探针中位数 | 探针最大值 |
| ---: | ---: | ---: | ---: | ---: |
| 31744 块 | 69 | 190 | 1.534 s | 5.871 s |
| 2048 块 | **82** | **211** | **0.913 s** | **4.771 s** |

相同 host 时间内，2048 块版本的两个独立进度指标分别提高 **18.84%** 和
**11.05%**；响应探针中位数下降 **40.46%**，最大值下降 **18.74%**。按各自最后
guest uptime 归一化后，输出行速率仍提高 10.42%，deps 文件速率提高 3.18%。两边
最后一次 cache hit rate 都是 98%，没有 `block_stall_warnings` 或
`block_stuck_warnings`，且所有采样点 `block_submitted == block_completed`。

因此，性能提升不是通过隐藏 I/O 或伪造时间获得的；较小 cache 做了更多真实 I/O，
但减少了当前全局元数据和 LRU 管理开销，guest 实际编译前进得更多。本批次没有完成
完整 BuildStorm 或正式 judge，结论只覆盖该固定 300 秒聚焦 workload。

## 2. 版本与环境

| 资产 | 值 |
| --- | --- |
| 顶层分支 / 基线 | `dev_final` / `21332ba37bf1ba0efe8229e7f80eeffa3b99a239` |
| `os/` 基线 | `b0185b3a4522c0ffc52599d73bd17b3d52320815` |
| final test source | `final-2026` / `b5ec6ef8497e1818cbdec3b54bb722f036e57972` |
| 本地 Linux 参考树 | `exampleOs/linux` / `4549871118cf616eecdd2d939f78e3b9e1dddc48` |
| QEMU / perf | 11.0.3 / 7.1.6 |
| 性能架构 | LoongArch64，12 vCPU，8 GiB |
| 镜像模式 | `IMAGE_MODE=copy`，每次从基准 raw 镜像重新生成 |
| LoongArch 镜像 SHA-256 | `2ad9d955684297abe9db48d94f1b7fcc488268fc8f481408c55b1ec27f520c6a` |
| guest Rust toolchain | `nightly-2026-05-28-loongarch64-unknown-linux-gnu` |

性能 A/B 临时启用相同的 `DEBUG_PERF` 配置以读取诊断字段，采集完成后已经恢复
`false`。正式对照未使用 `-perfmap`；后者只用于之前的热点定位。

## 3. 为什么日志不足以单独判断瓶颈

早期长跑日志同时出现：

- QEMU host CPU 持续增加；
- guest Cargo 输出仍在缓慢增长；
- block submitted/completed 同步前进，in-flight 最终回到 0；
- 没有 queue-full、stall 或 stuck warning；
- host `MemAvailable` 保持 20 GiB 以上，swap 没有持续下降。

这些证据能排除“等待一个永不完成的块请求”、宿主内存耗尽和完全死锁，却不能区分
guest 调度扫描、cache manager、QEMU TCG 翻译或真实编译工作各用了多少 CPU。
因此按以下顺序调查：

```text
串口/资源监控确认是否仍有进度
        -> perf + -perfmap 定位 guest/host 候选热点
        -> Linux 源码确认成熟机制与锁边界
        -> 不带 -perfmap 的固定时限 A/B 决定保留或回滚
```

## 4. perf 证据与测量边界

### 4.1 长采样

240 秒 `cycles:u`、99 Hz、call graph 采样保留约 102K samples，lost samples 为 0：

```text
testsuits-final/.tmp/final-runs/20260805-tg-xtask-profile-adaptive-cache-1/
```

主要 host 开销包括 `tb_gen_code` 27.79% 和 `helper_lookup_tb_ptr` 5.82%；同时约 9%
落在 libdw/ELF 符号查询路径。该次 `/tmp/perf-<pid>.map` 达到约 1.3 GiB，说明
`-perfmap` 自身对 QEMU 翻译和符号解析有明显扰动，不能把这次运行时间拿来做优化
收益结论。

### 4.2 可正常报告的短采样

45 秒短采样保留 19K samples，lost samples 为 0：

```text
testsuits-final/.tmp/final-runs/20260806-tg-xtask-profile-coalesced-lru-short-1/
```

`perf report --no-children` 的主要条目为：

| 符号 | overhead |
| --- | ---: |
| QEMU `tb_gen_code` | 14.53% |
| QEMU `helper_lookup_tb_ptr` | 9.51% |
| guest `has_ready_rt_any_at_or_above` 多个代码地址合计 | 至少 12.18% |
| `gelf_getshdr` | 4.47% |
| `gelf_getsymshndx` | 2.73% |

这段处于编译早期，cache 尚未进入稳定回收压力；block-cache 符号单项约 0.02% 或
更低，不能据此断言长时间 cache 回收没有成本。它能可靠指出调度器扫描是下一候选，
也再次证明 perfmap/libdw 会污染绝对耗时。

一次较长 profile 因探针超过 20 秒而自动停止；旧 driver 没有先让 perf 完整 flush，
得到的 `perf.data` 不可读，已明确排除。修正后的 driver 会先复制准确 PID 的 map、
停止 QEMU，并在必要时向 perf 发送 `SIGINT`，不再无限等待。

## 5. Linux 参考与本地取舍

本地 Linux 源码中的对应机制为：

| Linux 机制 | 本地参考 | 本次采用的性质 |
| --- | --- | --- |
| page-cache 索引 | `include/linux/xarray.h`、`mm/filemap.c` | 查找不随 entry 数做树高增长 |
| 访问状态 | `mm/swap.c:449-495` `folio_mark_accessed()` | hit 先记录 referenced，避免每次立即重排全局 LRU |
| 访问批处理 | `include/linux/folio_batch.h:14-32` | 小批量摊销 LRU bookkeeping；Linux batch 为 31 |
| 有界回收 | `include/linux/swap.h:219`、`mm/vmscan.c` | 按 cluster/batch 扫描；Linux `SWAP_CLUSTER_MAX` 为 32 |
| clean file reclaim | `mm/vmscan.c` | 优先释放无需 I/O 的 clean file page，dirty page 进入 writeback |

本地没有 Linux 的 XArray、folio lock、per-node LRU、per-CPU folio batch 和全局内存
回收器。当前 ext4 block cache 还会与 inode/file page cache 重复保存数据，并由一个
spin manager lock 管理。因此不能从“Linux page cache 可使用大量空闲内存”推出
“把本地全局 block cache 从 2048 直接放大到 31744 一定更快”。

本次复制的是 Linux 的可扩展性原则：索引、访问标记、有限批量、clean-first 和锁外
I/O；容量则由本地 A/B 决定。64-entry scan 比 Linux 的 31/32 稍大，但仍为常数上界，
适合当前 4 KiB block 粒度。

## 6. 实现

### 6.1 索引与容量

`CacheKey = (device_id, block_id)` 现在进入 no-std `hashbrown::HashMap`。内部 key 来自
已验证的文件系统映射，使用确定性整数 mixer，不引入随机种子和启动期依赖。

`os::mm::init()` 在 frame allocator 初始化后根据检测到的物理内存调用
`configure_block_cache_for_memory()`。容量只设定上限，不预分配内存。8 GiB
LoongArch 得到 2048 块，即 8 MiB 数据；RISC-V 会从原来的 512 块最低值扩到 2048
块。极大内存机器仍被 32768 块上限约束，避免当前单 manager 无限制膨胀。

### 6.2 hit promotion 合并

旧实现每次 hit 都生成一个新 stamp 和 queue record；队列达到容量的 8 倍后，又在
manager lock 内遍历整个 map 重建队列。热块访问会把一次 O(1) hit 周期性放大为
O(cache entries)。

新 `promotion_pending` 表示已经有一个较新的队列记录代表自上次 reclaim inspection
以来的 hit。后续 hit 只返回 cache，不再重复排队；scanner 看到该 current record 时
清除 pending，使未来访问能够再次晋升。单测验证高频访问后队列只增加一个记录，且
回收顺序仍淘汰真正更老的块。

### 6.3 有界 clean-first reclaim

容量满时，每轮最多从 LRU 前端检查 64 个 current record：

1. stale record 直接丢弃；
2. 被外部持有、正在 eviction 或 cache lock 暂时不可用的记录保持原相对顺序；
3. 找到第一个 clean 且无人引用的 entry 就建立 eviction ticket；
4. 窗口内没有 clean entry 时，使用最老的 eligible dirty entry；
5. manager lock 释放后才进行 writeback，再短暂加锁提交删除或取消结果。

这既避免持有 manager lock 做设备 I/O，也避免为了寻找 clean entry 无界扫描全部 dirty
cache。发生竞态时通过 stamp、pointer identity 和 strong count 复核，不删除重新被
引用或晋升的 entry。

### 6.4 可观测性

新增 `ext4_cache_{clean,dirty}_evictions`、entries/capacity 和块队列诊断。
entries 使用 atomic 维护，因此 `/proc/perf` 在高压时不会反过来等待 cache manager，
避免“诊断命令自己看起来卡死”。正常构建中 `DEBUG_PERF=false`，没有周期性性能日志。

## 7. 候选方案、实测与回滚

所有性能候选都受 20 秒响应硬上限保护。超过上限时 driver 立即停止 QEMU；没有让
疑似卡死测试无限运行。

### 7.1 直接扩大 cache：拒绝

早期 900 秒测试中，固定 2048 块版本达到 128 行/362 deps；直接按 `RAM/64` 放大到
31744 块的 BTreeMap 版本只有 114 行/307 deps，分别退化 10.94% 和 15.19%。这推动
后续改用 HashMap、有界回收和精确 A/B，而不是继续盲目扩容。

### 7.2 无界 clean scan：拒绝

为了尽可能避开 dirty writeback 而扫描整个队列后，guest 前台命令超过 300 秒没有
响应，约 500 秒被主动终止。根因是全局 manager lock 下的无界工作量使前台 cache
lookup 饥饿。改为 64-entry 上限后，同 workload 所有常规探针恢复到约 4 秒以内。

### 7.3 promotion 合并：保留

同为 31744 块的 300 秒测试中，有界回收版本为 65 行/174 deps；加入 promotion 合并
后为 69 行/190 deps，分别增加 6.15% 和 9.20%。最大探针从 4.012 秒升到 5.871 秒，
但仍远低于 20 秒硬上限；编译进度收益明确，因此保留，并用最终 2048 容量降低尾延迟。

### 7.4 CLOCK second chance：拒绝

用单 bit CLOCK 代替 promotion queue 后只有 61 行/176 deps，最大探针 6.255 秒，
同时产生更多重复读取。其局部实现虽简单，但当前 workload 的热集识别不如合并 LRU，
已经完整回滚。

### 7.5 RT runnable 快路径：拒绝

短 perf 指出 `has_ready_rt_any_at_or_above` 为显著热点。参考 Linux
`kernel/sched/sched.h` 中 `rt_rq.rt_nr_running` 与 priority bitmap，尝试加入全局/每 hart
RT runnable 计数。跨架构 `cargo check` 成功，但相同 300 秒测试只有 68 行/182 deps，
最大已完成探针 6.796 秒，随后探针超过 20 秒，driver 以 126 立即终止。缺少性能证明，
该调度器修改已经完整回滚；perf 热点保留为后续研究方向，不进入本批次结果。

### 7.6 最终容量 A/B：保留 2048

最终比较只改变 `BLOCK_CACHE_MEMORY_DIVISOR` 及其容量单测期望：

```text
31744:
testsuits-final/.tmp/final-runs/20260805-tg-xtask-responsive-coalesced-lru-1/

2048:
testsuits-final/.tmp/final-runs/20260806-tg-xtask-responsive-coalesced-lru-2k-1/
```

每个目录保留 `metadata.txt`、`serial.log`、`probe-latency.csv`、
`host-metrics.log` 和 `host-perf-stat.csv`。300 秒到期的 workload 返回 124 是预期的
GNU `timeout` 状态；QEMU 随后正常退出。2048 版本最后一次采样为 82 行/211 deps、
cache hit 98%、257265 evictions，其中 159399 clean、97866 dirty；215722 个 block
request 全部完成。更多 eviction 没有造成前台停顿，证明本地瓶颈不是单纯磁盘读次数。

## 8. 验证

### 8.1 block-cache 单元测试

为避免工作区默认裸机 target 缺少 `std/test`，单测显式使用 host target：

```zsh
TMPDIR=$PWD/.tmp cargo test --manifest-path ext4-fs/Cargo.toml \
    --target x86_64-unknown-linux-gnu block_cache
```

结果：11 passed，0 failed，2 filtered。覆盖 single-flight、锁外 miss I/O、锁外
writeback、并发写 generation、预读不覆盖 dirty block、promotion 合并、LRU 顺序、
有界 dirty fallback、clean-first、dirty writeback 和容量上下界。

### 8.2 双架构静态检查

```zsh
TMPDIR=$PWD/.tmp ARCH=riscv64 cargo check --manifest-path os/Cargo.toml \
    --target riscv64gc-unknown-none-elf
TMPDIR=$PWD/.tmp ARCH=loongarch64 cargo check --manifest-path os/Cargo.toml \
    --target loongarch64-unknown-none-softfloat
```

两项退出码均为 0。现存 warning 来自项目既有代码，不是本批次新增编译错误。

### 8.3 状态检查

- `DEBUG_PERF=false`；
- 没有残留 QEMU 或 perf 进程；
- `git diff --check` 与 `git -C os diff --check` 均通过；
- 性能期间 host `MemAvailable` 始终充足，swap 没有持续消耗；
- 基准镜像 checksum 未变化，所有性能写入发生在 copy 工作镜像。

## 9. 复现步骤

性能 driver 执行的核心步骤如下；正式复现应保持 A/B 只改变待测参数：

```zsh
# 每次都让 run.sh 从同一基准镜像生成干净副本。
ARCH=loongarch64 SMP=12 MEM=8G IMAGE_MODE=copy \
    FINAL_RUN_ROOT=testsuits-final/.tmp/final-runs/<name>/run-sh \
    testsuits-final/run.sh shell

# guest 内：固定环境和 300 秒 workload。
export PATH=/root/.cargo/bin:/usr/local/bin:/usr/bin:/bin:/sbin:/usr/sbin
export HOME=/root RUSTUP_HOME=/root/.rustup CARGO_HOME=/root/.cargo
export RUSTUP_TOOLCHAIN=nightly-2026-05-28 CARGO_NET_OFFLINE=true
cd /work/tgoskits
timeout 300 cargo build -p tg-xtask 2>&1 | tee /work/tg-xtask-responsive.out
```

每 30 秒从另一个 guest shell 读取 `/proc/uptime`、`/proc/perf`、输出行数和
`target/debug/deps` 文件数，host 端给单次探针设置 20 秒 deadline；同时采样 QEMU
RSS、线程数、host `MemAvailable/SwapFree` 和不带 `-perfmap` 的 `perf stat`。命令和
`-perfmap` 的安全用法已补充到 `testsuits-final/AGENTS.md`。

## 10. AI 使用说明与边界

本批次使用 AI 辅助读取本地 Linux/CongCore 源码、设计候选机制、修改 Rust/脚本、
运行 perf/回归、监控 QEMU 资源并整理报告。所有性能数字来自保留的真实串口、perf
和 host 采样文件；AI 没有修改 final judge、guest uptime、Cargo 输出或评分逻辑。

尚未完成：完整 `cargo build -p tg-xtask`、完整 BuildStorm、完整 final judge 和完整
LTP。本报告不把“300 秒内前进更多”表述为 BuildStorm 已通过。下一步应在当前保守
cache 上运行更长但仍有硬截止的 BuildStorm 阶段；若再次优化调度器，必须重新使用
perf 定位并用相同响应测试证明，不能仅凭热点百分比合入。
