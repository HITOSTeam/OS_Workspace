# 8-8 RISC-V 共享文件页复用 I-cache clean 状态

## 问题概述

多个 `rustc` 进程会通过 inode page cache（同一文件的共享物理页缓存）映射同一份
可执行文件和动态库。旧代码每发布一个 executable PTE 都调用一次
`mark_icache_stale()`。即使该物理页内容从未改变，每个进程仍重复执行 `fence.i`。

本批按照 Linux RISC-V 的 `PG_dcache_clean` 做法，在共享 `FrameOwner` 上记录普通
inode cache 页是否已经完成 I-cache 同步。内容变动时只清状态；下一次发布 executable
PTE 时才重新同步。新 PTE 发布后的本地 `sfence.vma` 保持不变。

## 背景知识

CPU 常把数据访问和取指分开缓存：

```text
文件读入物理页
      |
      +--> D-cache：普通 load/store 看到的字节
      |
      +--> I-cache：CPU 实际取来执行的指令
```

I-cache（Instruction Cache，指令缓存）不一定会自动看到通过数据访问写入的新字节。
RISC-V 用 `fence.i` 让当前 hart（硬件线程，可近似看作一个 CPU 核）重新取指。若同一
地址空间正在其他 hart 上运行，内核还要通知那些 hart；暂未运行该地址空间的 hart
可以记录为待刷新，在返回用户态前补做。

PTE（Page Table Entry，页表项）决定某个虚拟页是否存在以及能否执行。发布一个带 X
权限的新 PTE 前，必须先保证指令字节已经同步。发布后仍要执行本地 `sfence.vma`，让
当前 hart 丢掉可能缓存的 missing-PTE 结果。`fence.i` 管指令内容，`sfence.vma` 管地址
转换，两者不能互相替代。

普通文件的 page cache 页会被多个进程共享：

```text
rustc A 的虚拟页 ----+
                      +---- 同一个 FrameOwner / 物理页
rustc B 的虚拟页 ----+
```

如果文件页第一次映射后没有再写入，B 没有必要重复同步同一物理页。Linux 用 folio
（一页或一组连续页的缓存对象）上的 `PG_arch_1` 架构位记录这件事。在 RISC-V 下，该
位叫 `PG_dcache_clean`：

```text
页进入 page cache                 dirty
第一次发布 executable PTE         fence.i -> clean
其他进程发布同一物理页             clean hit，不重复 fence.i
write / truncate 尾页清零          clear clean -> dirty
下一次发布 executable PTE          fence.i -> clean
```

Linux 的关键顺序在 `__set_pte_at()`：先执行 `flush_icache_pte()`，再写 PTE。
`flush_icache_pte()` 只在 `PG_dcache_clean` 没有置位时刷新并置位。
`flush_dcache_folio()` 在页内容写入后只清位，不立即额外刷新。这样把开销推迟到真正
需要执行该页时，也不会让 executable PTE 先于 I-cache 同步对 CPU 可见。

CongCore 的 `FrameTracker` clone 共享一个 `FrameOwner`，因此 clean 状态放在 owner 上
就能随物理页一起被所有进程观察。匿名页、COW 页和 memfd/tmpfs 页不属于普通 inode
page cache。本批让它们保持 untracked，继续走原有的每地址空间同步路径，没有把 Linux
folio 规则套到生命周期不同的对象上。

## 如何发现

上一轮 960 秒诊断记录到 `icache_local_fences=3,158,847`，其中只有 14,591 次来自延迟
刷新；最后约 120 秒又增加 485,179 次。guest 最热路径是
`MemorySet::commit_lazy_fault()`，每个 executable missing-PTE 都无条件调用
`mark_icache_stale()`。代码审查确认普通文件页已经共享 `FrameTracker`，但 owner 上没有
对应 Linux `PG_dcache_clean` 的状态。

随后核对本地 `exampleOs/linux` commit
`4549871118cf616eecdd2d939f78e3b9e1dddc48`：

- `arch/riscv/mm/cacheflush.c::flush_icache_pte()` 负责 dirty 到 clean；
- `arch/riscv/include/asm/cacheflush.h::flush_dcache_folio()` 在写后清位；
- `arch/riscv/include/asm/pgtable.h::__set_pte_at()` 保证刷新先于 PTE 发布；
- `include/linux/page-flags.h` 保证 folio 初次进入 page cache 时 `PG_arch_1` 已清除。

## 怎么解决

**共享页状态。** RISC-V 的 `FrameOwner` 增加 `Untracked / Dirty / Clean` 三态。普通
inode cache 页完成磁盘填充后从 untracked 进入 dirty；其他 frame 不改变类别。

