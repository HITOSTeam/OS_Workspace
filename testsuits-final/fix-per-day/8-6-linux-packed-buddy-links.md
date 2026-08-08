# 8-6 把 buddy 空闲链表指针压进一个 u64，恢复 8-byte 最小粒度

## 问题概述

上一轮用 O(1) 双向链表和位图替换了线性扫描（见 `8-6-linux-o1-buddy-coalescing.md`），
但第一版空闲节点占 32 bytes（prev + next + order 各一个指针宽度），使得所有
1–31 byte 的请求至少浪费到 32 bytes。BuildStorm 在 guest uptime 约 908 秒耗尽
512 MiB 内核堆，OOM dump 显示 `actual/user ≈ 1.45`——真正分给用户的只有
320 MiB，分配器自己吃掉了 145 MiB。

```text
问题示意：一次 1-byte 分配在 32-byte 最小块下的浪费

请求: 1 byte
实际分配: 32 bytes  [prev 8B | next 8B | order 8B | 用户1B + 填充7B]
浪费率: 31/32 = 96.9%
```

解决方法：把 prev、next、order 三个字段编码进一个 64-bit word，最小块恢复到
8 bytes。续跑越过原 OOM 点并以 `rc=0` 完成；但全新冷启动在 1836 秒再次 OOM，
说明 buddy 本身的 2 的幂取整和外部碎片仍需后续 slab 分层解决。

## 背景知识

这一节给只上过一门 OS 课的读者铺路。已经熟悉 buddy 分配器的可以跳过。

**为什么内核要按 2 的幂分配**。Buddy 分配器把内存切成 2^0、2^1、2^2 …
大小的块，好处是合并时只需要看"同一个父块拆出来的另一半"（伙伴）是不是也
空闲，不用扫描整个链表。代价是：请求 5 bytes 就要给 8 bytes，请求 9 bytes
就要给 16 bytes——总是向上取整到最近的 2 的幂。

**free_area 数组和链表结构**：

```text
free_area[0]: 最小块链表  ──→ [块A] ⇄ [块B] ⇄ ...
free_area[1]: 2倍块链表   ──→ [块X] ⇄ [块Y] ⇄ ...
free_area[2]: 4倍块链表   ──→ [块M] ⇄ ...
  ...
```

需要 2^k 大小的块时，从 `free_area[k]` 摘一个。如果 k 级空了，从更高级拆分。
释放时用 XOR 算出伙伴地址，检查伙伴也空闲就合并成更大块挂到上一级。

**伙伴地址怎么算**。一块起始地址为 addr、大小为 2^order 的块，它的伙伴是：

```text
buddy_addr = addr XOR (1 << order)
```

一条异或指令就能找到伙伴，不需要遍历链表。

**为什么 Linux 把链表指针放在页描述符（struct page）里**。Linux 为每个物理页
预分配了一个 `struct page` 结构体，里面有 `lru` 字段可以当双向链表节点。
空闲块本身不需要存任何管理信息——管理信息全在 `struct page` 数组里，这个数组
在启动时一次性分配好，不占用户能申请的空间。

```text
Linux 的做法：

物理内存:    [页0] [页1] [页2] [页3] ...
struct page: [描述符0] [描述符1] [描述符2] [描述符3] ...
              ↑ 链表指针在这里，不在页的数据区域里

用户分配到的空间 = 整个页，没有一个字节被链表指针占用
```

**CongCore 没有独立页描述符的后果**。CongCore 的内核堆按字节分配，没有独立
的页描述符数组。链表指针只能放在空闲块自己的头部。第一版用了 3 个 8-byte 字段
（prev + next + order = 24 bytes），加上对齐填充到 32 bytes 做最小块。这意味着
1-byte 分配实际占 32 bytes，造成严重的内部碎片（内部碎片：分配的块比请求大，
多出来的空间谁也用不了）。

