# 8-6 Linux 式 signal frame 用户访问锁序修复

## 问题概述

信号投递持有 `TaskControlBlockInner` 自旋锁写入可能缺页的用户栈；signal frame 落在
COW/lazy 页时又需获取可睡眠 mm lock，与另一 hart 的 TLB shootdown、signal queue
形成稳定锁环，导致 BuildStorm minibuild 间歇停顿。

```text
Hart A: [TCB spinlock] -> [写用户栈] -> [page fault] -> [等待 mm lock]
            ^                                             |
            |                                             v
Hart B: [等待 TCB spinlock] <- [TLB shootdown/信号排队] <- [持有 mm lock]
```

## 背景知识

进程平时在用户态运行，寄存器里保存着程序计数器、栈指针和通用寄存器等现场。
当内核决定向进程投递 signal（信号）时，不能直接丢掉这份现场。
否则 signal handler（信号处理函数）结束后，进程不知道应从哪里继续执行。

内核会在用户栈上构造 signal frame（信号帧）。
这个 frame 通常包含保存的寄存器、signal info（信号信息）、signal mask（信号屏蔽字）
以及 handler 返回后进入内核的返回地址。
内核随后把用户态寄存器改成 handler 的入口和新栈指针，让 handler 开始执行。
handler 返回时，`sigreturn` 系统调用从 signal frame 读回现场，恢复原来的执行状态。

关键点是：signal frame 写在“用户内存”中，而不是内核自己的固定内存中。
用户内存使用虚拟地址，虚拟页是否已经有可写物理页，要由页表和 VMA 决定。
VMA（virtual memory area，虚拟内存区域）只说明一段地址合法以及它的访问权限。
它不保证这个地址此刻已经对应一个可写的物理页。

第一种常见情况是 lazy allocation（延迟分配）。
地址已经记录在 VMA 中，但程序第一次真正访问前，内核还没有分配物理页。
第二种情况是 COW（copy-on-write，写时复制）。
fork 后父子进程先共享只读物理页，第一次写入时才复制出当前进程自己的页面。
第三种情况是页面被 swap out（换出）到磁盘，需要重新调入内存。
所以，内核写用户栈本身也可能触发 page fault（缺页异常）。

缺页处理不是一次简单的内存写入。
它可能要分配物理页、复制 COW 页面、读回换出页，并修改页表和 VMA 状态。
这些共享结构需要 memory management lock（内存管理锁，简称 mm lock）保护。
Linux 中对应的核心锁通常称为 `mmap_lock`。
mm lock 是 sleeping lock（可睡眠锁，例如 mutex 或 rwsem），等待它的线程可以阻塞，
持有它的线程也可能被抢占；它不是只允许极短临界区的自旋锁。

现在看一个具体的死锁过程。
Hart A 为读取信号信息和修改任务状态，先取得 TCB 自旋锁。
它没有放锁就开始向用户栈写 signal frame。
目标栈页恰好是 COW 页，于是写操作触发 page fault，并进入 mm lock 的等待路径。
此时 Hart B 正在执行 `mmap` 或 `munmap`，已经持有 mm lock。
Hart B 修改页表后要做 TLB shootdown（跨核 TLB 失效），向 Hart A 发送 IPI（核间中断）。
Hart A 却仍拿着 TCB 自旋锁，并在 mm lock 路径上等待或自旋，无法及时响应这个 IPI。
另一条环是 Hart B 要向同一任务排队信号，因此还需要取得 TCB 自旋锁。
这样两个 hart 各自占有对方需要的锁，谁也无法继续。

```text
Hart A: TCB spinlock -> write user stack -> page fault -> wait mm lock
Hart B: mm lock -> TLB shootdown/signal queue -> wait TCB spinlock
```

这里必须遵守一条内核基本规则：持有 spinlock（自旋锁）时，绝不能执行可能睡眠、
阻塞或缺页的操作。自旋锁用于不可睡眠、持锁时间很短的临界区；用户指针访问则
可能随时进入缺页处理，因此两者不能重叠。

Linux 的 signal 投递明确划分了这条边界。
`get_signal()` 在锁内决定投递哪个信号并保存必要状态，然后释放 `sighand->siglock`；
之后才调用架构相关的 `setup_rt_frame()`，由它向用户内存写 signal frame。
也就是把“决定投递什么”和“构造用户 frame”拆成两个阶段。