**PTE 发布顺序。** 所有 active executable PTE 发布入口统一经过
`with_executable_mapping()`。dirty 页先执行 `mark_icache_stale()`，再置 clean，最后写
PTE；clean 页直接写 PTE；untracked 页保留原有同步。

**写后清状态。** fd write 的 page-cache 镜像、内核向驻留共享映射复制、truncate EOF
尾部清零和 writable `UserBuffer` 都经 `with_bytes_mut()`。受跟踪页在写完后只回到 dirty。

**保留原有边界。** 新 PTE 的本地 `sfence.vma`、跨 hart I-cache 同步以及匿名/COW/
memfd 的保守路径都保留。没有新增全局 hart mask、跨 mm 广播、写后立即 fence 或两步
`mprotect`。

**诊断计数。** `DEBUG_PERF=true` 的诊断内核增加 `icache_clean_hits`、
`icache_clean_misses` 和 `icache_clean_bypasses`。正式内核恢复为 `DEBUG_PERF=false`。

## 对应提交

`os/` 基线是 `6c0752901c7fdf7075f616b6227bcad74f37fe7c`。实现随
`b18fffbd7439d22571cf530b21786fb08bce62e9`（`kernel: integrate final performance
improvements`）提交；顶层由 `6cb4b18f`（`final: integrate recent performance
rounds`）集成。该 `os/` 提交还包含同一阶段已经验证的 allocator、VM、文件系统和调度
修改，因此不能把整个提交的性能差异只归因于 I-cache clean 状态；本节的精确消融数据
仍是该机制的因果证据。

## 对比提升

重复执行 `rustc` 的精确同源 A/B 中，两边都发布 39,429 个 executable PTE：

| 指标增量 | clean 状态候选 | 强制 miss 消融基线 | 差异 |
| --- | ---: | ---: | ---: |
| `icache_clean_hits` | 34,796 | 0 | 34,796 次避免重复同步 |
| `icache_clean_misses` | 4,633 | 39,429 | -88.2% |
| `icache_local_fences` | 4,769 | 39,810 | **-88.0%** |

两次候选运行的 hit/miss 增量完全一致。微基准中位数按 candidate / baseline /
candidate-repeat 为 `446,215 / 486,807 / 540,749 us`。候选复跑比基线慢，因此不能声称
稳定的延迟提升。

BuildStorm 固定截止 gate 只显示进度持平：

| 截止 | 候选 deps | 基线 deps | 候选/基线 peak RSS | 候选/基线 CPU ticks |
| --- | ---: | ---: | ---: | ---: |
| 120 秒 | 37 | 37 | 2,109,160 / 2,193,748 KiB | 83,971 / 83,740 |
| 300 秒 | 74 | 73 | 2,613,580 / 2,625,644 KiB | 223,527 / 223,342 |

四轮均为 `rc=124`，`tg-xtask` 尚未生成，无 OOM、panic 或块设备停滞。`deps +1` 在这组
波动内，不足以证明 BuildStorm 提速。本批证明的是重复 I-cache 同步被消除，不是完整
BuildStorm 已完成。

production 内核上的 9/9 聚焦回归通过，包括跨进程 fd write、内核 copy、truncate、
memfd/mremap，以及 `riscv_icache_smp_smoke` 的 128 次更新乘 7 个远端 hart。正式 RISC-V
内核 SHA-256 为
`12a5a56cb0250ebfedd4729140e1889dae041cb4ca2716d4899e224fa64de745`。

以下是 AI 的具体分析，作为存档。

---

## 历史分析背景

这项修改同时跨过共享 page cache、文件写入、truncate、lazy fault、`mprotect`、
`mremap`、页表发布和跨 hart I-cache 同步。clean 位如果在内容仍可能变化时提前置位，
后续进程可能执行旧指令；如果清位入口漏掉，计数下降也不能说明实现正确。因此保留
Linux 对照、消融方法和完整回归现场。

## Linux 对照与实现映射

Linux 参考版本：

```text
exampleOs/linux
commit 4549871118cf616eecdd2d939f78e3b9e1dddc48
```

对应关系：

| Linux | CongCore |
| --- | --- |
| folio 上的 `PG_dcache_clean` | `FrameOwner::icache_state` |
| page cache 新 folio 的架构位为 0 | 文件页填充后 `Untracked -> Dirty` |
| `flush_icache_pte()` test/flush/set | `with_executable_mapping()` |
| `flush_dcache_folio()` clear bit | `with_bytes_mut()` 写后置 `Dirty` |
| `__set_pte_at()` flush then set PTE | sync、置 clean、publish 的同一 owner 锁区间 |