**packed 编码的思路**。如果能把三个字段压进一个 8-byte word，最小块就能恢复到
8 bytes。关键观察：每个 shard 最大 64 MiB，按 8-byte slot 编号只需要 24 bit
索引（2^24 × 8 = 128 MiB > 64 MiB）；order 最多 60 多级，6 bit 够用。所以
prev、next、order 可以拼进 64 bit：

```text
一个 u64 里的布局：
┌──────────┬──────────┬────────┬──────────┐
│ prev(24b)│ next(24b)│order(6)│reserved(10)│
└──────────┴──────────┴────────┴──────────┘
```

这样空闲块头只占 8 bytes，1-byte 分配实际占 8 bytes（而不是 32），内部碎片从
96.9% 降到 87.5%——虽然仍有浪费，但 512 MiB 堆里能多出好几十 MiB 可用空间。

**外部碎片为什么还在**。即使最小块回到 8 bytes，buddy 仍然只能分配 2 的幂
大小的块。请求 201096 bytes 时要给 256 KiB（262144 bytes），浪费 30%。而且
反复分配释放后，大块被拆碎了可能拼不回来——总空闲量够，但找不到一块连续的
128 KiB，这就是外部碎片。彻底解决需要 slab：把常见的小 size 各自维护一个
free list，不走 buddy 的 2 的幂取整。

## 如何发现

32-byte 版本跑完整 BuildStorm 时触发 OOM panic：

```text
layout=Layout { size: 201096, align: 1 }
user=319614722 actual=464679392 total=536870912
```

同时 QEMU RSS 约 4.36 GiB，host `MemAvailable` 约 23 GiB，排除宿主压力。
OOM 磁盘状态冷续跑用 packed 内核一路完成，证明是节点过大造成提前耗尽。

```text
.tmp/final-runs/20260806-buildstorm-linux-buddy-full-105/
.tmp/final-runs/20260806-linux-packed-buddy-resume-108/
.tmp/final-runs/20260806-buildstorm-linux-packed-buddy-full-109/
```

```sh
# host：直接测试 allocator 数据结构
rustc --edition=2024 --test os/src/mm/buddy_heap.rs \
  -o /tmp/congcore-buddy-tests
/tmp/congcore-buddy-tests --nocapture
```

## 怎么解决

**把三个字段编码进一个 u64**。用相对 shard 起点的 one-based 8-byte slot index
表示前驱和后继，0 表示空。24 bit 索引覆盖接近 128 MiB，初始化时断言 shard
不超出编码容量。6 bit 存 order，剩余 bit 保留。

```text
bits  0..23: previous link (one-based slot)
bits 24..47: next link (one-based slot)
bits 48..53: buddy order
bits 54..63: reserved
```

**安全约束不变**：分配、释放前仍先查 free-head bitmap 确认地址属于 allocator
管理的空闲块，bitmap 和节点内 order 双重匹配后才 O(1) 摘链。metadata 不从全局
堆动态分配，不递归进入 allocator。

**和 Linux 的关系**。Linux 把链表字段放在预分配的 `struct page` 数组里，不占
用户请求空间，所以根本不存在"节点太大"的问题。CongCore 没有独立页描述符数组，
所以用相对索引压缩空闲块内元数据——这是过渡方案。长期应像 Linux 那样 buddy
只管页/大块，小对象进 slab。

## 对应提交

- 状态：待提交，packed buddy 当前位于未提交工作树；完整提交还应包含后续 slab
  分层，或拆成两个可独立验证的提交。
- 基线：顶层 `21332ba37bf1ba0efe8229e7f80eeffa3b99a239`；`os/`
  `b0185b3a4522c0ffc52599d73bd17b3d52320815`。
- 建议提交主题：`mm: pack buddy free-list links`。

## 对比提升

| 场景 | 结果 |
| --- | --- |
| 32-byte OOM 磁盘续跑 | packed 内核越过原 OOM 点，最终 `rc=0`；36 次探针中位数 361 ms、最大 888 ms |
| 官方 raw 全新冷启动 | 约 1836 秒再次 OOM（128 KiB 请求失败），说明 buddy 本身的 power-of-two 取整和外部碎片仍在 |

