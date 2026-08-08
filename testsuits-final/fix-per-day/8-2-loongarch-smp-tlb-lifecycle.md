# 8-2 LoongArch 12 核 TLB 与进程生命周期修复

## 问题概述

LoongArch 12 核上线后，用户程序遇到四类不同的故障：

```text
用户地址异常（TLBEHI 和 BADV 落在同一个 8 KiB 页对）
    → TLB 跨核失效不完整
重复 reap 警告（pid X not found, already reaped?）
    → 进程退出和 wait4 之间有竞态
rustc helper thread EAGAIN
    → 内存过量承诺策略把累计虚拟 commit 当成硬上限
CAgent 间歇 reject
    → 网络命名空间过早清理
```

不是同一个 bug，但都是多核暴露出的并发问题。

## 背景知识

这一节给只上过操作系统课的读者铺路。

**LoongArch 的两块 TLB 和刷新规则**。课上讲的 TLB 一般只有一块，LoongArch 分成
两块，查找时并行查：

```text
 虚拟地址
    │
    ├──────────────────────────────────────────┐
    ▼                                          ▼
┌──────────┐                           ┌─────────────┐
│   MTLB   │  全相联，每项自带页大小   │    STLB     │  组相联，固定 4 KiB
│  ~48 项  │  可混装 4K/2M/1G 映射     │  数百~千项  │  大容量、查找快
└────┬─────┘                           └──────┬──────┘
     │          命中任一即可                   │
     └─────────────────┬──────────────────────┘
                       ▼
                 物理页号 + 属性
```

一个 TLB 表项存偶/奇两个页（TLBELO0 和 TLBELO1），4 KiB 页配置下一个表项覆盖
8 KiB。所以"刷一页"的最小硬件粒度是 8 KiB 的页对。

**invtlb 指令——选择刷新粒度**。`invtlb op, asid, addr` 的 `op` 决定匹配规则：

| op | 作用 | 类比 |
|---|---|---|
| 0x5 | 按 ASID + 地址刷 | "只把这一个条目丢掉" |
| 0x4 | 按 ASID 刷所有非全局项 | "把这个进程的缓存全清" |
| 0x3 | 清本核所有非全局项 | "所有用户缓存一起丢" |
| 0x1 | 清本核全部 TLB（含全局） | "内核映射也重新来" |

刷新时两块 TLB 同时受影响。用错 op 会把别的进程的条目也刷掉（比如本想按地址刷
却用了按 ASID 刷的 0x4）。

**ASID（地址空间标识符）**。每个 TLB 项带一个 10 位 ASID 标签。不同进程用不同
ASID，这样切换进程不需要全刷 TLB。LoongArch 的 ASID 是 per-hart 分配的——同一个
进程在不同核上的 ASID 编号可以不同。ASID 0 保留给内核。

**PLV（特权级）**。LoongArch 有 4 级特权（0 最高，3 最低），内核在 PLV 0，用户在
PLV 3。PTE 的 PLV 字段决定哪个特权级能访问该页。

**CSR.ASID**。当 CPU 进入内核时，trampoline 会把 `CSR.ASID` 切成 0（内核 ASID）。
这意味着一旦进入内核态，TLB 里用户 ASID 的条目虽然物理上还在，但匹配不上了——
硬件比较 ASID 时发现不等，直接跳过。这是 LoongArch 与 RISC-V 的一个关键差异：
RISC-V 内核态不切 SATP，用户页表仍然活着。

**DMW（直接映射窗口）**。内核用 `CSR.DMW0..3` 配置一段虚拟地址直接映射物理内存，
不经过页表也不占 TLB。所以内核的线性映射零 TLB 开销。

**tlbsrch/tlbrd/tlbwr/tlbfill 四条指令分工**：
- `tlbsrch`：用 TLBEHI 中的虚拟页号和 ASID 去 TLB 里找，找到就把索引写入
  `CSR.TLBIDX`；
- `tlbrd`：读出 `TLBIDX` 指向的那一项到 TLBEHI/TLBELO0/TLBELO1；
- `tlbwr`：把 CSR 里的内容写入 `TLBIDX` 指向的位置（覆盖）；
- `tlbfill`：把 CSR 里的内容填入 TLB，**硬件自己选位置**（不会冲掉你指定的项）。

TLB refill 异常处理用的是 `tlbfill`，因为是硬件替换策略。如果错用 `tlbwr` 可能
覆盖掉别的有效项。

**跨核 TLB shootdown**。一个核修改了页表，别的核的 TLB 里可能还有旧条目。必须
通知它们刷新，否则会访问到旧物理页甚至已被释放的页。LoongArch 没有像 RISC-V
SBI RFENCE 那样的固件调用，必须软件自己发 IPI、等确认。流程：

```text
核 0 修改 PTE
    → dbar 0 保证写入全局可见
    → 填 IPI 请求（目标核、ASID、地址范围）
    → 发送 IPI
    → 目标核执行 invtlb 并回写 ack
    → 核 0 看到所有 ack 后才释放旧物理页
```

**进程退出与 wait4 的竞态**。课上讲的是：进程退出变成 zombie，父进程 wait 取走
状态。但如果"发布 zombie 状态"和"放入父进程队列"不是原子的，wait 可能在中间
取走了半成品，然后退出路径又把同一个进程再次入队。

