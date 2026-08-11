# 8-4 BuildStorm PreparedWait 退出修复与性能实验复核

## 问题概述

`PreparedWait` 已经封闭普通事件在"最后检查"和"真正睡眠"之间的丢唤醒窗口，但它
绕过了旧调度入口中对致命信号和 `execve()` 同线程组清理请求的检查。阻塞在
`epoll_wait()` 的线程被 `exit_group()` 或另一个线程的 `execve()` 唤醒后，可能把这次
退出请求当成普通事件，重新注册并再次睡眠，使进程退出或 exec 的 de-thread 阶段永久
等待。

## 背景知识


**先检查条件再睡眠——经典丢唤醒竞态**。操作系统课上讲过：一个任务想等某个条件
成立才继续，基本模式是"检查条件，不满足就睡眠"。问题是检查和睡眠之间有一道缝隙。
类比银行取号：你先看叫号屏幕（没轮到），再去取号；恰好这几秒屏幕叫了你的号，但
系统里没你，叫号白叫了，你取了号坐下来等，却再也不会被叫到。

用双核时序图来看：

```text
时间 ──────────────────────────────────────────────────→

CPU 0 (等待者)                    CPU 1 (唤醒者)
─────────────────                 ─────────────────
① 检查条件 → false
                                  ② 设置条件 = true
                                  ③ 唤醒：看等待队列 → 空，什么也不做
④ 把自己加入等待队列
⑤ 睡眠
   …… 永远不会被叫醒 ……
```

问题出在 ①→④ 之间有一段"裸奔"的窗口：等待者已经决定要睡，但还没注册自己；
唤醒者在这段时间来了，发现队列是空的，唤醒信号就丢了。这就叫 lost wakeup
（丢唤醒）。

**Linux 怎么关死这个窗口：prepare_to_wait / finish_wait**。Linux 的做法是把
"注册到等待队列"这一步提前到"检查条件"之前：

```text
等待者                            唤醒者
─────────                         ─────────
prepare_to_wait():
  拿 waitqueue 锁
  把自己挂进队列
  把自己标记为 TASK_INTERRUPTIBLE
  放锁
                                  设置条件 = true
检查条件 → 还是 false？                唤醒()：
                                    拿同一把 waitqueue 锁
                                    取出等待者
                                    放锁
                                    标记等待者为 TASK_RUNNING
schedule()  ← 如果状态还是
              INTERRUPTIBLE 就真的切走

finish_wait():
  把自己从队列里摘掉（如果没被唤醒摘过）
```

关键在三点：
1. 先入队再检查——确保唤醒者一定能在队列里找到等待者；
2. 检查和唤醒共用同一把 waitqueue 锁——二者不能同时进行；
3. 如果入队后发现条件其实已经满足了，等待者把自己标记回 RUNNING，`schedule()`
   不会真的切走——不会白睡。

这样无论唤醒信号在哪个时刻到达，都不会被漏掉。Linux 源码在
`kernel/sched/wait.c` 的 `prepare_to_wait()` 和 `prepare_to_wait_event()`。

**CongCore 的 PreparedWait 做了什么**。本项目用 `PreparedWait` 实现了类似协议：
waiter 在条件锁保护下先把任务状态发布为 `Blocked`，再携带关中断 guard 完成最终
检查和调度提交。这封住了普通事件的丢唤醒窗口。

**本文的问题：封住了普通丢唤醒，但漏了"致命退出"**。Linux 的
`prepare_to_wait_event()` 除了检查条件本身，还额外检查"当前进程有没有收到
致命信号（比如被 `kill -9` 了）"。如果有，它拒绝睡眠，直接返回。CongCore 的
`PreparedWait` 省了这一步。结果是：当 `exit_group()` 或者另一个线程调用
`execve()` 需要清理同组线程时，唤醒信号到了，但 epoll 循环只当它是普通事件，
又重新注册、又睡了回去——进程永远退不干净。

**什么是 exit_group 和 exec de-thread**。`exit_group()` 让整个线程组一起退出：
内核给同组所有线程发 SIGKILL，等它们全部退出。`execve()` 的 de-thread 类似：
一个线程要加载新程序，必须先终止同组其他线程（新地址空间会替换旧的，其他线程没
地方跑了）。两者都要唤醒正在睡眠的同组线程让它们退出。如果某个线程怎么也不醒、
不退出，整个进程就卡住了——这正是本文修复的 bug。

