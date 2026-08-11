# 8-3 BuildStorm 子进程 wait 防丢唤醒与 inode/文件页并发扩展性修复

## 问题概述

`tg-xtask`  过程中

```text
Cargo wait 子进程  --丢失唤醒--> 永久睡眠
unlink             --扫描全局--> 进程数 × fd 数
write / truncate   --扫描全局--> 进程数 × 映射数
```

## 一、Cargo wait 子进程 → 丢失唤醒 → 永久睡眠

丢失唤醒（lost wakeup）是同步原语的经典时序错误。Cargo 调 `wait4()` 回收 rustc，正确流程是"检查有没有子进程退出 → 没有就睡"。问题出在这两步之间：如果 rustc 恰好在这个窗口里退出，它发出的唤醒打在一个还没把自己标记成 Blocked 的任务上，唤醒被丢掉；随后任务真的睡下去，而它等的事件已经发生过了，再没有人会来叫它。于是永久睡眠。

现场特征很典型：rustc 已经变成僵尸，Cargo 却不 reap；手工发一个 `SIGCHLD` 构建立刻继续。手工信号提供了第二次唤醒事件——条件早就满足了，缺的只是叫醒动作。

修复是照 Linux 的 prepare-to-wait 协议，把顺序反过来：先声明要睡，再做最终检查。

```rust
// os/src/task/processor.rs:41
#[must_use = "a prepared wait must be slept or cancelled"]
pub(crate) struct PreparedWait {
    task: Arc<TaskControlBlock>,
    irq_guard: Option<crate::sync::LocalIrqSaveGuard>,
    armed: bool,
}
```


`wait4` 的调用点长这样：

```rust
// os/src/syscall/process/wait.rs:783
// Match Linux prepare_to_wait(): publish Blocked while the parent PCB
// lock still excludes child-exit publication, then retain irq-disable
// until the scheduler commit.
let prepared = PreparedWait::new().expect("wait4 lost its current task");
drop(process_inner);
prepared.sleep();
```

具体过程是这样 

## 原始过程中 

天真的顺序是：查条件 → 没满足 → 把自己标成 Blocked → 交给调度器。

```text
睡眠方 (hart 0)                      唤醒方 (hart 1)
──────────────────────────────────────────────────────
查条件：没有子进程退出
                                     子进程退出，条件变成真
                                     叫醒目标：状态不是 Blocked
                                     → 什么都没做，唤醒丢掉
状态 = Blocked
schedule()                           ...
永久睡眠
```

修复的核心思路只有一句：把"声明我要睡"提前到"最后一次查条件"之前，并且让声明和查条件都在同一把保护条件的锁下完成。

## 协议长什么样

调用方的标准写法，以内核等待队列为例（`os/src/sync.rs:70`）：

```rust
loop {
    if ready() { return; }                       // 快路径
    let irq_guard = LocalIrqSaveGuard::new();    // 1. 关本地中断
    let mut waiters = self.inner.lock();         // 2. 取条件锁
    if ready() { return; }                       // 3. 锁内复查
    waiters.push_back(task);                     // 4. 登记到等待队列
    let prepared = PreparedWait::with_irq_guard(irq_guard).unwrap();  // 5. 声明要睡
    drop(waiters);                               // 6. 放条件锁
    prepared.sleep();                            // 7. 真正睡
}
```

注意 5 在 6 之前：状态改成 `Blocked` 这件事发生在条件锁里面。唤醒方要改条件，必须先拿这把锁，所以它不可能在"我已登记但还没标记 Blocked"的状态下看到我。


## 分四步走一遍

### 第一步：arm（武装）

```rust
// os/src/task/processor.rs:52
pub(crate) fn with_irq_guard(irq_guard: LocalIrqSaveGuard) -> Option<Self> {
    let task = current_task()?;
    {
        let _wakeup_guard = task.lock_wakeup_transition();   // 任务级过渡锁
        let mut inner = task.borrow_mut();
        debug_assert_eq!(inner.task_status, TaskStatus::Running);
        inner.task_status = TaskStatus::Blocked;             // 提前发布
    }
    Some(Self { task, irq_guard: Some(irq_guard), armed: true })
}
```

做了两件事：状态置成 `Blocked`，以及把调用方传进来的中断关闭守卫接管过来。

`wakeup_lock`（`lock_wakeup_transition()`）是本地版的 Linux `task_struct::pi_lock`。它保证"改状态"和唤醒方的"读状态、读 on_cpu、决定怎么办"互斥。注释写得很直接：

```rust
// os/src/task/task_block.rs:78
/// This is the minimal counterpart of Linux `task_struct::pi_lock` in
/// `try_to_wake_up()`: the outgoing CPU and a concurrent hardirq waker
/// must agree whether the task is still on-CPU or already enqueueable.
```

中断必须关，因为定时器中断的抢占逻辑会把任务状态改回 `Running`——那样刚发布的 `Blocked` 就白发布了。守卫从 arm 一直持有到任务真正切走，中间不放。

### 第二步：交接

调用方放掉条件锁。从这一刻起唤醒方可以进来了，而它看到的是一个已经 `Blocked` 的目标。

### 第三步：commit（提交睡眠）