**overcommit（内存过量承诺）**。Linux 默认允许进程申请超过物理内存的虚拟地址空间
（因为大多数页不会真正用到）。`mode 0` 只拒绝"单次申请量 > 物理内存"的极端
情况；`mode 2` 才做严格的累计限制。旧代码错误地在 mode 0 下也比较累计虚拟 commit
与物理内存的 1.5 倍，导致多进程 fork 后很快触及上限。

## 如何发现

关键失败日志：

```text
.tmp/final-runs/20260802-113053-loongarch64-shell/serial.log
.tmp/final-runs/20260802-162552-loongarch64-cagent/serial.log
.tmp/final-runs/20260802-203851-loongarch64-shell/serial.log
.tmp/final-runs/20260802-174331-loongarch64-cagent/serial.log
.tmp/final-runs/20260802-174331-loongarch64-cagent/score.json
.tmp/final-runs/20260802-223007-loongarch64-shell/serial.log
```

运行命令：

```sh
ARCH=loongarch64 MEM=8G SMP=12 IMAGE_MODE=snapshot ./run.sh cagent
cd /work/tgoskits
export RUSTUP_TOOLCHAIN=nightly-2026-05-28 CARGO_NET_OFFLINE=true
cargo build -p tg-xtask
```

第一份日志中 `BADV` 和 `TLBEHI` 落在同一个 8 KiB 页对，说明跨核 TLB 发布不完整。
CAgent 日志出现重复 reap 警告。`tg-xtask` 在 `futures-core` 阶段报告
`pthread_create(EAGAIN)`，PID 只到约 50、无物理页分配失败，但 `Committed_AS`
已超过错误的 `1.5 * RAM` 门槛。

## 怎么解决

四个子问题各修一处：

**TLB shootdown 完善**：页表项写入后，按活动核心掩码发送失效请求；4 KiB 页按
8 KiB 偶/奇 pair 对齐；接收核执行指定 ASID 的 `invtlb 0x5`，回写完成序号；
发送方全部确认后才释放旧物理页。trap-context 的 supervisor-only 映射也纳入同一
ASID batch，不再以 U bit 判断是否需要 shootdown。

**进程退出改为三步**：

```text
最后存活线程执行资源 teardown
    → 原子发布 EXIT_ZOMBIE（exit code + 父队列 + waiter 唤醒 一次完成）
    → waiter 以一次性 claim 做 EXIT_ZOMBIE → EXIT_DEAD
    → 统计、PID 和 PCB 只释放一次
```

**网络命名空间**：进程、socket 和 namespace fd 都增加同一个统一引用表的引用；
四类引用同时为零时才能原子 claim teardown，不会出现跨类别的 torn snapshot。

**overcommit 修复**：mode 0 只拒绝单次大于物理内存的申请，不比较累计 commit；
三种策略抽成可单测的纯函数。

Linux 对照为 LoongArch `tlb.c/smp.c`、`kernel/exit.c`、`net/core/net_namespace.c`
和 `mm/util.c`。

## 对应提交

- `os/`：`e699326f17e23e617a2eddbe5ed8103e572c4a3e`
  `fix(loongarch): harden SMP execution and teardown`。
- `os/`：`90625864bb7d6c9de62a9b96538bc0e84a3c078e`
  `mm: fix default overcommit heuristic`。
- 顶层构建/指针：`2ce9a824`、`afef9ef5`、`af86d2af`。
- 报告提交：`5b9c454de00ec5f304dbc93f2b439f7153221192`，后续验证补充
  `63352ce73ce7499b5747da4b7e592c47fb44814d`。

## 对比提升

| 指标 | 结果 |
| --- | --- |
| 核心上线 | 12/12，online mask `0xfff` |
| CAgent | 10/10，权重 199.10/200 |
| kernel agent 连续回归 | 20/20 pass |
| 工具链连续启动 | 12 轮完成 |
| 最小离线 Cargo 工程 | 编译 + 运行 Hello world 成功，约 1m33s |
| `tg-xtask` | 越过原 EAGAIN 故障点，继续到 PID 40；60 分钟未完成 |

该批没有运行完整 BuildStorm，`tg-xtask` 聚焦构建确认原 blocker 消失但整体未完成。

以下是 AI 的具体分析，作为存档。

---

## 历史分析背景

这个批次把四个互相独立的多核正确性问题集中修复：TLB shootdown 不完整、进程退出
竞态、netns 过早清理和 overcommit 误判。它们共同导致 12 核 LoongArch 无法稳定
运行工具链和 CAgent。下面保留完整的失败现场、Linux 对照、设计细节和验证数据。

## 1. 结论

本批次在独立工作树 `loongarch-linux-fix` 中，以 Linux 的 LoongArch TLB 语义、
进程退出状态机和对象引用生命周期为参考，修复了 12 核启动、跨 hart TLB
shootdown、trap-context 映射发布、进程退出/回收以及 network namespace 清理中的
一组并发正确性问题。

当前可以确认：

