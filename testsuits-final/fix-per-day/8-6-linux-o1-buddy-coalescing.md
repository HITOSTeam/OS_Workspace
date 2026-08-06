# 8-6 Linux 风格 O(1) buddy 合并

## 问题概述

旧 `buddy_system_allocator` 在每个 order 合并时线性扫描单向 free list 查找 buddy；碎片
增多后，释放成本随运行时间恶化，并让 guest shell 探针逐步饥饿。第一版 O(1) 替代又
把最小块扩大到 32 bytes，完整冷启动随后证明该表示不可接受。

## 如何发现

BuildStorm 超过 20 秒的探针被及时停止后，准确 QEMU PID 的无 `-perfmap` perf 显示
`ShardedHeap::dealloc` 占总 period 31.8996%。源码审计确认 crate 按 order 线性找
buddy。设计参考 Linux `mm/page_alloc.c::__free_one_page()`、
`__del_page_from_free_list()` 与 `PageBuddy` 的 O(1) membership/list removal。

```text
.tmp/final-runs/20260806-buildstorm-shared-only-full-97/run/
.tmp/final-runs/20260806-linux-buddy-o1-resume-before-99/
.tmp/final-runs/20260806-linux-buddy-o1-resume-after-100/
.tmp/final-runs/20260806-linux-buddy-o1-resume-perf-after-102/run/perf.data
```

```sh
perf record -F 99 -e cycles:u -g -p <qemu-pid> \
  -o perf-slow-probe.data -- sleep 15
# guest
timeout 300 cargo build -p tg-xtask
```

## 怎么解决

项目内实现 free-head bitmap 与 intrusive doubly-linked list：用 XOR 得到 buddy，位图
和 order 验证后 O(1) 摘链。第一版 32-byte 节点只用于证明算法，随后因内部碎片被
packed 8-byte link 版本取代。长期应把 buddy 保留给页/大块，Rust 小对象进入 slab；
不能靠扩大 heap 或吞掉分配失败掩盖问题。

free block 头保存 order 与双向链表 link；free-head bitmap 先验证 buddy 地址确实是空闲
块头，再按已知节点 O(1) 摘链。分配从第一个非空 order 向下拆分，释放按异或运算计算
伙伴并逐级合并。
Linux 把 order、buddy 状态和链表节点放在预分配的 `struct page` 中；CongCore 的内核
堆没有独立页元数据，所以第一版把 link 放入空闲块自身并用 bitmap 验证。算法边界
相同，但 32-byte 最小节点造成额外内部碎片，因此表示层后来必须继续压缩。

## 对应提交

- 状态：待提交；32-byte 表示已被否决，最终提交必须与 packed/slab 后续一起描述，
  不能把第一版写成最终实现。
- 基线：顶层 `21332ba37bf1ba0efe8229e7f80eeffa3b99a239`；`os/`
  `b0185b3a4522c0ffc52599d73bd17b3d52320815`。
- 建议提交主题：`mm: make buddy coalescing constant-time`。

## 对比提升

相同失败磁盘状态的 300 秒 A/B 中，输出 `2012 -> 2639 bytes`（+31.16%），关键探针
`9455 -> 586 ms`（-93.80%）；dealloc perf 占比 `31.8996% -> 0.5220%`
（-98.36%）。但 32-byte 冷启动约 908 秒 OOM，因此算法收益成立，第一版表示不成立。

---

## 结论

本轮确认并保留 Linux 风格 O(1) buddy membership/list removal 这一算法方向，但
**不保留本轮第一版 32-byte 最小块表示**。完整 BuildStorm 首次运行在约 874 秒后出现严重的
shell starvation，随后 20 秒探针无法返回并被及时终止；无 `-perfmap` 的 `perf`
显示旧 `ShardedHeap::dealloc` 占 guest 总 period **31.90%**。根因不是 host OOM，
而是 `buddy_system_allocator 0.6` 在每一级合并时线性遍历整条单向空闲链表寻找 buddy。