```rust
// processor.rs:97
pub(crate) fn sleep(mut self) {
    let wake_already_pending = {
        let _wakeup_guard = self.task.lock_wakeup_transition();
        let pending = self.task.wakeup_pending.swap(false, AcqRel);
        if pending {
            self.task.wakeup_sync_hart.store(OFF_CPU, Release);
            self.task.borrow_mut().task_status = TaskStatus::Running;   // 撤销
        }
        pending
    };
    if wake_already_pending {
        self.armed = false;
        self.exit_for_fatal_teardown_if_requested();
        return;                                  // 根本不睡，直接回去重查条件
    }
    self.exit_for_fatal_teardown_if_requested();
    self.armed = false;
    block_prepared_current_and_run_next();       // 真的切走
    self.exit_for_fatal_teardown_if_requested();
}
```

`wakeup_pending` 是唤醒到达的标志。如果在第二步的窗口里有人来叫过，唤醒不会丢，而是存在这个原子标志里；`sleep()` 一进来就 `swap(false)` 检查，有就把状态改回 `Running` 直接返回，让调用方的 `loop` 再查一遍条件(wait 内部有一个循环 睡眠 等待 )。

没有待处理唤醒才真的切走：

```rust
// processor.rs:1818
fn block_prepared_current_and_run_next() {
    let task = take_current_task()?;
    charge_task_runtime_for_scheduler(&task);
    debug_assert_eq!(task.borrow_mut().task_status, TaskStatus::Blocked);
    ...
    local_processor().lock().set_pending_blocked(task);   // 只暂存，不入队
    schedule(task_cx_ptr);
}
```

关键是 `set_pending_blocked`：任务被暂存在本 hart 的处理器结构里，不做任何全局发布。此时 `on_cpu` 仍然指向本 hart——任务还在自己的内核栈上执行 `schedule()`。

### 第四步：idle 收尾

切到 idle 上下文后，才由 idle 做最后的清理（`processor.rs:1381`）：

```rust
if let Some(task) = local_processor().lock().take_pending_blocked() {
    let should_wake = {
        let _wakeup_guard = task.lock_wakeup_transition();
        task.clear_on_cpu();                                  // 现在才宣布"我下 CPU 了"
        task.wakeup_pending.swap(false, AcqRel)              
    };
    if should_wake {
        // 切走期间有人叫过 → 补唤醒
        let sync_hart = task.wakeup_sync_hart.swap(OFF_CPU, AcqRel);
        if sync_hart < MAX_HARTS { wakeup_sync_task_on_hart(task, sync_hart); }
        else { wakeup_task(task); }
    }
}
```

## 取消路径

如果调用方复查后发现条件已满足，`PreparedWait` 直接被 drop：

```rust
// processor.rs:132
impl Drop for PreparedWait {
    fn drop(&mut self) {
        if self.armed {
            let _wakeup_guard = self.task.lock_wakeup_transition();
            if inner.task_status == TaskStatus::Blocked {
                inner.task_status = TaskStatus::Running;      // 恢复
            }
            // 取消意味着调用方已经复查过条件、会继续运行或返回。
            // 期间攒下的唤醒因此已被观察到，不能泄漏给下一次无关的睡眠。
            self.task.wakeup_pending.store(false, Release);
            self.task.wakeup_sync_hart.store(OFF_CPU, Release);
        }
        drop(self.irq_guard.take());
    }
}
```

清 `wakeup_pending` 这一步容易被忽略但很重要：不清的话，一个已经被"复查条件"消化掉的唤醒会残留下来，导致之后某次完全不相关的睡眠被立即唤醒（虚假唤醒）。

## 唤醒方那一侧

```rust
// os/src/task/manager.rs:335
let _irq_guard = LocalIrqSaveGuard::new();
let _wakeup_guard = task.lock_wakeup_transition();

// SMP 安全性：如果任务确实仍在某个 hart 上执行，不要直接入队
if task.on_cpu.load(Acquire) != TaskControlBlock::OFF_CPU {
    task.wakeup_pending.store(true, Release);     // 存进位置 ，交给 idle 补
    return;
}
// 否则：状态是 Blocked → 改 Ready → 清 wakeup_pending → 入队
```

唤醒方先看 `on_cpu` 而不是先看状态。这是整套协议的枢纽：睡眠方在真正切走之前 `on_cpu` 一直是自己，所以任何在窗口期到达的唤醒都必然走"存暗格"这条分支，绝不会试图把一个还在跑的任务入队。


## 四种交错都是安全的

```text
唤醒时刻                            结果
────────────────────────────────────────────────────────────
① arm 之前                          第三步的锁内复查会看到条件已满足 → 不睡
② arm 之后、放条件锁之前            唤醒方拿不到条件锁，只能等 → 退化成 ③
③ 放条件锁之后、schedule 之前       on_cpu 还是自己 → 存 wakeup_pending
                                    sleep() 或 idle 收尾时消费 → 立即醒/补唤醒
④ 切到 idle、on_cpu 清掉之后        标准路径：Blocked → Ready → 入队
```

四条路径没有空隙，这就是"不可丢唤醒"的含义。

## 附带处理的致命信号