- LoongArch 12/12 hart 能稳定上线，online mask 为 `0xfff`，failed mask 为 `0x0`；
- guest 中 `rustc`/`cargo` 连续 12 轮探针均完成，没有再卡在首次工具链启动；
- 离线最小 Cargo 工程成功编译并运行 `Hello, world!`；
- 单个 CAgent kernel agent 连续 20 轮为 20 pass / 0 fail；
- 最终标准 CAgent 为 10/10，judge 权重合计 `199.10/200`；
- 修复默认 overcommit 策略后，`tg-xtask` 聚焦构建越过了此前
  `futures-core` 的 `pthread_create(EAGAIN)` 故障点，并继续创建 PID 40 之后的
  rustc 子进程；
- 最终串口日志中没有 panic、fatal trap 或重复 PID reap 警告；
- LoongArch 与 RISC-V 两个 target 的 `cargo check` 均通过，`git diff --check`
  通过。

本批次按用户要求**没有运行完整 BuildStorm**。只在 `/work/tgoskits` 中聚焦执行
`cargo build -p tg-xtask`，没有继续执行 arceos 主工作区编译。该聚焦构建运行
60 分钟后仍未返回，按验证上限主动结束；没有把越过原故障点写成 `tg-xtask`
完整通过。因此当前结论只能是：

> LoongArch 已经通过 BuildStorm 之前的关键短链路门槛，但尚无证据证明完整
> BuildStorm 可以通过，更没有完整编译耗时或性能分数据。

## 2. 工作范围与资产

### 2.1 独立工作树

```text
工作树：.tmp/worktrees/loongarch-linux-fix
分支：  loongarch-linux-fix
```

开始本批次时的基线：

| 资产 | 版本 |
| --- | --- |
| 顶层源码 | `b766dae83e2a96af6b15cca10409d25948d95818` |
| `os/` | `d21154e86fb50798f8413b4abd37e2835d2168b3` |
| final test source | `1eac61d3becaa592c8ef12a7535f0ec6bb9e3e36` |
| 本地 Linux 参考树 | `fc02acf6ac0ccde0c805c2daa9148683cdd01ba8` |

本批修复已在独立分支形成提交。内核实现 commit 为
`e699326f17e23e617a2eddbe5ed8103e572c4a3e`，内核验证文档 commit 为
`1e6c19fb32e9fa81cc2a757b090a4973a34118b7`，BuildStorm 前置 overcommit
修复 commit 为 `90625864bb7d6c9de62a9b96538bc0e84a3c078e`。顶层仓库通过独立提交固定
soft-float 构建目标并更新 `os/` 子模块指针；上表保留的是进入本批次前的基线，
不能把它误写成修复后的版本。

### 2.2 决赛资产

- LoongArch 镜像：`sdcard-la-pub.img`；
- SHA-256：
  `2ad9d955684297abe9db48d94f1b7fcc488268fc8f481408c55b1ec27f520c6a`；
- 文件系统：14 GiB raw ext4，卷标 `starry-rootfs`；
- QEMU：11.0.3；
- 运行配置：LoongArch64、12 vCPU、8 GiB、`IMAGE_MODE=snapshot`；
- guest Rust：
  `rustc 1.98.0-nightly (57d06900f 2026-05-27)`，
  `cargo 1.98.0-nightly (fbb61be30 2026-05-26)`。

所有运行均使用 QEMU snapshot，没有直接写入 14 GiB 基准镜像。临时
`testsuits-for-oskernel` 接线在验证结束后已删除，`user/.cargo/config.toml` 已恢复
为原来的 RISC-V target。

本批早期只读检查曾记录 remote `final-2026` 为
`b5ec6ef8497e1818cbdec3b54bb722f036e57972`，比本地测试源码新；按项目规则没有自动
拉取或替换基线。报告收尾时再次查询远端因 DNS 解析失败而没有得到新结果，正式
成绩采集前必须重新核对远端 HEAD。

## 3. 初始失败现场

### 3.1 12 核上线不等于用户态可用

早期版本已经能打印：

```text
[kernel] loongarch64 SMP online mask 0xfff (12 harts), failed=0x0
```

但首次执行工具链仍可能立即陷入内核 trap。日志
`.tmp/final-runs/20260802-113053-loongarch64-shell/serial.log` 中，
`rustc --version` 触发：

```text
PANIC: panicked at os/src/arch/loongarch64/trap/handler.rs:168:5:
Unhandled kernel trap: ecode=0 badv=0x40289000
badi=0x29c000ac era=0x80074980
```

另一次运行中，`rustc` 没有退出，watchdog 显示任务仍在 hart 0 运行，QEMU CSR
现场为：

```text
BADV    0x40377000
TLBEHI  0x40376000
ASID    0x000a0000
TLBELO0 0
TLBELO1 0
```

这说明问题不是“二级 hart 没启动”，而是用户地址空间第一次进入较复杂动态
glibc/Rust 程序时，LoongArch 的 paired TLB、ASID 和 trap-context 映射发布没有
让各个 hart 看到一致的结果。

### 3.2 重复 reap 警告

早期 CAgent 日志
`.tmp/final-runs/20260802-162552-loongarch64-cagent/serial.log` 出现：

```text
remove_from_pid2process: pid 5 not found (already reaped?)
remove_from_pid2process: pid 22 not found (already reaped?)
```

这不是无害的日志噪声。旧退出路径先发布 `is_zombie`，再把 child 放入父进程
exited queue；`wait4()` 可能在发布尚未完成时消费并删除 PID，随后 exit 路径又把
同一个 PCB 入队。结果可能包括：