## 如何发现

前一轮 `tg-xtask` 超时后观察到编译子进程已经成为 zombie，而多线程父进程仍未完成
等待；结合 `PreparedWait::sleep()` 与旧 `block_current_and_run_next_impl(true)` 的
源码差异，确认新协议漏掉了 fatal teardown 检查。复核命令和最终日志：

```sh
rg -n 'PreparedWait|block_current_and_run_next_impl|exec.*exit|fatal' os/src/task
ARCH=riscv64 SMP=8 MEM=8G ./run.sh shell
/user/exec_epoll_thread_smoke.bin; echo PREPARED_WAIT_RC=$?
```

```text
.tmp/final-runs/20260803-162551-loongarch64-shell/serial.log
.tmp/final-runs/20260804-013432-riscv64-shell/serial.log
.tmp/iozone/8-4-heap-sharded-3run.log
.tmp/iozone/8-4-heap-hybrid-3run.log
```

第一份日志是前序长构建现场；第二份是修复后的双场景退出回归；最后两份用于否决同时
试验的小对象缓存方案，避免把无关性能实验混入退出修复。

## 怎么解决

**在 PreparedWait 的三个边界检查致命退出**。新增
`exit_for_fatal_teardown_if_requested()`，在三个时刻调用：

```text
已经观察到 wakeup_pending，恢复 Running 之后
尚未收到 wake，提交 scheduler block 之前
远端唤醒并重新获得处理器，sleep() 返回之后
```


## 一、`exit_for_fatal_teardown_if_requested()` 的三步分流

代码在 `os/src/task/processor.rs:75-95`：

```rust
fn exit_for_fatal_teardown_if_requested(&mut self) {
    // 第 0 步：当前栈已经在做退出清理？
    if self.task.borrow_mut().res.is_none() {
        return;
    }
    // 第 1 步：exec de-thread 请求
    if self.task.exec_exit_requested() {
        self.armed = false;
        drop(self.irq_guard.take());
        exit_current_and_run_next(0);
    }
    // 第 2 步：默认致命信号
    if let Some((errno, msg)) = crate::task::signal::check_if_current_signals_error() {
        self.armed = false;
        drop(self.irq_guard.take());
        crate::task::signal::log_signal_exit(msg);
        exit_group_and_run_next(errno);
    }
}
```

### 第 0 步：`TaskUserRes` 防递归

`TaskUserRes` 是一个线程的用户态资源（TID、trap context、用户栈、mm 引用等，定义在 `os/src/task/id.rs:241-252`）：

```rust
pub struct TaskUserRes {
    pub tid: usize,
    trap_cx_slot: usize,
    pub ustack_base: usize,
    pub process: Weak<ProcessControlBlock>,
    memory_set: MmRef,
    ...
}
```

它由 `TaskControlBlockInner.res: Option<TaskUserRes>` 持有（`task_block.rs:469`）。线程退出时的**"不归路"动作**就是 `Option::take()` 把这个 `res` 取走。`processor.rs:81` 的判断 `res.is_none()` 就是发现——"我已经走过那一刀了"。

为什么必须这样做？注释里 `processor.rs:76-80` 解释了：退出清理本身会**让出 CPU**（unmap 旧 mm 时可能触发冷页访问或块 I/O）。如果被远端唤醒后又递归进入 `exit_current_and_run_next`，就会重复消费一次性的生命周期票据（`LiveThreadRetirement`，`id.rs:262-288`）。

所以第 0 步是**递归防护**：已经在退出流程里的栈，不能再次启动退出。

### 第 1 步：task-local exec exit token

`exec_exit_requested()` 在 `task_block.rs:212-214`：

```rust
pub(crate) fn exec_exit_requested(&self) -> bool {
    self.exec_exit_state.load(Ordering::Acquire) == Self::EXEC_EXIT_COUNTED
}
```

`exec_exit_state` 是个三态机（`task_block.rs:150-152`）：`EXEC_EXIT_NONE(0) / EXEC_EXIT_COUNTED(1) / EXEC_EXIT_RETIRED(2)`。谁想触发另一个线程为 exec 退出，就调用 `try_count_exec_exit()` 把它从 `NONE` 比到 `COUNTED`（`task_block.rs:201-210`）。

发起者就在 `process_block.rs:1447-1469`：exec owner 把同组其他线程都过一遍，逐个 `try_count_exec_exit()`，把成功被"标记"的收集起来：