`sleep()` 里三处 `exit_for_fatal_teardown_if_requested()` 对应 Linux 在提交 `TASK_INTERRUPTIBLE` 睡眠前调用的 `signal_pending_state()`。原因是 SIGKILL 和 exec 的 `de_thread()` 致命信号不能被当作一次普通唤醒——普通唤醒的语义是"回去重查条件"，条件不满足就又睡回去了，进程就杀不掉。所以这三个检查点分别覆盖：提交睡眠前、被唤醒返回后、以及消费掉暗格唤醒后。

```rust
if self.task.exec_exit_requested() { ...exit_current_and_run_next(0); }
if let Some((errno, msg)) = check_if_current_signals_error() { ...exit_group_and_run_next(errno); }
```

检查两种形式是因为 exec 用的是任务本地的一个 token，而普通 group exit 用的是待处理信号位图。

## 二、unlink → 扫描全局 → 进程数 × fd 数

背景是 POSIX 语义：`unlink` 一个仍被打开的文件，目录项立刻消失，但数据要活到最后一个 fd 关闭。Linux 靠 VFS 的 inode/dentry 引用计数天然做到；本项目的 ext4-fs 没有暴露那个生命周期，所以用"改名成隐藏名、等最后一个 close 再真删"来模拟。

要决定是否需要这么做，就得回答"这个 inode 现在还有人打开吗"。旧实现是全局扫描：

```rust
// 04bf218 删除的 has_open_inode_view()
let processes = { PID2PCB.lock().values().cloned().collect::<Vec<_>>() };
for process in processes {
    let Some(inner) = process.try_borrow_mut() else {
        // 拿不到锁就保守地认为"打开着"
        return true;
    };
    let table = Arc::clone(&inner.files);
    ...
    if table.lock().iter_files_snapshot().into_iter().any(|(_fd, file)| {
        file.as_any().downcast_ref::<OSInode>() ... // 比对 (dev, ino)
    })
}
```

每次 `unlink` 都是 O(进程数 × fd 数)，还要顺序取全局进程表锁和每张 fd 表锁。并行构建正好是最坏情况：几十个 rustc 各开着大量 fd，而 cargo 删临时文件极其频繁。那个"拿不到锁就 `return true`"的分支更糟——它把锁竞争变成了错误的语义判断。

修复换成 inode 级计数，把"谁打开着"记在 inode 自己身上：

```rust
// os/src/fs/inode.rs:466
struct InodeLifetimeState {
    /// 统计 OSInode 打开描述，而不是 fd 槽位。dup()/fork() 共享一个描述，
    /// 而 epoll 和 SCM_RIGHTS 的引用可以在最后一个 fd 槽消失后继续持有描述。
    open_descriptions: usize,
    /// unlink 在改名前先预约生命周期，封闭"最后一次 close"与 rename 的竞态。
    unlink_reservations: usize,
    /// 等最后一个描述消失后要删除的隐藏名。一个 inode 可以有多个硬链接名。
    deferred_cleanups: Vec<TmpfileCleanup>,
}
```

计数在 `register_open_inode_description()` / `unregister_open_inode_description()` 维护，后者在 `OSInode::drop()` 里调用，返回待清理项交给锁外执行。查询从"遍历全系统"变成一次 `BTreeMap` 查找。

`unlink_reservations` 解决的是原子性：判断"有人打开"和"改名成隐藏名"之间如果最后一个 close 挤进来，隐藏名就没人负责删了，变成垃圾文件。

```rust
// inode_utils.rs defer_unlink_open_file() 修复后
let Some(reservation) = reserve_deferred_unlink(child) else { return Ok(false) };
...
match parent.rename(name, &hidden) {
    Ok(_) => { reservation.commit(Arc::clone(parent), hidden); return Ok(true); }
    Err(e) => return Err(ext4_err_to_errno(e)),
}
```

`reserve_deferred_unlink` 返回 `None` 表示确实没人打开，可以直接删。预约期间计数不归零，所以 close 不会提前触发清理；`commit()` 原子地放掉预约并登记隐藏名；rename 失败时 `Drop` 自动释放预约（`inode.rs:575`）。

## 三、write / truncate → 扫描全局 → 进程数 × 映射数

这条是 mmap 一致性的代价。`MAP_SHARED` 映射了某文件的进程，必须能看到别人用 `write()` 写进去的字节；`truncate` 改了 i_size，每个映射的有效长度也要更新（决定尾部访问是否该 `SIGBUS`）。

旧实现每次写都全表扫描：

```rust
// 04bf218 删除的 update_inode_mmaps_size_all_processes()
resize_file_page_cache(dev, ino, file_size);
let processes = { PID2PCB.lock().values().cloned().collect::<Vec<_>>() };
for process in processes {
    let Some(memory_set) = process.try_memory_set() else { continue };
    memory_set.update_file_vm_size(dev, ino, file_size);
}
```

`write` 路径更狠：用户缓冲按 1 KiB 分块拷贝，每块都做一遍这个扫描。编译器输出的 `.o` 文件从来没被任何人 mmap，却要为每一块付 O(进程数 × VMA 数)。