- 同一 PID 被 wait 两次；
- `RUSAGE_CHILDREN` 重复累计；
- PID 表二次删除；
- zombie reparent 后没有正确通知新 reaper；
- pidfd/wait/vfork waiter 看到不一致状态。

### 3.3 一次间歇性 CAgent reject

`20260802-170611-loongarch64-cagent` 中有一次 kernel agent reject：

- 其余 9 项通过；
- kernel 项在 513 ms reject；
- 串口没有 panic、fatal trap 或 timeout；
- LLM server 只收到 18 个 POST，而正常 10 项应出现 20 个请求。

脚本会删除每个 agent 的临时输出，因此当时无法进一步区分第一次 exec、connect
还是请求写入前失败。没有把它简单标成“外部抖动”；后续改用单个 kernel agent
连续 20 轮，把进程退出、socket/netns 生命周期和首次连接作为聚焦回归。

### 3.4 `tg-xtask` helper thread 创建失败

首次聚焦运行：

```text
.tmp/final-runs/20260802-203851-loongarch64-shell/serial.log
```

`cargo build -p tg-xtask` 在 `futures-core` 阶段失败：

```text
thread 'rustc' (1245186) panicked at rustc_data_structures/src/jobserver.rs:124:
failed to create helper thread: Os { code: 11, kind: WouldBlock,
message: "Resource temporarily unavailable" }
error: could not compile `futures-core` (lib)
warning: build failed, waiting for other jobs to finish...
```

编码 TID `1245186` 对应 PID 38、线程槽 2，系统 PID 也只增长到约 50，因此它不符合
PID/TID 耗尽。内核日志没有 `[mm] OOM` 或 `frame_alloc failed`。glibc 的
`pthread_create()` 会把线程栈 `mmap()` 或 `clone()` 返回的 `ENOMEM` 转换为
`EAGAIN`，所以用户态错误码不能直接证明线程数限制。

## 4. Linux 对照

参考树：

```text
exampleOs/linux
fc02acf6ac0ccde0c805c2daa9148683cdd01ba8
```

本批次提取 Linux 的用户可观察语义、锁域和生命周期原则，不逐行复制其完整
基础设施。

| 本地问题 | Linux 对照 | 采用的原则 |
| --- | --- | --- |
| LoongArch 单页/range 失效 | [`arch/loongarch/mm/tlb.c`](../../exampleOs/linux/arch/loongarch/mm/tlb.c) | 4 KiB 页按 even/odd pair 形成一个 TLB 项；地址按 8 KiB 对齐，使用 `INVTLB_ADDR_GFALSE_AND_ASID`。 |
| 跨 CPU shootdown | [`arch/loongarch/kernel/smp.c`](../../exampleOs/linux/arch/loongarch/kernel/smp.c) | 以 mm 的 resident/CPU mask 为目标，同步完成远端 page/range/mm 失效；共享 kernel range 覆盖所有相关 CPU。 |
| exit 与 wait | [`kernel/exit.c`](../../exampleOs/linux/kernel/exit.c) | 重资源清理先于 waitable zombie 发布；`EXIT_ZOMBIE -> EXIT_DEAD` 只能由一个 waiter 消费；资源统计和 release 只发生一次。 |
| setns 的准备/提交 | [`kernel/nsproxy.c`](../../exampleOs/linux/kernel/nsproxy.c) | 先准备并持有目标 namespace 引用，验证成功后再一次性切换，失败不暴露半完成状态。 |
| namespace fd 生命周期 | [`fs/nsfs.c`](../../exampleOs/linux/fs/nsfs.c) | 打开的 namespace 文件本身持有 namespace 引用，最后关闭才释放。 |
| netns ref 与异步清理 | [`net/core/net_namespace.c`](../../exampleOs/linux/net/core/net_namespace.c)、[`include/net/net_namespace.h`](../../exampleOs/linux/include/net/net_namespace.h) | `get_net/put_net` 统一对象引用；最后引用只排队 cleanup，在工作上下文中先从可发现集合摘除，再运行各协议 teardown。 |
| 默认 overcommit 策略 | [`mm/util.c`](../../exampleOs/linux/mm/util.c) | mode 0 只拒绝单次大于 RAM+swap 的申请；全局 `Committed_AS` 与 `CommitLimit` 的硬比较只属于 mode 2。 |

Linux 使用 `mm_cpumask()`、per-CPU context、通用 SMP call、`tasklist_lock`、引用
计数对象和 workqueue。CongCore 当前规模更小，因此采用固定 hart mask、IPI
mailbox、PCB 锁序和统一 netns lifetime 表，但保留相同的语义边界。

## 5. LoongArch SMP 与 TLB 修复

### 5.1 FDT 驱动的 12 hart 启动

启动核从 FDT `/cpus` 建立 present mask，不再只依赖编译期 CPU 数量。二级 hart
完成以下步骤后才发布 online bit：

1. 进入独立启动栈；
2. 安装共享 kernel page table 和本地 CSR；
3. 初始化 trap、timer、IPI mailbox；
4. 执行本地 full TLB flush；
5. 发布 online bit 并进入调度器。

启动核发出所有启动请求后统一等待 aggregate online mask。远程 shootdown 只向
已经 online 的 hart 发送；尚未 online 的 CPU 会在上线前执行本地 full flush，
从而封闭“更新方取 online mask 时 CPU 正在上线”的窗口。