参考 Linux 的 `PageBuddy + order + list_del()`，本轮用一个项目内的最小 intrusive
buddy heap 替换该 crate：位图判断 buddy 是否是空闲块头，空闲内存保存 order 和双向
链表节点，已知 buddy 可 O(1) 摘除。相同 BuildStorm 中断磁盘状态的严格 300 秒 A/B
显示：新内核进度增加 **31.16%**，关键探针延迟下降 **93.80%**，分配器 dealloc 的
perf 占比降到 **0.522%**。但 300 秒窗口内 `tg-xtask` 仍未编完，因此这轮只证明
优化有效，不能写成 BuildStorm 已通过。之后的全新 overlay 完整运行在约 908 秒
触发 guest heap OOM，证明 32-byte 最小块造成的内部碎片不可接受；后续 packed
8-byte link 修复见 `8-6-linux-packed-buddy-links.md`。

## 问题与停止证据

上一轮 shared-only mmap 状态优化完成后，从官方 LoongArch root image 的全新 qcow2
overlay 执行完整 `buildstorm_testcode.sh`：

```text
.tmp/final-runs/20260806-buildstorm-shared-only-full-97/
kernel sha256: e73f908a6489bf0d8235d7c58a61b89a78996d3737221e07d182150fd8caf0c3
official image sha256: 2ad9d955684297abe9db48d94f1b7fcc488268fc8f481408c55b1ec27f520c6a
LoongArch64 / 12 harts / 8 GiB
```

`cargo build -p tg-xtask` 的输出一直在增加，但响应逐步恶化：

| guest uptime | output bytes | O(1) probe host latency |
| ---: | ---: | ---: |
| 61.08 s | 1261 | 650 ms |
| 367.10 s | 4229 | 1858 ms |
| 495.15 s | 4629 | 4033 ms |
| 802.84 s | 5969 | 1779 ms |
| 873.98 s | 6076 | 10976 ms |
| next probe | - | **超过 20000 ms，自动停机** |

停机后只读检查 overlay，`buildstorm-resume-full.out` 为 6314 bytes、200 lines，仍在
真实编译 `h2`、`hyper`、`nix`、`clap_builder`、`hyper-util` 等 crate；没有
`tg-xtask` 和最终产物。它不是伪进度或最后链接阶段。

资源监控同时显示 QEMU RSS 约 4.05 GiB，host `MemAvailable` 约 21.0 GiB，
`SwapFree` 约 16.2 GiB，读写计数仍在增长。QEMU、perf 和 guest workload 在硬停后
均已退出，所以不能用“资源没有释放”解释变慢。

## perf 定位

在第一次 10 秒级慢探针后附加：

```zsh
timeout --signal=INT --kill-after=5s 25s \
    perf record -F 99 -e cycles:u -g -p "$qemu_pid" \
    -o perf-slow-probe.data -- sleep 15
```

原始数据：

```text
.tmp/final-runs/20260806-buildstorm-shared-only-full-97/run/perf-slow-probe.data
samples: 17909
lost samples: 0
aggregated period: 349435765871
```

按每个样本调用链中最后一个 guest kernel PC 汇总，并用该次运行的精确 ELF 解析：

| guest symbol | period share |
| --- | ---: |
| `ShardedHeap::dealloc` / buddy `Heap::dealloc` | **31.8996%** |
| `MemorySet::discard_madvise_dontneed_range` | 6.7114% |
| ext4 `Inode::stat_snapshot` | 3.2232% |
| ext4 `Inode::is_dir` | 1.2973% |
| `frame_alloc` | 1.2204% |

旧 crate 的 `Heap::dealloc` 在每个 order 上遍历 `LinkedList`，直到发现地址等于
`current ^ block_size` 的 buddy；空闲块多且碎片化后，释放路径变成多级线性扫描。
这与 build 前段正常、运行越久 shell 越难被调度，以及 dealloc 占 31.9% 三项证据
一致。

## Linux 参考与设计

本地 Linux 参考：

- `exampleOs/linux/mm/page_alloc.c::__free_one_page()`；
- `exampleOs/linux/mm/page_alloc.c::__del_page_from_free_list()`；
- `exampleOs/linux/include/linux/page-flags.h` 的 `PageBuddy`。

Linux 先通过 page buddy 状态和 order 判断伙伴，再对已知节点执行 `list_del()`，不会
为了找到 buddy 扫描整条 order free list。CongCore 的 kernel heap 按字节而不是按页
分配，因此没有照搬完整 zone/page 体系。第一版只保留解决根因所需的同一不变量：

1. 每个 shard 仍有独立 buddy heap 和原有 `SpinMutex`；local shard 分配、跨 shard
   fallback、按地址归还 shard、`LocalIrqSaveGuard` 全部不变。