第二层问题在页缓存本身。原来全系统所有文件的缓存页挤在一个 `(dev, ino, page)` 的 `BTreeMap` 里，一把锁：互不相关的 rustc worker 写不同文件也要排队，`truncate` 想找某个 inode 的页得扫全系统的缓存。

`1d3b44b` 拆成两级，并加了反向索引：

```rust
// os/src/mm/memory_set/backing.rs:64
/// Per-inode page index, corresponding to Linux `inode->i_mapping->i_pages`.
struct FilePageCacheMapping {
    pages: Mutex<BTreeMap<usize, FilePageCacheSlot>>,
    /// Weak reverse map corresponding to Linux `address_space::i_mmap`.
    /// Stale entries are pruned opportunistically; they never retain an mm.
    mmap_mms: Mutex<Vec<super::mm_ref::WeakMmRef>>,
}
```

外层 `FILE_PAGE_MAPPINGS` 只负责按 `(dev, ino)` 找到 mapping，页查找和失效都用 inode 自己的锁——不同文件之间不再互相阻塞。

`mmap_mms` 是 Linux `address_space::i_mmap` 的对应物：只登记真正映射了这个 inode 的地址空间。`mmap` 时 `register_file_mmap()` 登记，写和改大小时只访问这些：

```rust
// os/src/mm/memory_set.rs:281
pub(crate) fn update_file_mmap_sizes(dev: usize, ino: u32, file_size: usize) {
    resize_file_page_cache(dev, ino, file_size);
    for mm in backing::file_page_cache_mapped_mms(dev, ino) {
        let _ = mm.update_file_vm_size(dev, ino, file_size);
    }
}
```

用 `Weak` 而不是 `Arc` 是必须的：反向索引不能让一个已退出的地址空间活着。`file_page_cache_mapped_mms()` 每次顺手 `retain(WeakMmRef::is_alive)` 剪掉死条目。没人映射时返回空 `Vec`，整条路径的成本降到零——这正是编译器输出文件的常见情况。

## 三者的关系

第一个是同步原语的时序错误，属于正确性问题，会直接卡死。后两个是"用全局扫描代替索引"的可扩展性错误，单任务构建看不出来（进程少、fd 少、映射少），并行构建下变成全局锁热点。共同点是三者都要有足够的并发才暴露，所以文档里是先用 `cargo build -j1` 稳定复现丢失唤醒，再靠源码检索（`rg -n 'PID2PCB' os/src`）把两处扫描找出来。


## 如何发现

先用单任务构建稳定复现：

```sh
cd /work/tgoskits
timeout 300 cargo build -p tg-xtask -j1
```

卡住时 rustc 子进程已经成为僵尸，但 Cargo 没有 reap；手工发送 `SIGCHLD` 后构建立即
继续，锁定 wait 的丢失唤醒。QEMU 现场与源码检索又定位到 unlink 的全局 fd 扫描，
以及 write/resize 对全部进程 `mm` 的扫描：

```sh
rg -n 'defer_unlink_open_file|has_open_inode_view|PID2PCB' os/src
rg -n 'FILE_PAGE_MAPPINGS|FilePageCacheMapping|WeakMmRef' os/src/mm
```

## 怎么解决

具体代码细节已经在上面列出了。

- 用 `PreparedWait` 原子衔接“登记等待、最终复查、进入睡眠”，并统一迁移
  `wait4()/waitid()`、vfork、等待队列、poll、epoll 与 pselect 路径。

- 用 inode 级打开描述计数和 unlink reservation 取代全进程 fd 扫描，封闭最后一次
  close 与延迟清理登记之间的竞态。

- 为每个 inode 建立独立页树和弱 `mm` 反向索引，使 write/truncate 只访问真正映射
  该文件的地址空间，不再扫描所有进程。

## 对应提交

- 核心修复：`491ed4c`、`1d3b44b`、`04bf218`。
- 回归与集成：`b3b4cce3`、`083fa837`；文档：`1c8f6670bf6767750bd893772f6c0eacc1a1e56a`。

## 对比提升

并发回归全部通过；并行构建从约 11 个 crate 后停住推进到 96 个不同 `Compiling`
阶段。

以下是 AI 的具体分析，作为存档。

---

## 历史分析背景

本批次涉及等待/唤醒原语、inode 引用计数和文件页缓存三个子系统，修改量大且与调度器和
内存管理交叉。下面保留完整的设计推导、Linux 对照、回归测试命令和 tg-xtask 进度记录。

## 1. 结论

本批次没有运行完整 BuildStorm，只围绕其前置工具 `tg-xtask` 的真实停顿继续定位，
并完成三组可复用的 Linux 式并发修复：

1. 增加 `PreparedWait`，把“发布 Blocked 状态”和最终条件检查组成不可丢唤醒的
   prepare-to-wait 协议，迁移 wait4/waitid、vfork、内核 WaitQueue、ppoll、epoll 和
   fd-less 无限 pselect 等路径；
2. 用 inode 级打开描述计数替代 unlink 时遍历 `PID2PCB -> files_struct -> fd`，并用
   reservation 封闭最后一次 close 与兼容 rename 的竞态；