```rust
let candidates = { /* 拷贝同组除自己外的 TCB */ };
let peers = candidates.into_iter().filter(|task| {
    self.exec_remaining.fetch_add(1, Ordering::Relaxed);
    if task.try_count_exec_exit() { true }       // 标记成功
    else {
        let previous = self.exec_remaining.fetch_sub(1, Ordering::Relaxed);
        debug_assert!(previous > 0, "exec peer accounting underflow");
        false
    }
}).collect::<Vec<_>>();
```

收集完后调用 `terminate_tasks_for_exec(peers)` 唤醒它们（`process_block.rs:1493`），然后 owner 自己在 `while exec_remaining != 0 { suspend }` 等它们全部退出（`process_block.rs:1494-1496`）。

第 1 步就是匹配这种情况：唤醒到来，token 置位 → 进入 `exit_current_and_run_next(0)`。这个函数只**单独退出当前线程**（`processor.rs:1940`），不广播 SIGKILL 给整个进程。

注意还有个小细节：`exit_current_and_run_next` 内部也会反过来查"是 exec peer 吗"（`processor.rs:1963`），并 `exit_group_and_run_next`（`processor.rs:2166`）开头也再判一次 `exec_exit_requested`（`processor.rs:2180, 2199`）。这是双向的保险——exec token 优先级高于 group exit，无论谁先发起，token 置位的线程都只走单线程退出路径。

### 第 2 步：默认致命信号

`check_if_current_signals_error()` 在 `signal.rs:521-524`，核心 `check_task_signals_error` 在 `signal.rs:453-519`。它**只有当默认动作是"终止进程"**才返回：

```rust
// signal.rs:506-515
if handler != SIG_DFL {
    continue;                 // 安装了用户 handler：不退出
}
// ...
if signum <= MAX_SIG {
    if let Some(flag) = SignalFlags::from_bits(1u32 << signum) {
        if let Some((errno, msg)) = flag.check_error() {
            return Some((errno, msg));  // SIG_DFL 是"致命"才返回
        }
    }
}
```

也就是说：`SIG_IGN`、`SIG_DFL` 但默认行为不是终止（如 `SIGCHLD` 一类）都不会触发退出；只有真正默认动作是终止的信号才会被这里捕到。

第 2 步走 `exit_group_and_run_next`（`processor.rs:2166`）。这个函数会广播 SIGKILL 给整个线程组（`processor.rs:2189-2195`）：

```rust
// processor.rs:2189-2195
if process.begin_group_exit(&task, tid, exit_code) {
    let _ = crate::task::signal::kill_current(crate::task::signal::SIGKILL_NUM as i32);
}
```

注意它和 exec 的优先级关系：`processor.rs:2196-2201` 显式再查一次 `exec_exit_requested`——如果 exec 在 group exit 中途胜出，立刻改成单线程退出，**绝不让 task-local 的 exec 请求被升级成进程级 SIGKILL**（否则会把发起 exec 的 owner 自己也杀掉）。

### 三步总结：分流顺序

| 步骤 | 检查 | 命中后走 | 影响范围 |
| --- | --- | --- | --- |
| 0 | `res.is_none()` | 直接 return | 已经在退出的栈继续原清理 |
| 1 | `exec_exit_requested()` | `exit_current_and_run_next(0)` | 只 retire 自己这一个线程 |
| 2 | `check_if_current_signals_error` | `exit_group_and_run_next` | 广播 SIGKILL，整组退出 |

## 二、与 Linux 的对应关系

文档的对照表对应这几处代码：

| Linux 机制 | Linux 源码 | CongCore 对应 |
| --- | --- | --- |
| 入队后发布 sleep state 封死丢唤醒 | `kernel/sched/wait.c:prepare_to_wait()` | `PreparedWait::new()`（`processor.rs:48-65`）标记 `TaskStatus::Blocked` |
| 在等待队列锁里拒绝带 pending signal 的可中断/可杀睡眠 | `prepare_to_wait_event()` + `include/linux/sched/signal.h:signal_pending_state()` | `exit_for_fatal_teardown_if_requested()` 三个边界（`processor.rs:116/123/129`） |
| 进程组退出广播 | `kernel/exit.c:do_group_exit()` + `kernel/signal.c:zap_other_threads()` | 第 2 步 + `exit_group_and_run_next` 广播 SIGKILL（`processor.rs:2189-2195`） |
| exec 杀 peer 并以 killable wait 等线程数归零 | `fs/exec.c:de_thread()` | `try_count_exec_exit()` + `exec_remaining` 计数 + `terminate_tasks_for_exec`（`process_block.rs:1461-1496`） |