更一般的原则叫 lock ordering（锁顺序）：所有路径必须按同一顺序取得多个锁。
如果路径一按 A -> B 加锁，而路径二按 B -> A 加锁，就存在形成环路的可能。
这里 signal delivery 形成 TCB -> mm，而其他路径形成 mm -> TCB
（或 mm -> 其他步骤 -> TCB），正是相反顺序。
修复的核心不是调整等待时间，而是彻底避免在用户写入期间持有 TCB 自旋锁。

## 如何发现

host 日志排除 CPU、内存、swap 和块设备耗尽；准确 QEMU PID 的 perf 显示三个固定
guest PC 各占约 9%，HMP vCPU 寄存器与静态回溯闭合了 TCB spinlock -> user fault ->
mm lock -> TLB/signal 的等待环。Linux 对照为 `kernel/signal.c::get_signal()`、
LoongArch `setup_rt_frame()`、`sys_sigaltstack()` 与 RISC-V `rt_sigreturn()` 的锁外
用户访问边界。

```text
testsuits-final/.tmp/final-runs/20260806-minibuild-stackdump-perf-10/
testsuits-final/.tmp/final-runs/20260806-minibuild-signal-unlock-perf-14/
testsuits-final/.tmp/final-runs/20260806-signal-frame-fault-regressions-17/
```

```sh
perf record -F 99 -g -p <qemu-pid> -o perf.data -- sleep 15
# guest
/user/signal_frame_fault_smoke.bin
```

## 怎么解决

signal delivery 改为三阶段：短锁内选择/快照，完全解锁后构造可能 fault 的用户 frame，
再加锁一次提交 context/mask/altstack；`rt_sigprocmask`、`sigaltstack` 和 RISC-V
`rt_sigreturn` 同样改为 copy-in/out 在锁外。长期应把“持原始自旋锁不得访问用户内存”
固化为可审计的通用规则，并继续清理同类 syscall。

`maybe_deliver_signal()` 现在生成不可变寄存器、mask 和 altstack 快照；frame helper
只接收快照并返回 frame 指针。只有用户写入成功后才重新取得任务锁，一次提交 saved
context 和新的 trap context，避免半完成状态对其他核心可见。
Linux `get_signal()` 在返回用户 handler 前释放 `sighand->siglock`，架构
`setup_rt_frame()` 随后才执行可能缺页的用户复制；CongCore 没有同样的 task/sighand
结构，因此用短 TCB 快照和二次提交实现相同的“自旋锁内不访问用户内存”边界。

## 对应提交

- 状态：待提交，当前实现仍位于未提交工作树。
- 基线：顶层 `21332ba37bf1ba0efe8229e7f80eeffa3b99a239`；`os/`
  `b0185b3a4522c0ffc52599d73bd17b3d52320815`。
- 建议提交主题：`signal: move user frame access outside task lock`。

## 对比提升

修复前连续 minibuild 在第 3 次后进入长等待；修复后 20/20 次完成，最终 guest uptime
205.54 秒。修复前三个各约 9% 的锁 PC 全部从 1% flat report 消失；64 轮真实 lazy/COW
altstack signal-frame 测试通过。完整 BuildStorm 仍需单独证明。

以下是 AI 的具体分析，作为存档。

---

## 历史分析背景

以下编号 1–9 保留了当时从现场诊断、Linux 语义对照到聚焦回归的完整推理链。
这份分析作为证据档案保留，方便后续贡献者复核测试环境、性能数据和修复边界。

## 1. 结论

BuildStorm 前置 minibuild 的间歇停顿不是宿主机 CPU、内存、swap 或块设备资源耗尽，
而是信号投递把当前任务的 `TaskControlBlockInner` 自旋锁带进了可缺页的用户栈写入。
signal frame 落在 COW/lazy 页时，缺页路径需要进入可睡眠的 `MmRef` 内存锁；同时其他
hart 可能正在释放该内存锁、等待 TLB shootdown 或向同一任务排队信号，形成稳定的
自旋锁/内存锁/TLB 等待环。

本次参考本地 Linux `kernel/signal.c`、LoongArch 与 RISC-V 的架构 signal 实现，建立
统一规则：

- signal handler 的选择、寄存器和 mask 快照只在短 TCB 临界区内完成；
- 完全释放 TCB 自旋锁后，才构造 signal frame、处理 COW/lazy fault；
- frame 成功后重新加锁，一次提交 saved context、mask、altstack 状态和 trap context；
- `rt_sigprocmask`、`sigaltstack` 同样在锁外执行用户指针复制；
- RISC-V `rt_sigreturn` 在锁外读取用户 ucontext，只在锁内提交恢复后的寄存器和 mask。