3. 将普通文件页缓存从一个全局 `(dev, ino, page)` 树拆成 inode mapping + inode 内
   page tree，并增加 inode 到弱 `mm` 引用的 mmap 反向索引，普通文件 write/truncate
   不再遍历所有进程地址空间。

这些改动分别对应 Linux 的 wait queue、`struct file`/inode 引用生命周期，以及
`address_space::{i_pages,i_mmap}`。实现没有逐行复制 Linux，也没有引入 RCU、XArray
或完整 VMA interval tree；当前规模下使用 irq-save、任务局部 transition lock、
短持有 spin lock 和弱引用实现相同的关键同步边界。

聚焦验证结果：

- LoongArch 12 hart guest 中，wait/pidfd 256 次、ppoll/epoll 128 次，以及
  open-unlinked 6 × 32 次并发回归全部通过；
- hard-link 多 dentry 的延迟清理、跨 mm 共享映射、truncate/EOF、内核缓冲 write、
  lazy file fault、eventfd/timerfd/epoll_ctl 回归全部通过；
- RISC-V64 与 LoongArch64 内核 `cargo check` 通过；新增和相关用户回归的双架构
  `cargo check` 通过；`git diff --check` 通过；
- 一次干净 snapshot 的并行 `cargo build -p tg-xtask` 记录到 96 个 crate 进入
  `Compiling`，越过了旧现场的 waitid 丢唤醒和 unlink 全局 fd 扫描停顿，但在验证
  上限内仍未成功退出。

因此本批次的严格结论是：已消除三个经源码和运行现场确认的全局串行/丢唤醒点，
但 **tg-xtask 与完整 BuildStorm 尚未通过，不能报告端到端加速比或比赛得分**。

## 2. 版本与不可变资产

本批次从下列基线开始。运行态验证发生在分类提交前，但对应的最终代码已按第 10 节
提交；提交过程只拆分职责和删除短暂的兼容入口，没有改变已验证的运行语义。

| 资产 | 值 |
| --- | --- |
| 顶层基线 | `dc3d813726e02352db214a63ee99bced4c6576a0` |
| `os/` 基线 | `2352f140e33a67f3b3bbf6bec529acd13563d6de` |
| `os/` 最终 revision | `04bf218113d8f4045db9750f751ab5fd58fbcc22` |
| 顶层回归提交 | `b3b4cce328cce7552d2be9f5715d9a4e06213df9` |
| 顶层内核集成提交 | `083fa83772460388f38d1c1def303ed585969806` |
| final 测试分支/commit | `final-2026` / `1eac61d3becaa592c8ef12a7535f0ec6bb9e3e36` |
| 本地 Linux 参考树 | `fc02acf6ac0ccde0c805c2daa9148683cdd01ba8` |
| QEMU | 11.0.3 |
| guest | LoongArch64，12 vCPU，8 GiB |
| 根镜像 | `sdcard-la-pub.img`，14 GiB raw ext4 |
| 镜像 SHA-256 | `2ad9d955684297abe9db48d94f1b7fcc488268fc8f481408c55b1ec27f520c6a` |
| image mode | QEMU `-snapshot` |

2026-08-03 收尾时重新执行了 `shasum -a 256 sdcard-la-pub.img`，结果与资产记录一致。
所有 guest 写入由 snapshot 丢弃，没有修改基准镜像。final 测试源码未拉取或更新。

## 3. 失败链路与根因

### 3.1 第一处：waitid(P_PIDFD) 丢失子进程退出唤醒

最初的单 job 诊断运行：

```sh
cd /work/tgoskits
timeout 300 cargo build -p tg-xtask -j1
```

第一个 rustc 子进程已经退出并成为 Cargo 的 zombie child，但 Cargo 长时间没有 reap。
手工向 Cargo 发送 SIGCHLD 后，它立即 reap 并开始下一个 crate；后续子进程可重复该
现象。镜像中的 Rust `PidFd::wait()` 使用：

```text
waitid(P_PIDFD, ..., WEXITED)
```

旧 `syscall_waitid()` 的顺序是：检查 child、加入 parent wait queue、释放 PCB lock，
再调用通用 block。若 waiter 在最后检查后被 timer 抢占，child exit 可以在它仍是
Running/Ready 时完成一次 wake；waiter 恢复后仍执行 block，从而睡在已经成立的条件
之后。

完整原始诊断记录：

```text
.tmp/final-runs/20260803-current-tg-xtask-focus/metadata.md
.tmp/final-runs/20260803-current-tg-xtask-focus/serial-j1.log
```

### 3.2 第二处：每次 unlink 扫描所有进程和 fd

wait 修复后，编译能够继续创建进程，但并行现场再次长时间无进度。QEMU monitor 在
12 个 hart 中反复观察到：

```text
CPU 0/2/3/4/5/6: PC 0x801d0104
CPU 1:             PC 0x80299394
```

使用该次 kernel ELF 解符号后，热点落在 `defer_unlink_open_file()` 与
`has_open_inode_view()`。旧实现为了判断一个 ext4 inode 是否仍被打开，每次 unlink
都执行：

```text
PID2PCB
  -> 每个 process
     -> files_struct
        -> 每个 fd
           -> downcast OSInode 并比较 (dev, ino)
```