2. 每 32 bytes 一个位图槽，只有 free block head 置位；位图检查是 O(1)。
3. free block 自身前 24 bytes 保存 `prev`、`next` 和 `order`；第一版最小块为 32 bytes。
4. 分配从目标 order 向上找第一个非空链表，弹出后逐级拆分。
5. 释放用 XOR 得到 buddy 地址，检查范围、free-head bit 和相同 order，再通过双向链表
   O(1) 摘除并继续向上合并。

每次释放最多检查 arena 的 order 数，因此从“每一级扫描任意长度链表”变为
`O(log arena)` 个常数时间合并步骤。位图按最大 shard 静态定长，LoongArch 12 shard
合计使 `.bss` 从 537965504 增至 540066000 bytes，即增加 **2100496 bytes**；没有
在 allocator 内动态分配 metadata，也没有递归进入全局 allocator。后续完整运行证明
该表示层虽然快，却把小分配向上取整过多；当前代码已经改用 8-byte packed link，不能
再把本节的 32-byte 数据当作最终实现。

本轮没有重新引入已被 A/B 拒绝的 per-hart magazine 或 small-object cache。那些方案
存在 refill/drain、迁移和通用 IO 回退；本实现只修复 perf 命中的 buddy membership
与摘链复杂度，没有绕过分配失败、伪造统计或为 BuildStorm 特判。

代码：

- `os/src/mm/buddy_heap.rs`：intrusive buddy、位图和 host 单测；
- `os/src/mm/heap_allocator.rs`：现有 shard 改用新 heap；
- `os/src/mm/mod.rs`：注册模块；
- `os/Cargo.toml`：kernel 不再依赖 `buddy_system_allocator`，user allocator 不受影响。

## 相同磁盘状态严格 A/B

先冻结全跑失败时的 qcow2 overlay，再从它创建两个新的 qcow2 child；父盘在两次运行
期间只读，两个 child 的 backing chain、root 内容、Cargo target、SMP、内存和命令
完全相同。A/B 不带 `-perfmap`，避免 JIT map 扰动真实耗时。

```text
old ELF: e73f908a6489bf0d8235d7c58a61b89a78996d3737221e07d182150fd8caf0c3
new ELF: 39f6228d652b4855ae2e6be9734723340bc8f8f3684b2bb7fc9cda9df23260b9
workload: timeout 300 cargo build -p tg-xtask
probe: /proc/uptime + stat(output) + stat(tg-xtask), no recursive find
probe interval/hard limit: 30 s / 20 s
```

原始目录：

```text
.tmp/final-runs/20260806-linux-buddy-o1-resume-before-99/
.tmp/final-runs/20260806-linux-buddy-o1-resume-after-100/
```

| uptime | old output / probe | new output / probe |
| ---: | ---: | ---: |
| ~92 s | 1107 B / 748 ms | 1107 B / 434 ms |
| ~153 s | 1280 B / 657 ms | 1281 B / 429 ms |
| ~184 s | 1398 B / 797 ms | 1497 B / 349 ms |
| ~214 s | 1583 B / 621 ms | 1774 B / 484 ms |
| ~245 s | 1827 B / **9455 ms** | 2149 B / **586 ms** |
| final | 2012 B，下一探针 >20 s 被停机 | 2639 B，300.19 s timeout 正常返回 |

结果：

- 300 秒最终输出进度 `2012 -> 2639 bytes`，增加 **31.16%**；
- 前 9 个完整探针中位延迟 `701 -> 434 ms`，下降 **38.09%**；
- 相同退化点探针 `9455 -> 586 ms`，下降 **93.80%**；
- old 在 guest timeout 到期附近无法响应，由 host 20 秒硬停；new 的 guest timeout 和
  随后的 final probe 均正常返回；
- 两者都没有生成 `tg-xtask`，所以该聚焦窗口不是完整通过证据。

## 优化后 perf 复核

从同一个只读父盘再创建新 child，在新内核约 214 秒处抓 15 秒 profile：

```text
.tmp/final-runs/20260806-linux-buddy-o1-resume-perf-after-102/run/perf.data
samples: 17703
lost samples: 0
sample duration: 15003.571 ms
```