这不是 BuildStorm 测试名特判，也没有跳过 signal frame 或缺页。新增回归真实使用
`SA_ONSTACK | SA_SIGINFO`，在 64 个 fork 子进程中把 frame 写到从未触碰的匿名
altstack，强制经过 lazy/COW fault；全部通过。

相同 LoongArch64、12 vCPU、8 GiB、snapshot、无 `-perfmap` 的连续 clean minibuild
测试中，修复前在第 3 次完成后进入长时间锁等待并被主动停止；修复后 **20/20** 次
构建完成，最终 guest uptime 为 205.54 秒。修复前 perf 的三个固定 guest PC 各占
9.04%～9.22%；修复后它们全部从 1% flat report 中消失，采样转为正常、分散的 QEMU
TCG/TLB 工作。

本文件先记录聚焦 A/B 和回归。完整 BuildStorm 的结果会在受监控正式运行完成后补入；
在出现正式成功标记和本地 judge 结果以前，不宣称 BuildStorm 已通过。

## 2. 版本与环境

| 资产 | 值 |
| --- | --- |
| 顶层分支 / 基线 | `dev_final` / `21332ba37bf1ba0efe8229e7f80eeffa3b99a239` |
| `os/` 基线 | `b0185b3a4522c0ffc52599d73bd17b3d52320815` |
| final test source | `final-2026` / `b5ec6ef8497e1818cbdec3b54bb722f036e57972` |
| 本地 Linux 参考树 | `exampleOs/linux` / `4549871118cf616eecdd2d939f78e3b9e1dddc48` |
| QEMU / perf | 11.0.3 / 7.1.6 |
| 聚焦架构 | LoongArch64，12 vCPU，8 GiB |
| 镜像模式 | `-snapshot` |
| LoongArch 镜像 SHA-256 | `2ad9d955684297abe9db48d94f1b7fcc488268fc8f481408c55b1ec27f520c6a` |

所有 guest 写入由 QEMU snapshot 丢弃，没有修改 14 GiB 基准镜像。诊断运行每 2 秒
记录 QEMU RSS、线程数、CPU ticks、I/O，以及 host `MemAvailable`/`SwapFree`；guest
探针有 10 秒硬上限。卡住时驱动停止 QEMU，没有无限等待。

## 3. 现场与 perf 证据

### 3.1 日志能排除什么

停顿现场中 QEMU 仍消耗约三个宿主核，RSS 低于 1.9 GiB，host `MemAvailable` 最低约
24.77 GiB，swap 仍有约 19.47 GiB，块 I/O 没有持续增长。因此不是宿主资源不足，也
不是镜像读写带宽被耗尽。

日志本身不能回答 CPU 是在做有效编译、QEMU 地址翻译还是内核锁自旋，所以继续对
准确 QEMU PID 执行：

```zsh
perf record -F 99 -g -p <qemu-pid> -o perf.data -- sleep 15
perf report --stdio --no-children --percent-limit 1.0 \
    --sort symbol -i perf.data
```

本轮正常运行没有添加 `-perfmap`，避免它显著放大 TCG 开销。之前验证过的
`QEMU_EXTRA_ARGS=-perfmap` 仍是需要 guest JIT 符号名时的首选诊断参数；它只用于定位，
不用于正式 BuildStorm 计时。

### 3.2 修复前锁现场

证据目录：

```text
testsuits-final/.tmp/final-runs/
  20260806-minibuild-stackdump-perf-10/
    serial.log
    host-metrics.log
    perf.data
```

perf 共 16K samples、lost 0。flat report 的异常固定 PC 为：

| overhead | guest PC / 调用位置 | 现场含义 |
| ---: | --- | --- |
| 9.22% | `0x80108628`，`KernelMutex<MemorySet>::lock` 返回点 | signal frame 缺页等待 mm |
| 9.17% | `0x8022358c`，`MmRef::resolve_cow_fault` 返回点 | COW fault 路径 |
| 9.04% | `0x8029b900`，`tgkill`/signal queue 返回点 | 另一 hart 等待同一 TCB |
| 4% 左右 | `0x801a3c14` 附近，`service_pending_tlb_shootdowns` | mm 解锁侧等待远端 TLB |