### 为什么 CongCore 用 task-local token 而非普通 SIGKILL

Linux 的 `de_thread()` 就是直接给所有 peer 发 `SIGKILL`。原因是 Linux 的 `task_struct` 没有"谁发起 exec"的特殊角色概念——SIGKILL 直接杀死所有非 current 的同组线程，current 自身通过 `group_exec_task` 指针豁免。

CongCore 的设计（参见 `processor.rs:2178-2182` 的注释）：

```rust
// SIGKILL sent by de-threading is task-local: it must retire this peer,
// not start a process-wide group exit that would also kill the exec caller.
if task.exec_exit_requested() {
    exit_current_and_run_next(0);
}
```

也就是：**如果 CongCore 也学 Linux 用普通 SIGKILL 位来做 de-thread**，那 `exit_group_and_run_next` 把 SIGKILL 广播过去时，连发起 exec 的那个 owner 也会被牵连——这正是 `exit_group_and_run_next` 自己内部要反复查 `exec_exit_requested()` 改路由的原因（`processor.rs:2180` 和 `2199` 各一次）。

task-local token 的好处是**方向单向、精确**：只置位被要求退出的线程，owner 自己的 `exec_exit_state` 永远是 `NONE`，owner 不会被自己发起的 exec 反向杀死。

代价就是：epoll readiness 循环**扫普通 signal 位扫不到 token**（token 在 `exec_exit_state` 这个独立 `AtomicU8` 里，不在 `pending_signals` 位图）。所以必须在 prepared-sleep 边界**显式**调一次 `exit_for_fatal_teardown_if_requested`，不能只依赖 trap entry 的 signal 扫描。这就是文档"必须在 prepared-sleep 边界显式检查，不能只依赖普通信号扫描"那一句的来历。

## 三、同批被否决的性能实验

这一段的核心：**一个修复批次里搭做了两个看起来 Linux 化、但被证据否决的方案**，本批没有进提交。这两点在当前代码树里都看不到了——验证方式就是查对应代码现在的状态。

### 实验 A：64 桶 futex 等待队列

设想是把 Linux `futex_hash` 那种 64 桶哈希搬进来，降低 futex 全局锁争用。但工程证据不足（在受控 BuildStorm 进度窗口里没看到收益），所以回退。

现在 `os/src/syscall/futex.rs:52` 还保留着最初的"单 BTreeMap"结构：

```rust
static ref FUTEX_QUEUES: Mutex<BTreeMap<FutexKey, VecDeque<FutexWaiter>>> = ...
```

如果是 64 桶版本被保留下来，应当看到的是 `[Mutex<BTreeMap>; 64]` 或 `array::from_fn(|_| Mutex::new(...))`。没有 = 已回滚至单 map。

项目的 FutexKey 域里还有相关并发设计——例如 `task_block.rs:115` 有一条"退出清理可直接进入对应 futex bucket 删除 waiter，避免扫描所有桶"的痕迹注释。但它未与 64 桶绑定，桶内仍然走 `FutexKey` 索引。

### 实验 B：分片 buddy + per-hart 小对象缓存

这个实验设想类似 Linux SLUB 的 per-CPU kmem_cache + PCP（per-CPU page cache）。后者的前提是 Linux 完整的 slab/page 双层分层、**批量补充（refill）和排空（drain）**、以及水位控制（`min`/`low`/`high` watermarks），否则每核缓存会过度持有空闲对象，整体碎片化严重。

实测结果（`8-4-buildstorm-prepared-wait-followup.md:154`）：

| 指标 | 现状 baseline | 加 per-hart 小对象缓存后 |
| --- | ---: | ---: |
| IOZone 三轮总耗时中位数 | 19.15 s | 19.90 s（**慢 3.9%**） |
| initial write | 基线 | 下降 8.1% |
| rewrite | 基线 | 下降 13.4% |

写吞吐大幅下降，原因是新增的 per-hart 缓存层在每核持有空闲对象后，对小对象分配热路径没有净收益，反而让跨核释放（这是写 I/O 的高频操作）多走一段缓存回收逻辑。同时缺少 Linux 的水位控制让缓存里的空闲页迟迟不回 buddy。

