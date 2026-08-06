# 8-6 Linux 风格 packed buddy links

## 问题概述

O(1) buddy 第一版需要 32-byte free node，使 1--31 byte 请求全部至少占 32 bytes；
BuildStorm 在约 908 秒耗尽 512 MiB kernel heap。改回较小节点后，纯 buddy 的
power-of-two 取整和大块外部碎片仍是剩余边界。

## 如何发现

OOM dump 给出 `user=319614722`、`actual=464679392`，结合 32-byte 最小块直接确认
表示层浪费；host 同时仍有约 23 GiB 可用内存，排除宿主 OOM。修复后从 OOM 磁盘状态
续跑成功，但官方 raw 冷启动约 1836 秒再次 OOM，进一步区分了“节点过大”和“纯 buddy
不适合小对象”两个问题。Linux 对照仍是 `PageBuddy`、order metadata 和 O(1)
`list_del()`。

```text
.tmp/final-runs/20260806-buildstorm-linux-buddy-full-105/
.tmp/final-runs/20260806-linux-packed-buddy-resume-108/
.tmp/final-runs/20260806-buildstorm-linux-packed-buddy-full-109/
```

```sh
# guest：从全新 overlay 或失败现场的只读 child 启动
./buildstorm_testcode.sh
# host：直接测试 allocator 数据结构
rustc --edition=2024 --test os/src/mm/buddy_heap.rs \
  -o /tmp/congcore-buddy-tests
/tmp/congcore-buddy-tests --nocapture
```

## 怎么解决

把 prev/next/order 压入一个 64-bit word，用相对 shard 的 one-based 8-byte slot 编码，
恢复 8-byte 最小粒度并保留位图验证和 O(1) 摘链。更好的最终方案是 buddy 管理页/大块，
小对象走有限 size-class slab；同时大块从共享 arena/页分配器获得，避免 12 个独立 shard
放大外部碎片。

编码把 one-based 8-byte slot 的前驱、后继和 order 压入一个 `u64`；0 表示空 link。
初始化时断言 shard 范围能由 24-bit 相对索引表示，读取 free memory 前仍必须通过
free-head bitmap 和 order 双重校验。
Linux 的链表字段位于固定 `struct page`，不占用户请求空间；本项目用相对索引压缩
空闲块内元数据，是缺少独立页描述数组时的过渡方案。它恢复 8-byte 粒度，却仍不能
替代 Linux 中“伙伴分配器管理页、slab 管理小对象”的分层。

## 对应提交

- 状态：待提交，packed buddy 当前仍位于未提交工作树；完整提交还应包含后续 slab
  分层，或拆成两个可独立验证的提交。
- 基线：顶层 `21332ba37bf1ba0efe8229e7f80eeffa3b99a239`；`os/`
  `b0185b3a4522c0ffc52599d73bd17b3d52320815`。
- 建议提交主题：`mm: pack buddy free-list links`。

## 对比提升

在 32-byte 版本的 OOM 磁盘状态上，packed 版本越过原 OOM 点并最终令脚本返回 rc=0；
36 次探针中位数 361 ms、最大 888 ms。但官方 raw 冷启动仍在约 1836 秒因 128 KiB
请求和碎片 OOM，所以只能证明 packed 表示修复有效，不能宣称完整问题闭环。

---

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
`qemu-img check`。所以续跑 rc=0 只能证明 packed 改动有益，不能证明容量根因已经闭环。

## 当前边界与下一步

packed link 修复了 32-byte 表示的额外浪费，并保留 O(1) 释放路径的响应性，但全新
冷启动证明纯 buddy 仍不适合作为所有 Rust 小对象的最终 allocator。下一轮先用固定
per-order counters 输出 live count、requested bytes 和 free blocks；依据真实分布选择
有限的 Linux slab/page 分层范围，而不是直接扩大 heap 或猜测 size classes。修复后
再从官方 raw 创建全新 overlay 完整运行。

本轮没有增加 heap 容量、没有忽略 OOM、没有伪造产物或 uptime，也没有为测试脚本
添加特判。结论来自精确 ELF、backing chain、serial、CSV、perf.data、资源日志和
真实 `rc=0` marker。