CongCore 没有完整 Linux folio lock。owner 上的短临界区把受控文件页写入与 executable
PTE 发布排开，避免 writer 在 flush 与 PTE store 之间留下错误的 clean 状态。锁顺序固定
为 `user_buffer_access -> icache_state`，反向获取被禁止。

`FrameIcacheState::Clean` 的消融版本只用于 A/B，它强制执行 miss 分支。测试结束后源码
已恢复为 `Clean => IcacheSyncOutcome::Hit`，正式构建不包含消融逻辑。

## A/B 资产与原始计数

诊断内核：

```text
candidate-debug-perf.elf
SHA-256 902485d5082dd7bf7235a60f5d257e2c1978be30fb016655d43a9e0b2e6c38ae

baseline-debug-perf.elf
SHA-256 675bb8c8177cd452a8fffbfbac8ffd9416873e4198775e08f632359a83458784
```

候选第一次运行：

```text
before: local_fences=377 hits=103 misses=370
after:  local_fences=5146 hits=34899 misses=5003
delta:  local_fences=4769 hits=34796 misses=4633
```

强制 miss 基线：

```text
before: local_fences=479 hits=0 misses=473
after:  local_fences=40289 hits=0 misses=39902
delta:  local_fences=39810 hits=0 misses=39429
```

候选复跑：

```text
before: local_fences=377 hits=103 misses=370
after:  local_fences=5137 hits=34899 misses=5003
delta:  local_fences=4760 hits=34796 misses=4633
```

local fence 还包含少量不属于这 39,429 次发布的同步，所以它与 miss 数不要求完全相等。
hit/miss 是直接对因计数；local fence 用于确认这些命中确实转化为更少的硬件屏障。

## 回归记录

production 回归目录：

```text
testsuits-final/.tmp/final-runs/
20260808-riscv-icache-clean-production-regressions-208/
```

通过项：

```text
file_mmap_lazy_fault_smoke
private_file_page_cache_smoke
private_file_madvise_dontneed_smoke
shared_file_alias_smoke
shared_file_cross_mm_smoke
shared_file_kernel_write_smoke
shared_file_truncate_cache_smoke
memfd_mremap_shared_smoke
riscv_icache_smp_smoke: 128 updates x 7 remote harts
```

诊断和消融日志：

```text
testsuits-final/.tmp/final-runs/20260808-riscv-icache-clean-perfdiag-204/
testsuits-final/.tmp/final-runs/20260808-riscv-icache-clean-regressions-205/
testsuits-final/.tmp/final-runs/20260808-riscv-icache-clean-ablation-206/
testsuits-final/.tmp/final-runs/20260808-riscv-icache-clean-perfdiag-repeat-207/

.tmp/ablate/20260808-icache-clean-tg-c120/
.tmp/ablate/20260808-icache-clean-tg-b120/
.tmp/ablate/20260808-icache-clean-tg-c300/
.tmp/ablate/20260808-icache-clean-tg-b300/
```

静态检查：

```sh
TMPDIR=$PWD/../.tmp ARCH=riscv64 cargo check \
  --manifest-path Cargo.toml --target riscv64gc-unknown-none-elf
TMPDIR=$PWD/../.tmp ARCH=loongarch64 cargo check \
  --manifest-path Cargo.toml --target loongarch64-unknown-none-softfloat
cargo fmt --all -- --check
git diff --check -- <本轮 os 文件>
```

双架构 check 和格式检查均通过，仅有仓库已有 warning。顶层全工作树的
`git diff --check` 仍会报告其他未提交文档的既有尾随空格；本批没有修改或清理那些文件。

## 适用边界

- 本批只缓存普通 inode page-cache frame 的 clean 状态。匿名/COW、memfd/tmpfs 继续
  bypass，不假设它们具有 Linux 普通文件 folio 的写入和失效规则。
- 内核控制的文件页写入会清状态。用户态直接改写可执行映射仍遵守 Linux/RISC-V 的
  显式 I-cache 同步要求，本批没有发明 page-fault 协议追踪每条用户 store。
- 960 秒诊断、120 秒和 300 秒 gate 都不是完整 BuildStorm。`tg-xtask` 尚未生成。
- 本批做了 LoongArch64 编译检查，但没有运行 LoongArch64 QEMU 回归。
- SATP interrupt guard 没有改动。此前同源 A/B 没有显示 BuildStorm 收益，且删除它会
  触及 hardirq 唤醒安全边界。