回退后，当前 `os/src/mm/heap_allocator.rs:101-111` 看到的是项目**保留的** baseline 形态：

```rust
/// A Linux-shaped shared page zone with refillable per-hart slab caches.
...
slab_pages: [UnsafeCell<SlabPageMeta>; HEAP_PAGE_COUNT],
```

`heap_allocator.rs:24-25` 注释："Retain one completely empty slab per class and hart. This is the bounded refill cache; a second empty slab is drained to the shared zone." 也就是说**每类每核只缓存一个空 slab，第二个就让回共享区**——这是有边界的简单分层，**没有** Linux 的批量 refill/drain、水位、PCP 列表抽页等机制。被否决的实验是在此基础上**再加一层** per-hart 小对象缓存，回退后只是回到这条被证明不退化的简单 slab 线。


## 对应提交

- 内核：`e625f82f3da80b315baf6cbe8627245e83bb218d`
  `sched: honor fatal teardown in prepared waits`。
- 回归：`190573fd5c08405fffad55288c11ad3e1eb3c38d`。
- 顶层集成：`927328f62ecfb753c9c4cb0bd825088be304f48e`。
- 文档提交：`1427fea2a8082558acb85777c70d5ec409c82b3f`。

## 对比提升

修复后测试输出 `EXIT_GROUP_PREPARED_WAIT_PASS`、exec 后新映像输出 `PASS`，返回码 0；
它证明两种退出路径从"可能永久等待"变为可完成，但不是性能百分比。被否决的小对象
缓存把 IOZone 三轮中位数 `19.15 s -> 19.90 s`（慢 3.9%），initial write 和 rewrite
分别下降 8.1% 和 13.4%，所以没有进入提交。完整 BuildStorm 未运行。

以下是 AI 的具体分析，作为存档。

---

## 历史分析背景

本批次的核心难点在于 PreparedWait 是项目唯一的防丢唤醒调度协议，修改它的退出检查
必须同时考虑正常 I/O 唤醒、exit_group 唤醒和 exec de-thread 唤醒三种来源，而且
退出清理自身也可能让出 CPU（冷页访问、块 I/O），必须防止递归退出。同批还试验了
分配器方案，需要完整的 A/B 证据来否决。保留长篇分析供后续参考。

## 1. 结论

本批次没有运行完整 BuildStorm。工作集中在前一轮 `tg-xtask` 卡顿现场暴露出的
多线程退出边界，并用聚焦运行态回归验证修复：

- `PreparedWait` 现在会在"已收到唤醒""提交调度前"和"被调度回来后"三个边界
  检查 exec de-thread 请求与默认致命信号；
- 阻塞在 `epoll_wait()` 的同组线程会在 `exit_group()` 或另一个线程 `execve()` 时
  完成退出，不再把致命 teardown wake 当作普通事件后重新睡眠；
- 新增双场景用户态回归，同时覆盖普通进程组退出和 exec de-thread；
- RISC-V64、LoongArch64 内核和新增用户回归的静态构建均通过；RISC-V 8 hart
  运行态回归输出两项 PASS，返回码为 0。

本批次还复核了两个看似 Linux 化、但证据不足或实际退化的性能方案：

- 64 桶 futex wait queue 没有在受控窗口内证明进度收益，已完全回退；
- "分片 buddy + per-hart 小对象缓存"虽然类似 Linux SLUB/PCP，但三轮 IOZone
  中位总耗时比原分片 buddy 慢 3.9%，并且写吞吐下降，已完全回退。

最终保留的内核 diff 只有 PreparedWait 致命退出修复。不能据此声称完整 BuildStorm
已经通过，也不能把被回退实验算作性能提升。

## 2. 版本与测试资产