Cargo/rustc 会高频创建、rename、unlink 临时文件；多核并发时该算法既是
`O(processes × fds)`，又不断争用 PCB 和 fd-table 锁，最终把独立编译任务重新串到
一个全局扫描点。

原始 monitor 现场：

```text
.tmp/final-runs/20260803-152507-loongarch64-shell/serial.log
```

### 3.3 第三处：全局文件 page tree 与全进程 mmap 广播

已有 Linux 式普通文件 page cache 解决了重复读取大型 Rust DSO，但内部仍只有一个：

```text
Mutex<BTreeMap<(dev, ino, page), Slot>>
```

不同 rustc 对互不相关的源文件、目标文件和 DSO 做 page lookup/write/resize 时都竞争
同一把锁。更严重的是，普通文件每次 write 或 size 变化都会扫描 `PID2PCB`，依次尝试
锁住每个进程的 `mm`，即使该进程从未 mmap 目标 inode。文件增长时，旧 resize 还会
扫描全系统所有 cache key。

这不是 Linux page cache 的组织方式。Linux 的 page tree 和 mmap reverse map 都属于
单个 `address_space`，无关 inode 不共享 page-tree 锁，truncate/unmap 通过 `i_mmap`
查找实际映射该文件的 VMA。

## 4. Linux 对照

本次直接阅读本地 `exampleOs/linux`，主要对应关系如下：

| 语义/机制 | Linux 参考 | 本次实现 |
| --- | --- | --- |
| waiter 入队并发布 task state | `kernel/sched/wait.c:prepare_to_wait()`、`prepare_to_wait_event()` | `PreparedWait` + wait queue/条件锁 |
| poll 最终 scan 前发布睡眠状态 | `fs/select.c:poll_schedule_timeout()` | ppoll register → arm → final scan → sleep |
| epoll 在锁下 final availability check | `fs/eventpoll.c:ep_poll()` | epoll waiter register 后 arm，再做 final readiness scan |
| unlink 后 inode 独立于 pathname 生存 | `fs/namei.c:vfs_unlink()`、`filename_unlinkat()` | open description 计数 + 延迟兼容清理 |
| 最后 struct file 引用释放 | `fs/file_table.c:__fput()` | 最后一个 `Arc<OSInode>` Drop |
| inode 引用归零后回收 | `fs/inode.c:iput()` | inode lifetime state 归零后清理 hidden names |
| inode 局部 page tree | `include/linux/fs.h:address_space::i_pages`、`mm/filemap.c` | `FilePageCacheMapping::pages` |
| 文件到映射它的 VMA 反向索引 | `address_space::i_mmap`、`mm/memory.c:unmap_mapping_*()` | inode 局部 `Vec<WeakMmRef>` |

Linux 的 `prepare_to_wait()` 特意在 wait-queue spinlock 下先加入 entry，再调用
`set_current_state()`；注释明确指出该顺序和 memory barrier 用于保证 waker 至少看到
“wait entry 已存在”或 waiter 在后续检查中看到事件。本实现使用更保守的 REF-walk
式锁协议：本地关中断、任务 transition lock 和 `wakeup_pending`，没有复制 Linux 的
memory-order 快路径。

## 5. 实现

### 5.1 `PreparedWait`

`os/src/task/processor.rs` 新增一次性 token：

```text
持有条件锁/注册锁
    -> LocalIrqSaveGuard
    -> task transition lock 下 Running -> Blocked
    -> 释放条件锁
    -> 最终条件检查已完成
    -> consume wakeup_pending，或提交 scheduler block
```

关键性质：

- 本地 timer 不能插入“最终检查完成、尚未提交 Blocked”的窗口；
- 远端 waker 看到 task 仍在 CPU 上时，将 wake 记录到 `wakeup_pending`；
- token 在 ready/error 快路径被 Drop 时恢复 Running，并取消该 token 已经观察到的
  pending wake；
- 真正切换到 idle 前后已有的 `pending_blocked`/`on_cpu` 协议继续负责远端 wake 的
  最终入队，不会让同一个内核栈同时在两个 hart 运行。

迁移范围：

- `WaitQueue::wait_until()`；
- `wait4()`、`waitid()`；
- `CLONE_VFORK` parent wait；
- `ppoll()`、`epoll_wait()`；
- `pselect6(nfds=0, timeout=NULL)` 的信号等待。

### 5.2 inode 打开描述生命周期

`os/src/fs/inode.rs` 增加按 `(device_id, inode_num)` 索引的：

```text
InodeLifetimeState {
    open_descriptions,
    unlink_reservations,
    deferred_cleanups[],
}
```

计数对象是 `OSInode`，也就是本项目的 open file description，而不是 fd slot：

- dup/fork 共享同一 `Arc<dyn File>`，不会重复计数；
- SCM_RIGHTS、epoll 或其他内核引用即使没有 fd slot，也会保留该 description；
- 最后一个 `OSInode` Drop 才触发延迟清理。

unlink 先取得 reservation，再在不持有 lifetime spin lock 时执行 ext4 rename；成功后
发布 cleanup，失败则 Drop reservation。这样最后一次 close 不可能落在“已决定延迟、
但 cleanup 尚未登记”的缝隙中。