### 5.2 IPI mailbox 与同步完成

LoongArch 不使用 RISC-V SBI RFENCE。本批实现按 hart 保存 IPI action/mailbox，
用于调度唤醒和 TLB invalidation。PTE writer 的同步流程为：

```text
发布 PTE store
    ↓
记录目标 hart 的 invalidation 请求
    ↓
发送 IOCSR IPI
    ↓
目标 hart 执行 local invtlb 并回写完成序号
    ↓
发送方观察全部 ack 后才释放 retired frame
```

这样 `munmap`、COW、mprotect 或地址空间销毁不会在远端旧 translation 仍可访问时
复用物理页。

### 5.3 ASID 与 8 KiB paired-TLB 规则

Linux LoongArch 的 `local_flush_tlb_range()` 会把 start/end 按
`PAGE_SIZE << 1` 对齐，逐个 paired entry 使用
`INVTLB_ADDR_GFALSE_AND_ASID`；单页失效也先清除地址最低的 pair bit。本地实现采用
相同规则：

- 单页修改实际 invalidation 覆盖所在 8 KiB pair；
- 小范围按 pair 合并，避免重复 IPI；
- 大范围退休当前 mm context/ASID；
- context 回绕后要求各 resident hart 在再次进入用户态前完成本地清理；
- 返回用户态与 invalidation sequence 复查配合，避免刚好漏掉正在切换 mm 的 hart。

### 5.4 supervisor-only trap-context 也属于 mm 一致性

旧 `MapArea::map_batched()` 只把带 `U` 的新 PTE 记录到用户 ASID batch。LoongArch
trap-context 是 supervisor-only 用户地址空间映射，不带 `U`，因此可能出现：

```text
pair 的一半：用户页，远端已有 TLB 状态
pair 的另一半：新安装 trap-context，但没有进入 invalidation batch
```

另一个 hart 第一次进入 `alltraps` 时可能继续看到 pair 另一半的 invalid
translation。现在所有实际安装到该用户 page table 的新映射都会记录到同一个
mm/ASID batch，不再以 `U` bit 作为是否需要 shootdown 的判断条件。

### 5.5 共享 kernel 映射不能使用任意用户 ASID

kernel stack、运行期 PCI ECAM/BAR 使用共享高半区 kernel page table。LoongArch
内核态使用 PGDH/ASID 0，给某个用户 mm 做 ASID invalidation 不能替代共享 kernel
shootdown。因此新增：

- kernel stack 映射安装后、任务可调度前执行 shared-kernel TLB flush；
- PCI ECAM/BAR 新 PTE 安装后执行一次 shared-kernel flush；
- 删除 kernel stack 时仍在 frame 可复用前完成相同级别的同步。

没有把所有 supervisor PTE 都粗暴升级为全 kernel flush：用户 mm 内的
trap-context 仍使用 mm-local ASID batch，只有真正共享的 kernel page table 才走
shared-kernel 路径。

## 6. 线程、信号与进程退出

### 6.1 最后存活线程决定进程 teardown

旧路径过度依赖 `tid == 0` 或 leader 身份。多线程 `exit_group()`、exec 杀死 peer、
leader 先退出等情况下，真正最后离开的线程未必是原 leader。

现在 PCB 使用独立 live-thread 计数和 group-exit owner：

- 每个线程只退休一次；
- 最后存活线程取得 process teardown 所有权；
- group-exit code 由一次性发布的 owner 决定；
- peer 先完成 futex、clear-child-tid、调度队列和 CPU accounting 清理；
- mm、files、SysV shm、packet-ring 和 netns 等进程资源只由最终 owner 释放。

LoongArch trap/signal 路径同时补齐用户 FP/LSX 状态保存恢复和 signal frame 交接，
避免多核调度或信号返回时把上一个任务的扩展寄存器状态带入当前任务。

### 6.2 原子发布 waitable exit

`publish_process_exit()` 在固定的 parent -> child 锁序下共同发布：

- child `is_zombie`、exit code、CPU time；
- parent 的 exited-children queue；
- wait/vfork waiter；
- pidfd waiter；
- parent notification 所需的 exit signal。

在这之前先完成重资源分离。waiter 因此不会看到“已经是 zombie，但尚未进入父
队列”或“PID 已删除，exit 又重新入队”的半状态。

### 6.3 一次性 reap claim

新增 `wait_reaped` 表示一次性的 `EXIT_ZOMBIE -> EXIT_DEAD` claim：

- `wait4()` 和会消费状态的 `waitid()` 必须先成功 claim；
- `WNOWAIT` 只观察，不 claim；
- PID 删除、child CPU time 累计和 PCB 最终释放只执行一次；
- concurrent waiter 不能重复取得同一 child。

### 6.4 reparent、CLONE_PARENT 与错误回滚

- `exit_teardown` 和 live-thread count 阻止正在退出的进程继续成为 orphan reaper；
- zombie 被 reparent 后，新 reaper 会收到退出通知并唤醒 waiter；
- `CLONE_PARENT` 按 prospective-parent -> caller -> child 锁序重新验证父关系、
  identity 和 liveness；无法保持 Linux 语义时在 child 可运行前返回 `EAGAIN`；
- fork/clone 错误回滚先设置 `wait_reaped/exit_teardown`，再按 Arc identity 从当前
  parent 摘除，能抵抗同时发生的 reparent；