packed 修复了 32-byte 节点的额外浪费，但不能说容量问题已经彻底解决。纯 buddy 对
任意 layout 向上取整到 2 的幂以及大块外部碎片仍需 slab 分层解决。全新冷启动
的 OOM 也证明：只压缩链表指针不够，还需要减少 buddy 本身的 rounding 浪费。

以下是 AI 的具体分析，作为存档。

---

## 历史分析背景

这个问题跨越 allocator 表示层和分配策略两层：第一版 O(1) buddy 证明了算法方向
（释放从线性扫描变为常数时间），但把最小块扩大到 32 bytes 是副作用。packed 只
修复表示层，buddy 的 power-of-two rounding 和碎片需要后续 slab 分层才能真正
解决。下面保留完整的编码设计、OOM 证据、续跑数据和冷启动反证。


## 结论

上一轮用 O(1) free-head bitmap 和 intrusive doubly-linked list 消除了
`buddy_system_allocator 0.6` 释放时的线性扫描，但第一版把最小块从 8 bytes 提高到
32 bytes。全新 BuildStorm 在 guest uptime 约 907.9 秒真实耗尽 512 MiB kernel heap：

```text
layout=Layout { size: 201096, align: 1 }
user=319614722 actual=464679392 total=536870912
```

本轮不撤回已经由 perf 证明的 O(1) 合并机制，而是把 free-list metadata 压缩成一个
8-byte `u64`，恢复原分配器的 8-byte 最小粒度。相同 OOM 磁盘状态的续跑最终使
`buildstorm_testcode.sh` 返回 **rc=0**；36 次运行中探针中位数约 **361 ms**、最大
**888 ms**，没有再次 OOM 或超过 20 秒硬上限。但官方 immutable image 的后续全新
冷启动在约 1836 秒再次 OOM，说明 packed link 只消除了 32-byte 节点的额外浪费，
没有消除 buddy 对任意 layout 取整到 power-of-two 以及外部碎片。packed O(1) buddy
可继续作为 page/backing allocator，不能单独写成 BuildStorm 已通过。

## 为什么 32-byte 版本必须否决

第一版完整运行目录：

```text
.tmp/final-runs/20260806-buildstorm-linux-buddy-full-105/
kernel sha256: 44dae4b24165c298c07145a54ed64b01b024140635eeeb0049487d0c1fd4e8cf
official image sha256: 2ad9d955684297abe9db48d94f1b7fcc488268fc8f481408c55b1ec27f520c6a
user image sha256: 73b3f0006a2fc38af7b35bc8916aa25ea4f2e42f2b428959a38819109efe4d53
LoongArch64 / 12 harts / 8 GiB
```

它已经解决延迟退化：61--908 秒的浅探针均在 234--730 ms 返回，输出从 1261
增长到 9012 bytes，明显越过旧 allocator 在约 874 秒的 20 秒探针失败点。但随后
guest allocator 报告 `actual/user ~= 1.45` 并 panic。此时 QEMU RSS 约 4.36 GiB，
host `MemAvailable` 约 23 GiB、`SwapFree` 约 20 GiB，故不是宿主 OOM，也不是未释放的
QEMU 进程。OOM 后 qcow2 通过 `qemu-img check`。

32-byte free node 使所有 1--31 byte allocation 至少占 32 bytes，并让相同编译阶段
更早 OOM，因此必须否决。但 8-byte 冷启动仍有约 148.6 MiB `actual-user` 差额，证明
最小节点并非全部根因；buddy 的 power-of-two rounding 和无法满足 128-KiB 请求的
外部碎片仍需单独解决。增加 heap 或吞掉 allocation failure 会掩盖该问题。

## Linux 语义与 packed 表示

继续参考：

- `exampleOs/linux/mm/page_alloc.c::__free_one_page()`；
- `exampleOs/linux/mm/page_alloc.c::__del_page_from_free_list()`；
- `exampleOs/linux/include/linux/page-flags.h` 的 `PageBuddy`。