一个 inode 可以有多个 hard-link dentry，所以 cleanup 不是 `Option`，而是 `Vec`。
回归专门在 fd 打开时 unlink 两个 hard-link 名称，并在最后 close 后要求父目录可被
rmdir，以捕获只保留一个 cleanup 导致的 hidden-name 泄漏。

### 5.3 inode 局部 page cache

`os/src/mm/memory_set/backing.rs` 现在使用两层结构：

```text
短持有全局表: (dev, ino) -> Arc<FilePageCacheMapping>

FilePageCacheMapping {
    pages: Mutex<BTreeMap<page, Loading|Ready>>,
    mmap_mms: Mutex<Vec<WeakMmRef>>,
}
```

全局锁只解析 inode mapping；页 lookup、single-flight 发布、write、truncate 和回收在
inode 局部锁内完成。resize 使用 inode-local `split_off(remove_from)`，不再遍历其他
inode 的页面。

原有 Loading/Ready single-flight、EOF 页尾清零、超出 EOF 页失效以及 clean-unused
reclaim 语义保持不变。

### 5.4 mmap 弱反向索引

普通文件 mmap 成功后，把当前 `MmRef` 的弱引用注册到 inode mapping；fork/exec 创建
新 `MmRef` 时也从继承/新建 VMA 集合注册涉及的 inode。后续 fd write 或 i_size 更新：

1. 先更新该 inode 的 page cache；
2. snapshot 该 inode 的 weak-mm 列表；
3. 只锁仍存活且确实映射该 inode 的 mm；
4. 同步 MAP_SHARED resident alias、`file_valid_len`、EOF/SIGBUS metadata。

弱引用不延长进程地址空间生命周期；失效 weak entry 在注册或读取时清理。unmap 后但
mm 尚存活的 entry 可以暂留，真正更新会在 VMA 过滤阶段成为 no-op。

## 6. 聚焦运行态回归

运行入口等价于：

```sh
ARCH=loongarch64 MEM=8G SMP=12 IMAGE_MODE=snapshot ./run.sh shell
```

### 6.1 wait 与 open-unlinked

日志：

```text
.tmp/final-runs/20260803-171647-loongarch64-shell/serial.log
```

结果：

```text
WAIT_WAKEUP_RACE_PASS iterations=256
WAIT_WAKEUP_RC=0
OPEN_UNLINK_LIFETIME_PASS workers=6 iterations=32
OPEN_UNLINK_RC=0
private_file_page_cache_smoke passed
shared_file_alias_smoke passed
private_file_madvise_dontneed_smoke passed
VFS_EXIT_FS_ZOMBIE_PASS
```

wait regression 混合 child 立即退出、yield 后退出和短 timer 后退出，同时覆盖
`waitid(P_PIDFD)` 与 wait4；这正对原 Cargo pidfd 丢唤醒窗口。

### 6.2 mmap reverse index 与 resize

日志：

```text
.tmp/final-runs/20260803-171949-loongarch64-shell/serial.log
```

结果：

```text
shared_file_cross_mm_smoke passed
shared_file_truncate_cache_smoke passed
shared_file_kernel_write_smoke passed
file_mmap_lazy_fault_smoke passed
```

覆盖跨进程 MAP_SHARED 可见性、truncate/扩展/EOF、kernel-buffer write，以及普通文件
lazy fault。

### 6.3 poll/epoll 丢唤醒

日志：

```text
.tmp/final-runs/20260803-181055-loongarch64-shell/serial.log
```

结果：

```text
POLL_WAKEUP_RACE_PASS ppoll=64 epoll=64
POLL_RACE_RC=0
eventfd_epoll_smoke passed
EVENTFD_EPOLL_RC=0
timerfd_epoll_smoke passed
TIMERFD_EPOLL_RC=0
epoll_ctl_wakeup_smoke passed
EPOLL_CTL_WAKE_RC=0
WAIT_WAKEUP_RACE_PASS iterations=256
WAIT_RACE_RC=0
```

## 7. `tg-xtask` 聚焦进度

按用户要求，没有执行 `buildstorm_testcode.sh`，也没有进入正式的
`cargo xtask arceos build -p arceos-helloworld` 阶段。

### 7.1 旧现场

prepare-to-wait 修复前：

- default parallel 在最初约 11 个 crate 后停住；
- `-j1` 可观察到 rustc zombie，Cargo 卡在 `waitid(P_PIDFD)`；
- 手工 SIGCHLD 能让 Cargo 继续，证明不是 rustc 本身仍在运行。

### 7.2 修复后的有限窗口

一次干净 snapshot 只执行：

```sh
cd /work/tgoskits
timeout 1800 cargo build -p tg-xtask
```

串口日志：

```text
.tmp/final-runs/20260803-162551-loongarch64-shell/serial.log
```

该次运行有 96 行不同构建阶段的 `Compiling` 记录，已经进入 `aws-lc-sys`、`ring`、
`tokio`、`rustix`、ICU data 和 `libz-sys` 等依赖；未再停在 waitid 或 unlink fd scan。
但命令在允许窗口内没有返回，guest timeout/Ctrl-C 也未能及时收尾，最终从宿主侧结束
该 snapshot。没有成功退出码，因此不能标记 `tg-xtask` 通过。