- private child mm 回滚 packet-ring 和 SysV shm，`CLONE_VM` 则不误清父 mm。

## 7. Network namespace 生命周期

### 7.1 为什么分离计数仍会误清理

初版修复分别维护 process、namespace-file、socket 和 fork pin 计数，但 cleanup
逐个读取它们。并发 `setns()` 可以形成：

```text
cleanup 先读 process = 0
setns 把 namespace file 交接成 process owner
最后一个 namespace file Drop，file = 0
cleanup 再读 file/socket/pin = 0
cleanup 误拆仍被新 process 使用的 namespace
```

即使每个独立计数都是原子的，这仍是跨类别的 torn snapshot。

### 7.2 统一 lifetime 表与 teardown claim

最终实现把四类引用放入同一个 `NET_NAMESPACE_LIFETIMES` 锁域：

```text
NetNamespaceLifetimeState
 ├─ process_refs
 ├─ file_refs
 ├─ socket_refs
 ├─ transient_refs
 ├─ teardown_in_progress
 └─ dead
```

关键交接都在一个临界区完成：

- `setns()`：`file ref -> process ref`；
- fork publication：`transient pin -> child process ref`；
- process switch：`new process++ -> old process--`；
- cleanup：只有四类引用同时为零时才能原子 claim teardown。

实际协议栈清理不在 lifetime 锁内执行；claim 后先解锁，再清理 TCP/UDP、packet、
raw、UNIX、netlink、netdev 和 smoltcp state，避免 socket Drop/weak registry 递归
进入 lifetime 锁。完成后保留 dead tombstone。namespace ID 单调且不复用，旧裸
ID 不能在 teardown 完成后重新注册资源。

### 7.3 socket 和 namespace fd 的独立引用

进程 `setns()` 后，旧 socket 仍应继续固定在创建时的 namespace。当前独立引用
覆盖：

- TCP/UDP `NetSocketFile`，包括 accept 后的新 TCP socket；
- packet socket；
- raw socket；
- pathname/abstract/connected UNIX socket；
- netlink socket；
- `/proc/<pid>/ns/net` 打开的 `NamespaceFile`。

最后一个 socket/file Drop 只把 namespace 放入 pending cleanup 集合；idle worker
在 registry 锁外执行 cleanup。`setns()` 与 exit/rollback 在 PCB 锁内串行 owner
状态，atomic owner sentinel 保证 process ref 至多释放一次。

## 8. BuildStorm 前置 overcommit 修复

旧实现把 `overcommit_memory=0` 近似成固定硬阈值：

```text
Committed_AS + additional > 1.5 * managed RAM -> ENOMEM
```

BuildStorm 同时运行多个 rustc。每个 fork 后的 COW 地址空间都会独立计入全局
`Committed_AS`；当全局虚拟 commit 超过约 1.5 倍 RAM 后，即使还有可用物理页，
glibc 新申请的少量 pthread 栈也会被拒绝。glibc 再把 `ENOMEM` 转成 `EAGAIN`，
形成第 3.4 节的 rustc jobserver panic。

Linux 当前三种策略的关键区别是：

```text
mode 0 / OVERCOMMIT_GUESS:
    additional > RAM + swap 才拒绝
mode 1 / OVERCOMMIT_ALWAYS:
    commit accounting 不拒绝
mode 2 / OVERCOMMIT_NEVER:
    Committed_AS + additional > CommitLimit 时拒绝
```

本内核还没有 swap，因此 mode 0 使用 managed RAM 作为单次申请上限。修复同时：

- 把三种策略抽成可单测的纯判定函数；
- 覆盖 mode 0 忽略累计 commit、小申请边界、mode 1 放行以及 mode 2 严格限制；
- 让 `mmap()` 与 `brk()` 共用同一判定；
- 仅在实际拒绝时输出限频 `[mm-overcommit]`，包含操作、PID、mode、申请量、
  `Committed_AS`、限制值和空闲页，避免下一次只看到 glibc 转换后的 `EAGAIN`。

这项修改只修复错误的虚拟 commit 门槛。当前内核仍没有 swap、匿名页回收和完整
OOM killer；mode 0 放行后，真正触碰物理内存上限仍可能在 fault 时 OOM，后续应以
低水位回收和关键内核分配保留页增强，而不能重新引入全局虚拟内存硬阈值。

## 9. 验证

### 9.1 静态检查

最终 netns gate 与 overcommit 修复后执行：

```sh
TMPDIR=$PWD/.tmp ARCH=loongarch64 cargo check --quiet \
  --target loongarch64-unknown-none-softfloat

TMPDIR=$PWD/.tmp ARCH=riscv64 cargo check --quiet \
  --target riscv64gc-unknown-none-elf

git diff --check
```

三项退出码均为 0。编译仍会输出仓库已有 warning，没有新增编译错误。本批修改的
Rust 文件已单独执行 `rustfmt --edition 2024`；仓库级 fmt 仍会命中本批之前存在的
`src/syscall/filesystem/perm_utils.rs` 格式差异，没有为了全绿混入无关格式改动。