| 资产 | 值 |
| --- | --- |
| 顶层基线 | `1c8f6670bf6767750bd893772f6c0eacc1a1e56a` |
| `os/` 基线 | `04bf218113d8f4045db9750f751ab5fd58fbcc22` |
| `os/` 最终 revision | `e625f82f3da80b315baf6cbe8627245e83bb218d` |
| 顶层回归提交 | `190573fd5c08405fffad55288c11ad3e1eb3c38d` |
| 顶层内核集成提交 | `927328f62ecfb753c9c4cb0bd825088be304f48e` |
| final 测试分支/commit | `final-2026` / `1eac61d3becaa592c8ef12a7535f0ec6bb9e3e36` |
| 本地 Linux 参考树 | `fc02acf6ac0ccde0c805c2daa9148683cdd01ba8` |
| QEMU | 11.0.3 |
| 最终运行态 guest | RISC-V64，8 vCPU，8 GiB，snapshot |
| 决赛根镜像 | `sdcard-rv-pub.img`，14 GiB raw ext4 |
| 决赛镜像 SHA-256 | `d899fe43d333d1d17ad8a5f8a8b74b68117b8c1ceacfc3843bfeadb1ca705bd1` |
| IOZone 临时镜像 SHA-256 | `c0cb4e209e0aa243af72c599c3e34a679ff1f195ac46789e054b77eac3bf453d` |

`run.sh` 在启动最终回归前重新扫描并确认了 14 GiB 决赛镜像校验值。运行使用 QEMU
snapshot，没有修改基准镜像。final 测试源码未拉取或更新。

## 3. 失败根因

前一轮为避免条件检查与真正睡眠之间丢唤醒，引入了 `PreparedWait`：waiter 在条件锁
保护下把任务发布为 `Blocked`，然后携带 irq-save guard 完成最终检查和 scheduler
commit。这个协议封住了普通事件的 lost wakeup，但它绕过了旧的
`block_current_and_run_next_impl(true)`，后者原本会在睡眠前检查：

1. exec de-thread 的 task-local exit token；
2. 默认动作是终止进程的 pending signal。

当 `exit_group()` 或 `execve()` 的 de-thread 协调者唤醒一个 epoll waiter 时，旧
`PreparedWait::sleep()` 只消费 `wakeup_pending` 并返回。对普通 I/O 事件这是正确的，
但对 teardown wake 不够：尤其 exec token 不是一个普通 SIGKILL 位，epoll readiness
循环无法通过自己的 signal scan 识别它，于是可能再次注册 waiter 并睡眠，使 exec
协调者一直等不到 peer retirement。

退出清理自身还可能在旧 mm 的冷页访问或块 I/O 上让出 CPU。此时
`TaskUserRes` 已被取走，不能再次递归进入一次性线程退出流程，否则会重复消费
`LiveThreadRetirement` 等生命周期票据。

## 4. Linux 对照

本次直接阅读本地 `exampleOs/linux`，没有逐行移植：

| Linux 机制 | 参考文件 | 本项目对应实现 |
| --- | --- | --- |
| 入队后发布 sleep state，封闭 lost wakeup | `kernel/sched/wait.c:prepare_to_wait()` | `PreparedWait::new()` + `wakeup_pending` |
| 在 wait-queue 锁下拒绝带 pending signal 的 interruptible/killable sleep | `kernel/sched/wait.c:prepare_to_wait_event()`、`include/linux/sched/signal.h:signal_pending_state()` | PreparedWait 三个 fatal-teardown 检查点 |
| `exit_group()` 发布 group exit 并杀死其他线程 | `kernel/exit.c:do_group_exit()`、`kernel/signal.c:zap_other_threads()` | group SIGKILL pending bit + wake |
| exec 杀死 peer，并以 killable wait 等待线程数归零 | `fs/exec.c:de_thread()` | task-local exec exit token + peer retirement counter |

Linux 的关键语义不是"任何 wake 都可以继续等待"，而是 interruptible/killable waiter
在提交 sleep 时必须同时观察 fatal signal。Linux 的 exec peer 使用 SIGKILL；本项目
为了不把 peer teardown 重新广播给 exec owner，使用 task-local token，因此必须在
相同的 prepared-sleep 边界显式检查 token。

## 5. 实现

`os/src/task/processor.rs` 新增
`PreparedWait::exit_for_fatal_teardown_if_requested()`，检查顺序为：

1. 若 `TaskUserRes` 已被取走，说明当前栈正在执行退出清理，继续原清理流程；
2. 若 task-local exec exit token 已发布，解除 token 自身的 armed/irq 状态并进入
   `exit_current_and_run_next(0)`；
3. 若存在默认致命 signal，记录退出原因并进入 `exit_group_and_run_next()`。

调用点覆盖三种竞态：

```text
prepare Blocked
    -> 已有 wake_pending：恢复 Running 后检查 teardown
    -> 尚无 wake：提交 schedule 前检查 teardown
    -> schedule 被远端 wake 后返回：再次检查 teardown
```