HMP 暂停全部 vCPU 后读取寄存器和 `$r3`（LoongArch SP），得到的并发状态为：

```text
hart A: queue_signal_to_task -> 等待目标 TaskControlBlockInner
hart B: try_resolve_user_page -> MmRef::resolve_cow_fault -> 等待 MemorySet mutex
hart C: MemorySet guard drop / wait queue -> 等待仍由 hart A/B 关联的锁
hart D: service_pending_tlb_shootdowns
其余: idle
```

静态回溯最终落到 `maybe_deliver_signal()`：它持有 `TaskControlBlockInner` 后调用
`setup_loongarch_rt_signal_frame()`，后者通过 `write_user_value()` 写用户栈。用户页若是
COW/lazy，`write_user_value()` 必须进入 `try_resolve_user_page()` 和 mm 可睡眠锁，正好
闭合现场锁环。

### 3.3 修复后 perf

证据目录：

```text
testsuits-final/.tmp/final-runs/
  20260806-minibuild-signal-unlock-perf-14/
    serial.log
    host-metrics.log
    probe-latency.csv
    perf.data
```

perf 共 45K samples、lost 0。1% flat report 中主要条目变为：

| 符号 | overhead |
| --- | ---: |
| QEMU `helper_lookup_tb_ptr` | 11.68% |
| QEMU `tlb_set_page_full` | 2.48% |
| QEMU `cpu_atomic_fetch_addq_le_mmu` | 2.33% |
| QEMU `cpu_atomic_xchgq_le_mmu` | 1.88% |
| QEMU `tcg_gen_code` | 1.20% |

修复前三个各约 9% 的固定锁 PC 均消失。这说明成本没有换一个函数名继续集中自旋，
热点已经恢复为长时间真实构建应有的 TCG、TLB 和原子访存工作。

## 4. Linux 参考语义

| Linux 位置 | Linux 边界 | 本次实现 |
| --- | --- | --- |
| `kernel/signal.c:get_signal()`（约 2802～3041） | 返回用户 handler 前释放 `sighand->siglock` | 选择 signal/快照状态后释放 TCB 锁 |
| `arch/loongarch/kernel/signal.c:setup_rt_frame()`（约 935～954） | 解锁后执行 `copy_siginfo_to_user`、`__copy_to_user` | 锁外写 LoongArch rt frame 与 FP extcontext |
| `kernel/signal.c:sys_sigaltstack()`（约 4448） | `copy_from_user` → `do_sigaltstack` → `copy_to_user` | new stack 锁外读取，短锁提交，old stack 锁外写回 |
| `arch/riscv/kernel/signal.c:rt_sigreturn()`（约 310～331） | 从用户 frame 复制 mask/context 后恢复寄存器 | 锁外读 ucontext，锁内一次提交 |

Linux 的 `sighand->siglock`、`task_struct` 与本内核 TCB 结构不同，不能机械复制内部类型；
真正需要保持的是锁边界：raw spinlock 临界区中不发生可能 fault/sleep 的用户访问。

## 5. 实现

### 5.1 signal delivery 三阶段

`maybe_deliver_signal()` 现在按三个阶段运行：

1. TCB 锁内快照 trap context、旧 mask、altstack、FP 状态并计算新 mask；
2. 完全解锁后，通过现有通用用户缺页/COW 机制构造 signal frame；
3. frame 成功后重新加锁，提交 saved context、mask、altstack 与 handler 寄存器。

LoongArch helper 不再接收可变 `TaskControlBlockInner`，只接收不可变快照，并返回
`(frame_ptr, ucontext_ptr)`。RISC-V 同样先写 frame、再提交 TCB，避免只修比赛所用架构。

### 5.2 同类 syscall 锁序审计

`rt_sigprocmask` 原先持 TCB 锁读取 `set`、写回 `oldset`；`sigaltstack` 也在 TCB 锁内
读写两个用户结构。两者都改为 Linux 的 copy-in / short commit / copy-out 顺序，同时
保留 `set == oldset` 的 aliasing 语义。

RISC-V `rt_sigreturn` 原先从 `sig_saved_ctx` pop 后继续持锁读取用户 ucontext。现在先在
短临界区取得 kernel snapshot 与 frame 地址，解锁读取 frame，再加锁提交恢复结果。
LoongArch 路径此前已经在锁外读取 authoritative frame，本次保持该语义。

## 6. 单一证明测试

新增：

```text
user/src/bin/smoke_archive/signal_frame_fault_smoke.rs
```