不同 crate 的构建成本差异很大，前后窗口也不是严格同一 host 负载，所以“11/19 到
96 个 Compiling 记录”只能证明进度越过旧串行点，不能换算成吞吐提升百分比。mmap
弱反向索引是在这次窗口之后完成的，只做了精确语义回归，尚未用 tg-xtask 再测。

镜像中没有 `iozone`、`lmdd` 或 `bw_file_rd`：

```sh
command -v iozone
command -v lmdd
find /glibc /musl /bin /usr/bin -maxdepth 3 -type f \
  \( -name 'iozone*' -o -name 'lmdd*' -o -name 'bw_file_rd*' \)
```

均无输出。因此没有伪造 iozone 数据；本轮唯一真实重文件 I/O workload 是上述
`tg-xtask` 聚焦构建。

## 8. 静态验证

最终代码执行：

```sh
TMPDIR=$PWD/.tmp/check-tmp \
CARGO_TARGET_DIR=$PWD/.tmp/check-riscv \
ARCH=riscv64 cargo check --quiet --offline \
  --manifest-path ../os/Cargo.toml \
  --target riscv64gc-unknown-none-elf

TMPDIR=$PWD/.tmp/check-tmp \
CARGO_TARGET_DIR=$PWD/.tmp/check-loong \
ARCH=loongarch64 cargo check --quiet --offline \
  --manifest-path ../os/Cargo.toml \
  --target loongarch64-unknown-none-softfloat
```

两项退出码均为 0。相关用户回归同时对
`riscv64gc-unknown-none-elf` 与 `loongarch64-unknown-none` 执行 `cargo check`，退出码
均为 0。仓库现有 warning 保留，没有新增编译错误。

所有本批 Rust 文件已格式化。workspace 级 rustfmt 曾触及 vendor 的纯格式差异，收尾
时已逐项撤回；最终 `git status` 中没有 vendor 修改。以下检查通过：

```sh
git -C ../os diff --check
git -C .. diff --check
```

## 9. 明确边界与下一步

1. ext4-fs 尚不能让零链接 inode 仅靠内存引用生存，所以 open-unlinked 仍使用
   `.ltp_orphan.*` hidden rename 兼容层。最后 close 生命周期正确，但打开期间
   `st_nlink`/目录内部可见性还不是完整 Linux dentry/inode 语义；最终应由统一 VFS
   inode 生命周期替代 hidden name。
2. `FILE_PAGE_MAPPINGS` 暂不主动删除空 inode mapping；weak mm 会清理，但空 mapping
   可能留下少量元数据。下一步应在 page tree、weak list 都为空时安全回收 mapping。
3. mmap reverse index 以 mm 为粒度，不是 Linux 按 file offset 排序的 VMA interval
   tree；更新仍需在命中的 mm 内过滤 VMA。它消除了全进程扫描，但超大单 mm 的范围
   更新仍有优化空间。
4. finite pselect 和少数尚无 poll waiter 注册能力的 file type 仍沿用定时重试；本批
   没有宣称全部等待原语已经统一。
5. timed poll token 的 timer 当前不可取消；若事件先到，后续 timer 可能产生一次无害
   的额外 wake。后续可引入可撤销 timer handle，减少虚假唤醒。
6. 没有完整 BuildStorm、BuildStorm judge、完整 LTP、完整 CAgent 或 RISC-V QEMU
   runtime 结果。下一步应先在无其他重负载的宿主环境重跑干净 tg-xtask；它成功退出
   后，才能按顺序运行最小 Cargo、完整 BuildStorm 和相关 LTP 回归。

## 10. 提交记录

修改已按职责拆分，`os/` 每个提交均保留可构建的依赖顺序：

| 仓库 | commit / 标题 | 内容 |
| --- | --- | --- |
| `os/` | `491ed4c sched: make condition waits wakeup-atomic` | PreparedWait 与 wait/poll/epoll/vfork 迁移 |
| `os/` | `1d3b44b mm: shard file mappings by inode` | inode page tree 与 weak mmap reverse index |
| `os/` | `04bf218 fs: track open inode lifetimes locally` | 删除全局 fd/mm scan、reservation、多 hard-link cleanup |
| 顶层 | `b3b4cce3 test: cover filesystem and wait concurrency` | 三个新 smoke 与相关 bin 注册 |
| 顶层 | `083fa837 kernel: integrate wait and inode concurrency fixes` | 更新 `os` revision |
| 顶层 | 本提交：`docs(final): record BuildStorm concurrency follow-up` | 本报告 |

各提交的验证说明应引用第 6、8 节，并明确写出完整 BuildStorm 未运行。

## 11. AI 使用说明

AI 用于阅读本地 Linux wait queue、eventpoll、VFS file/inode lifetime 和 address_space
实现，分析 QEMU monitor/串口现场，设计并实现并发协议，编写聚焦回归，执行用户授权
的有限验证并整理报告。

没有修改 judge、评分常量、`/proc/uptime` 或基准镜像；没有按测试名硬编码返回值；
没有把超时/被终止的 tg-xtask 或未运行的完整 BuildStorm 记录成通过。