普通 readiness wake 仍按原协议返回调用者重查条件；没有引入轮询、固定延时或测试名
特判。

新增 `user/src/bin/smoke_archive/exec_epoll_thread_smoke.rs`：

- 子进程创建一个同 mm/sighand/thread-group 的线程，让其无限阻塞在 epoll；主线程
  调用 `exit_group(0)`，父进程用 `waitpid()` 验证整个组可退出；
- 当前进程再次创建同类 epoll waiter，主线程 exec `/user/basename.bin`；新映像输出
  `PASS`，证明 exec peer 已退休且 exec 没有被错误的 group SIGKILL 杀死。

## 6. 运行态验证

最终命令：

```sh
ARCH=riscv64 SMP=8 MEM=8G ./run.sh shell
/user/exec_epoll_thread_smoke.bin; echo PREPARED_WAIT_RC=$?
```

串口日志：

```text
.tmp/final-runs/20260804-013432-riscv64-shell/serial.log
```

关键输出：

```text
EXIT_GROUP_PREPARED_WAIT_PASS
PASS
PREPARED_WAIT_RC=0
```

启动日志同时确认 `riscv64 SMP online mask 0xff (8 harts), failed=0x0`。

## 7. 分配器 IOZone A/B 与回退决定

实验配置：RISC-V64、8 vCPU、2 GiB、QEMU snapshot，4 个 IOZone worker，每个
4 MiB 文件，1 KiB record，只运行 sequential write/rewrite/read/reread。命令：

```sh
IOZONE_RUNS=3 IOZONE_WORKLOAD=sequential \
  bash tools/run_iozone_focus.sh \
  /home/shiyicong/temp/CongCore \
  .tmp/iozone/iozone-root.img \
  <log>
```

三轮中位数如下，吞吐单位为 KiB/s：

| 实现 | 总耗时/s | initial write | rewrite | read | reread |
| --- | ---: | ---: | ---: | ---: | ---: |
| 原 per-hart 分片 buddy | 19.15 | 5154.16 | 17082.61 | 14715.09 | 14573.42 |
| 分片 buddy + 小对象缓存 | 19.90 | 4737.26 | 14800.60 | 14468.31 | 13616.02 |
| 缓存方案相对变化 | **慢 3.9%** | -8.1% | -13.4% | -1.7% | -6.6% |

原始日志：

```text
.tmp/iozone/8-4-heap-sharded-3run.log
.tmp/iozone/8-4-heap-hybrid-3run.log
```

Linux SLUB/PCP 的 CPU-local freelist 不仅是一组本地链表，还依赖成熟的 slab/page
分层、批量 refill/drain、水位控制和 NUMA/zone 策略。把小对象缓存直接叠在当前 buddy
分片上增加了 refill/drain 与碎片化成本，本场景没有收益。因此实验代码已完全回退，
不把"形式上更像 Linux"当成交付理由。

## 8. 静态验证

以下检查均返回 0：

```sh
ARCH=riscv64 cargo check --offline --manifest-path os/Cargo.toml \
  --target riscv64gc-unknown-none-elf

ARCH=loongarch64 cargo check --offline --manifest-path os/Cargo.toml \
  --target loongarch64-unknown-none-softfloat

cargo check --offline --manifest-path user/Cargo.toml \
  --bin exec_epoll_thread_smoke --target riscv64gc-unknown-none-elf

cargo check --offline --manifest-path user/Cargo.toml \
  --bin exec_epoll_thread_smoke --target loongarch64-unknown-none

git -C os diff --check
git diff --check
```

构建仅有仓库既有 warning，没有新增编译错误。`user/.cargo/config.toml` 在测试结束后
保持 RISC-V 默认 target。没有运行完整 BuildStorm、完整 LTP、CAgent 或完整性能套件。

## 9. 适用边界与下一步

- 当前 PreparedWait 仍是项目的锁保护 REF-wait 模型，没有实现 Linux waitqueue 的
  exclusive waiter、restart block 或完整 task-state 位图；
- 运行态聚焦回归已覆盖 RISC-V；LoongArch 本批次完成静态检查，先前同一修复的
  LoongArch 聚焦回归也通过，但不把旧日志冒充本次最终运行；
- 下一步应重新运行受控的 `tg-xtask` 聚焦窗口，确认 de-thread 卡点消失，再依据新的
  PC/进度证据选择下一个瓶颈；在它能稳定完成以前仍不应运行或宣称完整 BuildStorm。