另尝试了 `cargo check --tests --target loongarch64-unknown-none-softfloat`，目标 sysroot
不提供 Rust `test` crate，因此包括仓库既有 tmpfs/VFS 测试在内的所有 `#[test]`
均报 `E0463: can't find crate for test`。本次新增的四个 policy 单元测试已保留，但
不能把这次 target 基础设施失败写成“测试已运行通过”；实际可执行验证以 9.5 节
guest 聚焦构建为准。

### 9.2 工具链与最小 Cargo

运行：

```text
.tmp/final-runs/20260802-161932-loongarch64-shell/serial.log
```

结果：

- 12/12 hart online；
- `rustc --version` 与 `cargo --version` 各连续完成 12 次；
- `cargo new /tmp/minibuild-codex-1`；
- `cargo build --offline` 成功，用时约 1 分 33 秒；
- 运行生成程序输出 `Hello, world!`。

这个结果只证明动态 glibc Rust 工具链、进程/线程、文件系统和最小链接路径可用，
不等价于数百 crate 的 BuildStorm。

### 9.3 kernel agent 聚焦回归

运行：

```text
.tmp/final-runs/20260802-171856-loongarch64-shell/serial.log
```

同一个 kernel agent 首连接连续执行 20 次：

```text
KERNEL_AGENT_SUMMARY pass=20 fail=0
```

该回归覆盖了此前一次 513 ms reject 对应的短进程、shell、TCP 连接和 server 请求
路径。

### 9.4 最终标准 CAgent

当前最终代码在统一 netns lifetime/teardown gate 完成后运行：

```sh
ARCH=loongarch64 MEM=8G SMP=12 IMAGE_MODE=snapshot ./run.sh cagent
```

日志与评分：

```text
.tmp/final-runs/20260802-174331-loongarch64-cagent/serial.log
.tmp/final-runs/20260802-174331-loongarch64-cagent/score.json
```

| 子项 | 结果 | 时间 | 分数 |
| --- | --- | ---: | ---: |
| factorial | pass | 454 ms | 14.85 |
| date | pass | 735 ms | 14.85 |
| network | pass | 1218 ms | 22.00 |
| cpu | pass | 637 ms | 14.85 |
| kernel | pass | 733 ms | 14.85 |
| fs-create | pass | 494 ms | 22.00 |
| fs-readwrite | pass | 771 ms | 22.00 |
| fs-directory | pass | 1062 ms | 22.00 |
| fs-search | pass | 626 ms | 29.70 |
| fs-usage | pass | 599 ms | 22.00 |

结果为 10/10，judge 权重合计 `199.10`。报告不把它四舍五入成 200。最终串口：

- 有 `OS COMP TEST GROUP END cagent`；
- 有 `Run completed successfully`；
- 没有 `remove_from_pid2process: ... already reaped`；
- 没有 panic、fatal trap 或 failed hart。

### 9.5 `tg-xtask` 聚焦复验

修复 commit `9062586` 后，以 LoongArch64、12 vCPU、8 GiB、snapshot 重新运行：

```text
.tmp/final-runs/20260802-223007-loongarch64-shell/serial.log
```

guest 起始状态：

```text
OVERCOMMIT_MODE=0
MemTotal:       7597804 kB
MemFree:        7567744 kB
CommitLimit:    3797878 kB
Committed_AS:   772 kB
```

结果：

- 正常越过原来失败的 `futures-core`；
- 继续编译 `bytes`、`lock_api`、`once_cell`、`pin-project-lite`、`thiserror` 和
  `equivalent`；
- 继续创建到 PID 40，Cargo 主进程保持 15 个线程；
- 没有 `failed to create helper thread`、`Resource temporarily unavailable`、
  rustc panic、`[mm-overcommit]`、`[mm] OOM` 或 `frame_alloc failed`；
- QEMU 在约 14 分钟时约 401% CPU/5.3 GiB RSS，在约 52 分钟时约
  320% CPU/3.1 GiB RSS，说明中途有子进程完成并释放内存；
- 运行 60 分钟后仍没有 `TG_XTASK_RESULT`，最后一个新 crate 日志约在
  46 分钟，随后保持高 CPU 但无串口进展，按聚焦验证上限用 QEMU `Ctrl-A x`
  主动结束。

因此本次验证确认了原 `EAGAIN` 故障已被修复，但不能把 `tg-xtask` 标为完整通过。
剩余问题更像 LoongArch rustc 极慢或后续调度/futex 停滞，需要新的带进程状态采样
聚焦测试区分；它不应与本次已确认的 overcommit 错误重新混为一个根因。

## 10. BuildStorm 就绪度

BuildStorm 脚本的主要阶段与当前证据如下：

| 阶段 | 当前状态 | 证据 |
| --- | --- | --- |
| 动态 glibc shell 与根文件系统 | 通过 | CAgent、工具链 shell 均正常完成 |
| `/proc`、`/sys`、`/dev` 与基础命令 | 通过短链路 | CAgent 10/10 |
| `rustc`/`cargo` 启动 | 通过 | 连续 12 轮，无 hang |
| 最小离线 Cargo 工程 | 通过 | 1m33s，运行输出正确 |
| `tg-xtask` 辅助工具 | 原 EAGAIN 已修复，整体未完成 | 60 分钟聚焦构建越过 `futures-core`/PID 40，但没有退出码 |
| `arceos-helloworld` 主工作区编译 | 未运行 | 用户明确要求不运行完整 BuildStorm |
| BuildStorm judge 成功分 | 未知 | 没有完整串口日志，不能评分 |
| BuildStorm 性能分 | 未知 | 没有完整编译时间 |