精确新 ELF 解析后，`ShardedHeap::dealloc` 为 **0.5220%**，相对旧 profile 的
31.8996% 下降 **98.36%**；最高 guest 单点 `__rust_alloc` 也只有 0.8292%，热点已
分散而不是移到另一个线性 free-list 函数。采样期间 214/260 秒探针分别为
462/467 ms。

本次耗时 A/B 特意关闭 `-perfmap`。需要解析 QEMU JIT/guest 符号时，应按
`testsuits-final/AGENTS.md` 的“使用 perf 和 QEMU `-perfmap`”章节启动：

```zsh
QEMU_EXTRA_ARGS=-perfmap ARCH=loongarch64 IMAGE_MODE=snapshot ./run.sh shell
```

并保存与准确 QEMU PID 对应的 `/tmp/perf-${qemu_pid}.map`。`-perfmap` 很有用，但会
显著放大 TCG/JIT 开销，只用于短时定位，不能拿带 map 的运行与无 map 的运行比较
BuildStorm 时间。

## 回归

分配器自身用 host memory 运行四项直接单测：完整合并、混合 layout 对齐/不重叠、
不跨不对齐 shard 边界合并，以及 20,000 步确定性 alloc/free churn。全部通过：

```zsh
rustc --edition 2024 --test os/src/mm/buddy_heap.rs \
    -o /tmp/congcore-buddy-heap-tests
/tmp/congcore-buddy-heap-tests
```

双架构检查通过：

```zsh
TMPDIR=$PWD/.tmp ARCH=loongarch64 cargo check --manifest-path os/Cargo.toml \
    --target loongarch64-unknown-none-softfloat
TMPDIR=$PWD/.tmp ARCH=riscv64 cargo check --manifest-path os/Cargo.toml \
    --target riscv64gc-unknown-none-elf
```

LoongArch 运行回归：

- exact old/new fork + 16-thread 测试各 11 轮均通过；guest 中位数
  `128399 -> 128893 us`（+0.38%，噪声范围），host `157 -> 155 ms`；
- `.tmp/final-runs/20260806-linux-buddy-o1-mmap-regressions-103/`：lazy fault、private
  page cache、private madvise、shared cross-mm、shared truncate、exec teardown 共 6 项
  通过；
- `.tmp/final-runs/20260806-linux-buddy-o1-vfs-regressions-104/`：12-worker stat、
  open/unlink lifetime、pathwalk errno、Unix VFS path 共 4 项通过。

定向 `cargo fmt`、`git diff --check` 通过。构建只有仓库既有 warnings。

## 后续完整运行对第一版的否决

在官方 raw image 的全新 qcow2 child 上运行 32-byte 版本：

```text
.tmp/final-runs/20260806-buildstorm-linux-buddy-full-105/
kernel sha256: 44dae4b24165c298c07145a54ed64b01b024140635eeeb0049487d0c1fd4e8cf
LoongArch64 / 12 harts / 8 GiB
```

它解决了旧版本的延迟退化，探针一直在 234--730 ms 返回并越过旧停止点；但 guest
uptime 约 907.9 秒时真实触发：

```text
[oom] heap alloc failed: layout=Layout { size: 201096, align: 1 }
user=319614722 actual=464679392 total=536870912
```

此时 QEMU RSS 约 4.36 GiB，host 仍有约 23 GiB 可用内存、约 20 GiB swap free，
所以不是宿主压力。`actual/user` 约 1.45，直接证明 32-byte 最小块和 power-of-two
向上取整带来的内部碎片使 guest kernel heap 先耗尽。该 qcow2 通过 `qemu-img check`，
无镜像损坏。32-byte 表示因此被否决；O(1) 合并机制保留并在下一轮压缩回 8-byte
粒度。

## 下一步与复现说明

这一轮已经用真实 BuildStorm 状态证明 O(1) 方向的性能提升，也用完整运行否决了
第一版 32-byte 表示。packed 8-byte 修复及其续跑通过证据单独记录在
`8-6-linux-packed-buddy-links.md`；不能因为本轮局部 A/B 成立就放宽卡死标准。

本轮由 AI 协助检查 perf 调用链、比对本地 Linux 源码、实现和生成测试命令；所有
结论均来自上述精确 ELF、磁盘 backing chain、原始 CSV/serial/perf.data 和实际
marker，不使用固定返回值、缩短 guest uptime 或伪造 BuildStorm 产物。