Linux 用 page metadata 保存 buddy/order/list 状态，因此既不扫描链表，也不会为了
链表指针扩大实际 page。CongCore 的 byte-granularity kernel heap 没有独立 `struct
page`，所以把等价状态放进 free block 自己的第一个 64-bit word：

```text
bits  0..23: previous free node link
bits 24..47: next free node link
bits 48..53: buddy order
bits 54..63: reserved
```

link 是相对 shard 起点的 one-based 8-byte slot index，0 表示 null。24 bits 可覆盖
接近 128 MiB，代码在 `init()` 中断言 shard 范围不超过编码容量；当前最大 shard 为
64 MiB。6-bit order 覆盖 64-bit target 的全部有效 order，并有 compile-time assert。

保留的不变量：

1. free-head bitmap 先证明地址上存在 allocator 所有的节点，才读取 free memory；
2. bitmap 与节点内 order 同时匹配后，才通过 packed prev/next O(1) 摘链；
3. 分配和归还仍在原有 shard、`SpinMutex` 和 `LocalIrqSaveGuard` 规则内；
4. allocator metadata 不从全局 heap 动态分配，不递归进入自身；
5. allocation failure、统计和 layout 均使用真实值，没有 BuildStorm 分支或固定返回。

`MIN_BLOCK_SIZE` 恢复到 8 bytes 后，512 MiB heap 的 free-head bitmap 约占 8 MiB。
与 32-byte 候选 ELF 相比，`.bss` 从 540066000 增至 546353384 bytes，增加
6287384 bytes；这是固定、可计算的约 6 MiB metadata 换取小分配粒度恢复，不会随
碎片数量增长。第一版省下约 6 MiB bitmap，却使相同编译阶段额外消耗约 23 MiB
actual 并提前 OOM；后续冷启动又证明剩余大部分 `actual-user` 差额来自 buddy 本身的
size rounding，不能全部归因于 32-byte 最小块。

代码：

- `os/src/mm/buddy_heap.rs`：packed node、relative links、bitmap 和直接单测；
- `os/src/mm/heap_allocator.rs`：按 8-byte slot 计算每个 shard 的静态 bitmap；
- `os/src/mm/mod.rs`：注册项目内 allocator；
- `os/Cargo.toml`：kernel 不再依赖旧 buddy crate，user allocator 不受影响。

## 静态检查与定向回归

allocator host tests 新增“1-byte layout 的 actual 必须为 8 bytes”，加上完整合并、
混合 layout、不对齐边界和 20,000 步 churn，共 **5/5** 通过：

```zsh
rustc --edition=2024 --test os/src/mm/buddy_heap.rs \
    -o /tmp/congcore-buddy-tests
/tmp/congcore-buddy-tests --nocapture
```

双架构检查通过：

```zsh
TMPDIR=$PWD/.tmp ARCH=riscv64 cargo check --manifest-path os/Cargo.toml \
    --target riscv64gc-unknown-none-elf
TMPDIR=$PWD/.tmp ARCH=loongarch64 cargo check --manifest-path os/Cargo.toml \
    --target loongarch64-unknown-none-softfloat
```

实际 LoongArch ELF：

```text
sha256: 846f76650c2e5f5708bb956c56f80007ffa3ff4f6fca7593267a5af5f75f35ae
Machine: LoongArch
entry: 0x80000000
```

LoongArch 12-hart / 8-GiB 回归：

- `.tmp/final-runs/20260806-linux-packed-buddy-vfs-106/`：12-worker stat、
  open/unlink lifetime、pathwalk errno、Unix VFS path，共 4/4 通过；
- `.tmp/final-runs/20260806-linux-packed-buddy-mmap-107/`：lazy mmap fault、private
  page cache、private madvise、shared cross-mm、shared truncate、exec teardown，
  共 6/6 通过。

`cargo fmt` 和 `git diff --check` 通过；构建只出现仓库既有 warning。

## 从 OOM 磁盘状态续跑

父盘是上述 32-byte 全跑 OOM 后留下的 qcow2，并已通过只读 `qemu-img check`。从它
创建新的 qcow2 child，父盘保持不写：