每次迭代执行：

1. parent 创建 64 KiB `MAP_PRIVATE | MAP_ANONYMOUS` 区域，但不访问任何页；
2. fork child，child 把该区域注册为 altstack；
3. child 向自身发送 `SIGUSR1`，action 使用 `SA_ONSTACK | SA_SIGINFO`；
4. 内核必须在未触碰的 lazy/COW altstack 上写 rt frame 和 ucontext；
5. handler 正常返回到内核提供的 sigreturn trampoline；
6. parent 验证 child 退出状态，重复 64 次。

结果：

```text
SIGNAL_FRAME_FAULT_PASS iterations=64
```

包含该测试的完整聚焦组：

| 测试 | host elapsed | 结果 |
| --- | ---: | --- |
| `signal_frame_fault_smoke` | 136 ms | PASS，64 iterations |
| `socketpair_exit_eof_smoke` | 164 ms | PASS |
| `concurrent_spawn_wait_smoke` | 264 ms | PASS，256 children |
| `wait_wakeup_race_smoke` | 369 ms | PASS，256 iterations |
| `fork_thread_group_perf_smoke` | 152 ms | PASS，guest 116400 μs |
| `tlb_shootdown_smp_smoke` | 52 ms | PASS |

日志：

```text
testsuits-final/.tmp/final-runs/
  20260806-signal-frame-fault-regressions-17/
```

测试结束后 QEMU 正常退出，没有残留实例。

## 7. 连续构建 A/B

相同 driver 连续执行 20 次：

```sh
rm -rf target
timeout 900 cargo build
```

每 15 秒执行一个有 10 秒硬上限的 guest `/proc/perf`、进程和进度探针。

| 指标 | 修复前 | 修复后 |
| --- | ---: | ---: |
| 完成的 clean builds | 3/20 后锁停顿并主动终止 | **20/20** |
| 最终 guest uptime | 未完成 | **205.54 s** |
| 完成的响应探针 | 2 | 13（12 个计时探针） |
| 探针中位数 | 808.5 ms（停顿前，仅供诊断） | 644 ms |
| perf lost samples | 0 | 0 |
| QEMU peak RSS | 1,891,688 KiB | 2,052,392 KiB（运行更久） |
| host 最低 MemAvailable | 24,772,668 KiB | 24,543,656 KiB |
| host 最低 SwapFree | 19,465,172 KiB | 19,466,796 KiB |

修复后的 guest 最终块请求 submitted/completed 相等、inflight 为 0，ext4 cache hit 为
98%。RSS 的小幅增加来自测试从第 3 次推进到第 20 次，host 仍有约 24.5 GiB 可用内存，
不能把它解释成资源泄漏。

## 8. 静态验证

```zsh
TMPDIR=$PWD/.tmp ARCH=riscv64 cargo check --manifest-path os/Cargo.toml \
    --target riscv64gc-unknown-none-elf
TMPDIR=$PWD/.tmp ARCH=loongarch64 cargo check --manifest-path os/Cargo.toml \
    --target loongarch64-unknown-none-softfloat
```

两项均成功；输出只有仓库既有 warning。`DEBUG_PERF` 与 `DEBUG_WATCHDOG` 在正式构建前
均恢复为 `false`。

## 9. AI 使用与复现

AI 用于组织只读诊断、对照本地 Linux 源码、生成锁序修复和测试草案；所有结论都由
真实 guest 日志、HMP 寄存器现场、perf samples、编译结果和回归退出码验证。没有修改
judge、计时、`/proc/uptime` 或基准镜像，也没有伪造完成标记。

复现顺序：

1. 使用 snapshot、LoongArch64 12 vCPU/8 GiB 启动 final 镜像；
2. 连续 clean build，并以有硬上限的探针监控进度；
3. 卡住时立即 `perf record -p <qemu-pid>`，随后暂停 vCPU 读取 HMP registers/stack；
4. 使用本地 Linux `get_signal()` 与各架构 `setup_rt_frame`/`rt_sigreturn` 核对锁边界；
5. 应用修复后重复相同 20-build 场景；
6. 运行 `signal_frame_fault_smoke` 与五项并发/MM 回归；
7. 双架构 `cargo check`；
8. 最后才运行无诊断扰动、受监控且有硬截止的正式 BuildStorm。

建议内核提交：

```text
signal: drop task lock around user frame access
```

建议顶层测试提交：

```text
test(signal): cover faultable alternate frames
```