因此“现在能否完整通过 BuildStorm”的严谨答案仍是**未知**。这次修复已经移除
一个可复现的前置 blocker，但 `tg-xtask` 自身尚未完成，并且速度/停滞仍不满足
完整 BuildStorm 的就绪标准，不能替代实际全量编译。

## 11. 尚未覆盖的边界

### 11.1 MM 回滚

`MapArea::append_to()` 的 non-lazy 部分映射失败回滚仍使用本地逐页 unmap，并立即
释放 frame。当前没有会安装 PTE 的外部调用者，因此不是本轮短链路 blocker；若
以后启用，必须改成 batched rollback，在同步 ASID invalidation 后再释放 frame。

### 11.2 netns 语义

- 部分 rtnetlink 请求仍通过 `current_process()` 选择 namespace，而不是始终使用
  socket 固定的 `net_ns_id`；setns 后旧 netlink fd 的完整 Linux 语义尚未实现；
- `socketpair(AF_UNIX)` 的内部 endpoint 没有独立计入 netns refs；当前 endpoint
  不访问会被 netns cleanup 删除的协议栈状态；
- 一般无 socket 网络短操作仍可能只快照裸 `net_ns_id`。本项目把 netns membership
  简化为 PCB 级；若未来允许同一 PCB 多线程并发 `setns()`，应改为 per-task
  namespace，或让所有 in-flight 操作持有 RAII transient pin。

### 11.3 验证范围

- 没有运行完整 BuildStorm；
- 没有运行完整初赛/LTP 回归；
- `tg-xtask` 聚焦运行只确认越过原故障点，没有完成标记；
- 没有运行 unixbench、libcbench 或完整 IOZone；
- 没有 BuildStorm 前后性能对比；
- 本地 final test source 不是最后一次已知的 remote HEAD。

正式进入完整 BuildStorm 前，应先重新核对 final suite HEAD、judge 的 LoongArch
核数和镜像 checksum，再由用户明确授权长时间编译。

## 12. 修改范围与提交记录

首个 `os/` 实现提交涉及 70 个源码/构建文件，共 5372 行新增、2098 行删除。
LoongArch TLB、通用用户页 pin、fork/exit 和 netns teardown 共同修改 MM/TCB/PCB
发布边界，因此保留为一个内核实现提交。后续 overcommit 根因明确且只修改两个 MM
文件，单独形成 `9062586`；文档、顶层构建接入和子模块指针继续分别提交，避免把
证据与实现混在一起。

| 仓库 | commit | 内容 |
| --- | --- | --- |
| `os/` | `e699326f17e23e617a2eddbe5ed8103e572c4a3e` | `fix(loongarch): harden SMP execution and teardown` |
| `os/` | `1e6c19fb32e9fa81cc2a757b090a4973a34118b7` | `docs(final): record LoongArch short-path validation` |
| `os/` | `90625864bb7d6c9de62a9b96538bc0e84a3c078e` | `mm: fix default overcommit heuristic` |
| 顶层 | `2ce9a82410aa8cbeba36362d10c54d5b43c4c2b5` | `build(loongarch): use the soft-float kernel target` |
| 顶层 | `afef9ef51a8ccf874e485951ab7c5666829c269e` | `chore(os): update the LoongArch kernel revision` |
| 顶层 | `af86d2afc950845844d71c285abcb7a94a938c06` | `chore(os): update overcommit kernel revision` |

提交前已确认 vendored smoltcp 的差异只是 rustfmt 副作用，已恢复且没有进入任何
提交。本日报作为独立的顶层文档提交保存。

## 13. 复现

静态检查：

```sh
cd os
TMPDIR=$PWD/.tmp ARCH=loongarch64 cargo check --quiet \
  --target loongarch64-unknown-none-softfloat
TMPDIR=$PWD/.tmp ARCH=riscv64 cargo check --quiet \
  --target riscv64gc-unknown-none-elf
git diff --check
```

短链路 CAgent：

```sh
cd testsuits-final
ARCH=loongarch64 MEM=8G SMP=12 IMAGE_MODE=snapshot ./run.sh cagent
```

`tg-xtask` 聚焦复验需要进入 snapshot shell，只执行：

```sh
cd /work/tgoskits
export RUSTUP_TOOLCHAIN=nightly-2026-05-28 CARGO_NET_OFFLINE=true
cargo build -p tg-xtask
```

本报告不提供或执行完整 BuildStorm 命令，避免误触发长时间编译。需要正式验证时，
应先确认测试源码和镜像，再按用户授权单独运行并保留完整串口、score JSON、QEMU
版本、kernel commit 和宿主负载信息。

## 14. AI 使用说明

AI 用于：只读审查本地 Linux 参考树、分析 QEMU CSR/串口失败现场、推导
LoongArch paired-TLB 与跨 hart shootdown 竞态、实现和复审进程/netns 生命周期、
执行经授权的短链路验证并整理本报告。

所有结论来自本地源码、实际命令、串口日志和 judge JSON。没有修改评分脚本、
伪造测试输出、硬编码测试名返回值、篡改 `/proc/uptime`，也没有把未运行的完整
BuildStorm 写成通过。