```text
.tmp/final-runs/20260806-linux-packed-buddy-resume-108/
image chain: resume.qcow2 -> failed full-run qcow2 -> immutable official raw
kernel sha256: 846f76650c2e5f5708bb956c56f80007ffa3ff4f6fca7593267a5af5f75f35ae
LoongArch64 / 12 harts / 8 GiB
workload: ./buildstorm_testcode.sh
probe interval / hard limit: 60 s / 20 s
stagnant limit: 8 probes
```

结果：

- 32-byte 内核曾在 907.9 秒 OOM；packed 内核 906.5 秒探针为 **178 ms**，无 OOM；
- 1222.5 秒生成 `target/debug/tg-xtask`，完成旧版本失败的第一阶段；
- 约 2189 秒后 `BUILDSTORM_RESUME_DONE rc=0`，随后 `sync` 和 QEMU 退出成功；
- 36 个探针：mean 397.4 ms、median 361 ms、p95 795 ms、max 888 ms；
- 最大连续无浅层 marker 变化 5 次，随后输出恢复；没有触发 8 次停止阈值；
- QEMU peak RSS 5941844 KiB，host 最低 `MemAvailable` 23129456 KiB，
  `SwapFree` 始终至少 20971516 KiB；
- 完成后的 child 再次通过 `qemu-img check`，无 qcow2 错误。

连续 5 次浅 marker 不变时按规则抓了 15 秒 `perf record`：4310 samples、lost 0。
该续跑启动时未传 `-perfmap`，所以报告只能看到宿主 TCG 的
`helper_lookup_tb_ptr`/atomic helpers，不能据此归因到 guest Rust 函数。本轮没有用
这份不完整符号信息制造新的“热点修复”；后续 guest 诊断必须按
`testsuits-final/AGENTS.md` 的 perfmap 章节启动 `QEMU_EXTRA_ARGS=-perfmap` 并保存准确
PID 对应的 map。正式无异常计时仍关闭 perfmap，避免扰动比较。

## 全新冷启动的反证

续跑成功后，从同一个官方 raw image 创建完全新的 qcow2 child，target 初始为空：

```text
.tmp/final-runs/20260806-buildstorm-linux-packed-buddy-full-109/
kernel sha256: 846f76650c2e5f5708bb956c56f80007ffa3ff4f6fca7593267a5af5f75f35ae
LoongArch64 / 12 harts / 8 GiB
```

运行一直保持响应，并在 909.5 秒以 750-ms 探针、9028-byte 输出越过 32-byte 候选的
OOM 点；之后继续编译到约 1836 秒。但 `tg-xtask` 最终链接期间再次真实 OOM：

```text
layout=Layout { size: 131072, align: 1 }
user=338967607 actual=487527504 total=536870912
frame_refs=603050
```

此时仍约有 49343408 bytes 未计入 live actual，却无法找到一个 128-KiB block，说明
除了约 148.6 MiB internal rounding 之外还有外部碎片。QEMU RSS 约 4.74 GiB，host
`MemAvailable` 约 24.4 GiB、`SwapFree` 约 20 GiB；退出后无残留进程，qcow2 再次通过
`qemu-img check`。所以续跑 rc=0 只能证明 packed 改动有益，不能证明容量问题的根因已经解决。

## 当前边界与下一步

packed link 修复了 32-byte 表示的额外浪费，并保留 O(1) 释放路径的响应性，但全新
冷启动证明纯 buddy 仍不适合作为所有 Rust 小对象的最终 allocator。下一轮先用固定
per-order counters 输出 live count、requested bytes 和 free blocks；依据真实分布选择
有限的 Linux slab/page 分层范围，而不是直接扩大 heap 或猜测 size classes。修复后
再从官方 raw 创建全新 overlay 完整运行。

本轮没有增加 heap 容量、没有忽略 OOM、没有伪造产物或 uptime，也没有为测试脚本
添加特判。结论来自精确 ELF、backing chain、serial、CSV、perf.data、资源日志和
真实 `rc=0` marker。
