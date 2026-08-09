# 8-1 筛选并集成 Linux 式内核并发修复，消除八核 CAgent 停顿

## 问题概述
CAgent 执行过程中死锁 

## 如何发现

QEMU 监视器显示 7 个核心都在等同一个 inode 写信号量；`debugfs ncheck` 把它定位到
只读  libc
方法： info registers -a 拿到 7 个 hart 的 pc，落在同一个地址区间。 2. 用内核 ELF 把地址翻成符号（nm/addr2line os/target/<arch>/release/os），看到都在读写信号量的等待循环里。 3. 锁对象的地址在寄存器或栈上，xp 把那块内存 dump 出来，读出里面的 inode 号——一个裸数字。这才需要 debugfs ncheck 反查成 libc.so.6。

代码审查随后确认：读缺页等块 I/O 时，退出路径的
`OSInode::drop()` 无条件抢写锁；

进程控制块锁与内存空间锁；读取
`/proc/meminfo` 时，`Committed_AS` 扫描全部进程和虚拟内存区域，又放大了等待。

>[!TIP] 
> Committed_AS 是 /proc/meminfo 里的一个字段，AS 指 address space。含义是：系统当前已经"承诺"给所有进程的内存总量（单位
>  kB）——如果每个进程把它申请到的可写私有内存全部真正写一遍，需要多少物理内存加 swap。
>  
>  关键点是它记的是承诺而不是占用：
>  
>  - 记账发生在映射建立时（mmap、brk），不是首次访问时。所以它和 RSS（实际驻留）是两回事，通常远大于 RSS。
>>  - 因为是承诺，它可以超过物理内存总量——这就是 overcommit（超额分配）。Linux 默认允许，因为大多数进程申请了不会全用。
>  - 只有"可能需要独占物理页"的映射才计入：私有可写映射、堆、共享匿名/shmem。只读文件映射不计入（缺页时读的是 page
>  cache，多个进程共享同一份），MAP_NORESERVE 也不计入。
  


串口日志还显示 10 个测试其实都已完成，只因 stdout 按字节交错，judge 仅识别出
5 个完整结果行。
## 那把 inode 写信号量是什么

每个 `OSInode` 持有 ext4 inode 的读写信号量 `self.inode_lock`（可睡眠，对应 Linux 的 `i_rwsem`）。文件映射缺页读取时走 `pread_at`，只读描述取的是读锁：

```rust
// os/src/fs/inode.rs:970
pub fn pread_at(&self, offset: usize, buf: &mut [u8]) -> usize {
    if self.writable {
        let _inode_guard = self.inode_lock.write();
        return self.pread_at_locked(offset, buf);
    }
    let _inode_guard = self.inode_lock.read();
```

调用点在缺页路径 `os/src/mm/memory_set/fault.rs:800`，读锁一直持到 ext4 块 I/O 返回，中间会睡眠让出 CPU：

```rust
match file_page_cache_get_or_load(dev, ino, file_page, |page| {
    let _ = os_inode.pread_at(plan.file_off, page);
}) { ... }
```


## 无条件抢写锁的退出路径

`OSInode::drop()` 是这样的：

```rust
 impl Drop for OSInode {
     fn drop(&mut self) {
-        let inode_key = {
-            let _inode_guard = self.inode_lock.write();   // 无条件
-            let mut inner = self.inner.lock();
-            let inode_key = (inner.inode.device_id(), inner.inode.inode_num());
-            if !inner.write_buf.is_empty() { ...回写... }
-            inode_key
-        };
```

inode 号只是从 `inner` 里读两个字段，却为此取了写锁；只读文件的 `write_buf` 永远是空的，写锁纯属多余。修复后先判断再取：

```rust
// os/src/fs/inode.rs:1755
let inode_key = (self.inode_device_id, self.inode_num);
let has_pending_write = self.writable && !self.inner.lock().write_buf.is_empty();
if has_pending_write {
    let _inode_guard = self.inode_lock.write();
    ...
}
```

同一提交还把回写提前到 `close` 阶段（`inode.rs` 的 `File::close`，`if last_fd && self.writable { flush_with_error() }`）。原因写在提交注释里：`Drop` 可能在 idle 清理路径上跑，那里没有 current task，根本不能在 inode 信号量上睡眠——一个不能睡眠的上下文去抢一把要睡眠等待的写锁，就是死锁。

`libc.so.6` 之所以是受害者：它是只读的、被每个动态链接程序映射，读者极多；任何一个进程退出关闭它，都会往这把读写锁里插一个写者，把后续所有读者一并挡住。

## 锁序反向的两处

`c58f935` 之前，MemorySet 的锁是 `spin::Mutex`，而且几乎所有路径都是"先拿 PCB 锁，再在 PCB 锁里拿 mm 锁"：

```rust
// mmap.rs 修复前
let inner = process.borrow_mut();          // PCB ticket lock，不可睡眠
...
let mut memory_set = inner.memory_set.lock();   // 之后可能等 inode rwsem
```

```rust
// procfs/content.rs 修复前
let inner = proc.borrow_mut();
let regions = { let memory_set = inner.memory_set.lock(); ... };
```

而私有文件映射在 mm 锁里要等 inode 读写信号量。于是形成：PCB 自旋锁 → mm 锁 → inode 信号量。持 inode 读锁的那个缺页正在等磁盘，要靠调度器唤醒才能推进；可其他核心正拿着不可睡眠的 PCB 自旋锁死等——自旋锁不会让出 CPU，owner 就永远醒不过来。

`c58f935` 的注释把这条讲得很直接：

```rust
// os/src/task/process_block.rs:1590
/// This is the local equivalent of Linux `get_task_mm()`: only the short
/// task lookup is protected by the task lock; callers then acquire the
/// sleeping mmap lock independently.  Keeping the monolithic PCB spinlock
/// while waiting for MemorySet can otherwise strand the mm owner and make
/// signal/procfs walkers spin behind the same process.
pub fn memory_set(&self) -> MmRef { self.borrow_mut().memory_set.clone() }
```

两处改动：mm 锁换成可睡眠的 `KernelMutex`（`mm_ref.rs` 的 `use spin::MutexGuard;` → `use crate::sync::{KernelMutex, KernelMutexGuard};`）；调用方只在 PCB 锁里克隆一个 `MmRef` 指针就放锁，锁不再嵌套。

## Committed_AS 的放大作用

`c58f935` 删掉的就是原来那个全量扫描：

```rust
 pub fn vm_committed_as_bytes() -> usize {
-    let processes = { PID2PCB.lock().values().cloned().collect::<Vec<_>>() };
-    processes.iter().fold(0usize, |acc, process| {
-        let Some(inner) = process.try_borrow_mut() else { return acc };
-        let memory_set = inner.memory_set.lock();          // 又是 PCB → mm
-        acc + memory_set.heap_size() + memory_set.anon_private_writable_vm_bytes()
-    })
+    MmRef::global_committed_bytes()
 }
```

它同时踩了两个雷：遍历所有进程和 VMA 让临界区变长，且用的正是"PCB → mm"这个错误锁序。现在改成 VMA 变动时增量维护一个全局原子计数，读取是 O(1)：

```rust
// os/src/mm/memory_set/mm_ref.rs:11
static VM_COMMITTED_BYTES: AtomicUsize = AtomicUsize::new(0);
// MmGuard::drop() 里 update_committed_bytes(self.guard.committed_vm_bytes())
```

LTP 的 `overcommit_memory.c`、`max_map_count.c` 都要反复读 `/proc/meminfo`，所以这条路径在测试里是热路径，不是偶发。

## judge 报 5/10 的原因

`Stdout::write` 是逐字节 `console_putchar` 的循环，中间没有任何互斥：

```rust
// os/src/fs/stdio.rs:82
fn write(&self, user_buf: UserBuffer) -> usize {
    let bytes = user_buf.to_vec();
    for &b in &bytes { console_putchar(b as usize); }
    bytes.len()
}
```

多进程并发写终端时，两条 `PASS:` 行会逐字节交错，judge 按整行匹配就只认出 5 条。`d21154e` 加了一把可睡眠的写锁，并且在翻译用户页之前就取：

```rust
// os/src/fs/stdio.rs:16
static STDOUT_WRITE_LOCK: KernelMutex<()> = KernelMutex::new(());

// os/src/syscall/filesystem/io.rs:345
let _stdout_write_guard = if file.as_any().downcast_ref::<Stdout>().is_some() {
    let Some(guard) = Stdout::lock_write(nonblock) else { return err(SyscallError::EAGAIN) };
    Some(guard)
} else { None };
```

## 归纳

四处缺陷是同一个错误的四种形态：慢操作（块 I/O、全量扫描、串口输出）落在了不该持有的锁里面。
综合起来的设计模式可以看成是这样 

```text
持锁：只快照 VMA/PTE/frame 并固定对象
放锁：分配、清零、拷贝、读文件、等 I/O
重新加锁：复核快照未变则继续，变了丢弃重试
```

`fault.rs` 里 `prepare_lazy_fault` / `commit_lazy_fault` 的拆分就是这个模式的实现。

## 怎么解决

所有保留的修复都把慢操作移到锁外，再用下面三步提交结果：

```text
内存空间锁内：快照虚拟内存区域（VMA）、页表项（PTE）、旧物理页（frame），并固定旧对象
锁外：分配、清零、复制或读取文件
重新加锁：确认 VMA/PTE/来源页未变化，再安装；否则丢弃并重试
```

**缺页与页分配。** VMA/PTE 只在锁内快照与复核；free-list（空闲页链表）锁内只取页号。
4 KiB 清零、`Arc<FrameOwner>` 所有者构造和文件读取都在锁外；RISC-V 只让用过该地址空间的远端核心失效这一页的地址转换缓存。

**文件描述符与 ext4 释放。** `DetachedFd` 保存摘下的 fd（文件描述符），`RejectedFd` 保存未能安装的 fd；
通知、mount（挂载点）引用释放和最后一个 `Arc<File>` 析构都在表锁外。只有最后一个可写描述在 close（关闭）阶段刷新缓冲；只读文件不取 inode 写锁。

**中断安全。** frame free-list 与 heap shard（内核堆分片）的元数据自旋锁保存并关闭本地中断，
释放锁后恢复中断，避免同一核心的分配器中断重入。

**锁序与 procfs。** `MmRef` 改用可睡眠的 `KernelMutex`；进程控制块锁只固定它，不再等内存空间锁。
虚拟内存区域变化时直接更新 O(1) 的全局 `Committed_AS`；procfs（`/proc` 虚拟文件系统）不再扫描全部进程。

**终端输出。** 翻译用户页以前先取得可睡眠的 console 写互斥锁，让一次用户
`write()` 完整输出；非阻塞竞争返回 `EAGAIN`。

**Linux 对照与边界。** 参考 `do_cow_fault()/finish_fault()`、`file_close_fd_locked()/filp_close()`、
`get_task_mm()`、`__fput()`、ext4 release 和 `tty_write_lock()`。CongCore 没有完整 task-work、
每处理器计数器、可并发读的 `mmap_lock` 或终端行规程，只保留相同的锁边界与对象交接。
调度原子快照、heap magazine 和早期终端自旋锁未过停顿或性能门槛，因此没有合入。

## 对应提交

- 第一阶段：`72ee89b`（内存管理）、`9b8059d`（文件描述符生命周期）。
- 最终锁序修复：`ac12c47`（分配器）、`c58f935`（内存空间与 procfs）、
  `f169acc`（ext4 文件释放）、`d21154e86fb50798f8413b4abd37e2835d2168b3`
  （终端写入）。
- 顶层集成：`73b3d980`；最终专题文档提交：
  `b766dae83e2a96af6b15cca10409d25948d95818`。

## 对比提升

最终 CAgent 在 RISC-V 8 核、8 GiB、快照模式下 10/10 完成，单项 1566--3715 ms；
混合 IOZone 三轮中位数为 66.86 s。相对 7 月 31 日同工作负载的 76.23 s 集成基线
缩短 12.29%，但该数字跨越多批修复，只能作为累计状态，不能归因给某一个提交。

以下是 AI 的具体分析，作为存档。

---

## 历史分析背景

> 更新说明：第 1--9 节保留截至内核 `eb4b543` 的初始候选筛选和失败现场。
> 后续继续追踪了其中记录的 8-hart 卡顿，并在 `ac12c47`--`d21154e` 完成修复。
> 第 10 节是最终结论；它取代第 1、5、7.4、7.5 节中“TTY 暂不合入”和
> “残余 SMP 卡顿尚未解决”的阶段性结论。

## 1. 结论

本批次审查了 `final-perf-concurrency` 工作树中的并发优化，并以 Linux 的用户
可观察语义、锁边界和对象生命周期作为标准，将通过静态检查与聚焦回归的修改
按职责拆分后集成到主 `os/dev_final`。最终从候选工作树保留两组修改：

| 主线提交 | 范围 | 结论 |
| --- | --- | --- |
| `72ee89b` | frame allocation、COW/lazy fault、RISC-V active-mm TLB shootdown | 保留；耗时页初始化和文件读取移出 mm 全局锁，提交时重验 PTE/VMA。 |
| `9b8059d` | fd close/install/replace 生命周期 | 保留；表锁内只摘除，通知、mount ref 释放和对象析构在锁外完成。 |

主线原有的 `1ed50a2` IRQ-driven 多请求 VirtIO 和 `5a3d3e2` ext4 inode
读写锁保持不变，不属于本次从候选工作树移植的内容。

以下候选没有进入主线：

- 工作树中的轮询 VirtIO：主线已有 `1ed50a2` 的 IRQ-driven 多请求实现，不能
  再叠加一套 token/完成生命周期，也不应从中断完成退回同步轮询。
- scheduler 全局 runnable 原子快照：两版均在线程 hackbench 中稳定停滞；缩减
  到只统计 RT runnable 后，对普通 workload 没有机制上的广泛收益，烟测还出现
  process-pipe 约 3.2 倍异常回退。
- central buddy + per-hart magazine：首个完成的 5-sample workload 中位数由
  核心组合的 64.17 秒变为 75.60 秒，回退约 17.8%，超过预设 5% 上限；同时
  当前 per-hart cache 没有完整实现 Linux 的 migration/IRQ-safe fast path。
- TTY write 串行化：初始筛选先后试验 scheduler-aware mutex 和 spin lock；前者
  被当时尚未定位的 fput/mm 卡顿干扰，后者会把用户页翻译和串口输出包在不可
  睡眠临界区内，因此 `3523361` 先移除了它。后续修复根因后，`d21154e` 按
  Linux `tty_write_lock()` 的边界重新实现并通过标准 CAgent，详见第 10 节。

另有一次 `5db1332` 将 IRQ block completion 的协作让出改成纯轮询，用来验证
“持有 ext4 锁时调度”假设。源码确认 ext4 已使用可睡眠 `KernelRwSemaphore`，
且运行态卡顿没有消失，因此 `eb4b543` 完整恢复原 IRQ 等待路径。该实验不属于
最终实现。

这次筛选坚持两条规则：一是“看起来像 Linux”不等于可合入；二是候选只要在
并发正确性或独立性能门槛中失败，就不靠其他快项抵消风险。

## 2. Linux 对照

参考树为：

```text
exampleOs/linux
fc02acf6ac0ccde0c805c2daa9148683cdd01ba8
```

本地实现复用的是 Linux 的并发原则，不是一比一复制完整子系统：

| 本地问题 | Linux 参考 | 采用的原则 |
| --- | --- | --- |
| 页分配器在全局锁内清零 4 KiB | `mm/page_alloc.c` | free-list 元数据操作和 `prep_new_page()` 类初始化分离。 |
| COW/lazy fault 在 mm 锁内分配、复制或读文件 | `mm/memory.c` 的 `do_cow_fault()`、`finish_fault()` | prepare → lockless work → PTE/VMA recheck and commit。 |
| COW 后向所有核做粗粒度处理 | `arch/riscv/mm/tlbflush.c` 的 `mm_cpumask(mm)` | 只对实际可能使用该 mm 的在线远端 hart 做页粒度 shootdown。 |
| fd 表锁内执行最终 close/drop | `fs/file.c` 的 `file_close_fd_locked()`、`filp_close()` | 锁内摘除 fd，锁外执行可能递归取锁或释放重对象的 close。 |
| 并发 stdout write 字符级交错 | `drivers/tty/tty_io.c` 的 `tty_write_lock()` | 初始 spin 版本不满足边界；后续改为翻译用户页之前取得可睡眠 mutex，并实现 nonblock `EAGAIN`。 |

## 3. 内存管理：缩短页错误临界区

### 3.1 frame allocator 锁外清零

旧 `frame_alloc()` 在 `FRAME_ALLOCATOR` 自旋锁内构造 `FrameTracker`，而构造会
清零整页并分配 `Arc` 控制块。多个 hart 同时 fault 时，所有 4 KiB 清零都被
串成一条全局通道。

现在锁只保护空闲页号的移除：

```text
FRAME_ALLOCATOR.lock().alloc()
                |
                v  release allocator lock
        clear 4 KiB + build Arc owner
```

页号从 free-list 移除后已由当前调用独占，因此锁外初始化不改变所有权语义。

### 3.2 COW/lazy fault 三阶段提交

`MmRef` 的 COW 和 lazy fault 统一采用：

```text
prepare under mm lock
  snapshot VMA/PTE/backing and pin old frame
                  |
                  v
work without mm lock
  allocate + zero/copy, or file pread
                  |
                  v
commit under mm lock
  revalidate VMA/PTE/source PPN, then install
```

commit 发现竞态时丢弃本轮临时页并重试。前三次走乐观路径；持续竞争后只让
retrying fault 通过 `fault_retry_lock` 串行，但仍执行相同的 prepare/work/recheck，
不会把“竞争次数多”误报成无效地址或 `SIGSEGV`。

lazy fault 还显式传入 `charge_pid`。内核替子 mm 处理
`CLONE_CHILD_SETTID` 一类 fault 时，不再把 child 匿名页记到当前 parent 的
cgroup/memory 账上。

### 3.3 RISC-V active-mm 与块 I/O guard

`AsidContext` 增加 active hart mask，语义对应 Linux `mm_cpumask(mm)`：

- 返回用户态前先发布本 hart 的 active bit，再安装/恢复用户 SATP；
- syscall/trap 中继续使用 user SATP，因此不能在 trap entry 过早清 bit；
- PTE writer 读取 active mask，只向在线远端 hart 发送单页
  `remote_sfence_vma()`；
- 若 hart 在 writer 取快照之后才激活，返回用户态前的本地 flush 覆盖该竞态。

主线 VirtIO 的 `KernelPageTableGuard` 可能跨调度等待 IRQ 完成。为避免任务恢复到
另一个 hart 后恢复旧 user SATP 却没有发布 active bit，guard 额外 pin 对应
`Arc<AsidContext>`，恢复 SATP 前在当前 hart 重新发布它。

LoongArch 保留原有 ASID/TLB 路径；本批次没有把 RISC-V SBI shootdown 机制生搬
到 LoongArch IOCSR IPI。

## 4. fd：锁内摘除，锁外完成 close

`FilesStruct` 引入两个 `must_use` 的所有权载体：

- `DetachedFd`：已经从表中摘除，等待锁外 close notification、mount ref 释放
  和最后的 `Arc<File>` drop；
- `RejectedFd`：安装/替换失败时返还输入 file/mount，防止错误路径在表锁内析构。

close、close_range、exec 的 CLOEXEC、dup/replace、socket/pipe 安装回滚、fork
失败回滚和 idle 批量清理均迁移到这一边界。这样即使 file destructor 触发
socket wakeup、fanotify、mount pin 释放或其他内存分配，也不会递归占用
`FilesLock`。

## 5. TTY 候选：初始审查后移除

Linux `tty_write_lock()` 说明一次 userspace write 应作为 TTY 串行单位，但本地
实现不能只增加一把全局锁：

- `UserBuffer` 保存未 pin 的用户页切片，翻译之后等待可睡眠 mutex 会跨调度保留
  可能失效的裸切片；
- 翻译之前取得 spin lock，又会把可能 fault、复制用户页和字符输出的长路径放进
  不可睡眠临界区；
- scheduler-aware mutex 版和 spin 版均未给出稳定的 8-hart CAgent 证据。

因此 `33fcb6b`/`717ff5e` 仅作为审查实验保留在提交历史中，`3523361` 在初始
筛选阶段移除了 stdout 串行行为。若后续要实现 Linux 风格 TTY，
应先保证等待发生在用户页翻译之前，再用可睡眠锁覆盖单次 write。后续
`d21154e` 采用了这一较小边界：先取得 console write mutex，再翻译用户页并完成
整次输出；当前只有一个 console 对象，因此暂不引入完整 per-tty 层。最终验证见
第 10 节。

## 6. 候选筛选证据

性能门槛使用五类 workload 的 5 次中位数，越低越好：

```text
hackbench: process/socket, process/pipe, thread/socket, thread/pipe
lmbench:   lat_proc fork, -P 8 -W 1 -N 5
```

候选要求：五项中位数的几何平均至少提升 5%，且任一单项不得回退超过 5%。
`tools/analyze_concurrency_focus.py` 固化了这一判据。

### 6.1 核心组合观测值

筛选阶段 `33fcb6b` 的 5 次 guest `/proc/uptime` 中位数为：

| workload | 中位数 |
| --- | ---: |
| process/socket | 64.17 s |
| process/pipe | 68.20 s |
| thread/socket | 59.53 s |
| thread/pipe | 59.76 s |
| lat_proc fork | 97.13 s |

这些数字只用于同一宿主现场下拒绝 scheduler/heap 候选，不代表最终保留 TTY
修改，也不作为 Linux 或比赛机的绝对性能基线。

### 6.2 scheduler 拒绝原因

原候选让 fair/RT runnable 汇总和 least-loaded placement 读取原子计数，减少依次
锁全部 runqueue。该思路类似 Linux `rq->nr_running`，但本地计数与现有队列、
迁移和节流的边界还没有判断周全：

1. 原版在线程 socket hackbench 第 2 次稳定停滞；
2. 增加“计数为零时回查权威队列”后，线程 socket 第 1 次仍停滞；
3. 缩减为仅 RT 计数后，普通任务没有预期广泛收益，烟测 process-pipe 从核心
   中位数 68.20 s 异常到 220.77 s，并且 750-task 压力长时间未完成。

因此没有把计数快路径带入主线。

### 6.3 heap magazine 拒绝原因

候选用完整 central buddy 保留 512 MiB 容量，再给每 hart 的 8 B--16 KiB 幂次
class 提供 intrusive magazine，批量 refill 16、每 class 上限 64、OOM 前 drain。
它修复固定 shard 容量孤岛的方向是合理的，但独立门槛的第一项已经确定失败：

```text
core process/socket median: 64.17 s
heap process/socket median: 75.60 s
regression:                 17.81%
```

此外 idle cleanup 在开中断时可能触发 allocator，而 per-hart cache 使用普通
自旋锁。本项目若继续实现该方向，至少要像 Linux local/per-CPU fast path 一样
明确 migration/preemption 与 IRQ re-entry 约束，不能仅依赖“临界区很短”。

## 7. 验证记录

### 7.1 资产

- 初始筛选 kernel HEAD：`eb4b54391daf8c1053f30756871d628b95e66f3a`
- 后续修复 kernel HEAD：
  `d21154e86fb50798f8413b4abd37e2835d2168b3`
- final test source：本地 `final-2026`，
  `1eac61d3becaa592c8ef12a7535f0ec6bb9e3e36`
- 先前只读检查到 remote `final-2026` 为
  `b5ec6ef8497e1818cbdec3b54bb722f036e57972`；按规则没有自动拉取或改基线
- RISC-V image SHA-256：
  `d899fe43d333d1d17ad8a5f8a8b74b68117b8c1ceacfc3843bfeadb1ca705bd1`
- QEMU：11.0.3

### 7.2 静态检查

主工作树最终 HEAD 的以下两架构均通过；现有 warning 不计为本批次失败：

```sh
TMPDIR=$PWD/.tmp cargo check --offline --locked \
  --manifest-path ../os/Cargo.toml \
  --target riscv64gc-unknown-none-elf
TMPDIR=$PWD/.tmp cargo check --offline --locked \
  --manifest-path ../os/Cargo.toml \
  --target loongarch64-unknown-none-softfloat
git -C ../os diff --check
```

### 7.3 并发压力与 LTP

筛选组合 `33fcb6b` 在 RISC-V 8 hart、2 GiB、snapshot 下完成：

- 400 process task hackbench，200 messages/sender：105.996 s，rc=0；
- `fork03/04/05/07/08/09/10`：全部 rc=0；
- `close01/02/close_range02`：全部 rc=0；
- `close_range02` 的 11 个断言全部 TPASS；
- 总结束标记：`CONCURRENCY_FOCUSED_DONE rc=0`。

TTY 候选随后被移除；最终 HEAD 相对已经单独验证过的 `9b8059d` 只有一处 import
的 `rustfmt` 换行，不存在额外运行态逻辑。上述日志可证明 MM/FD 修改通过这组
回归，但不能冒充最终 HEAD 已重新完成同一轮 400-task 压力。

日志：

```text
.tmp/concurrency-runs/core-final-regression-retry.log
```

### 7.4 IOZone（初始诊断，最终结果见 10.6）

为了区分顺序阶段和容易卡住的随机阶段，工具增加
`IOZONE_WORKLOAD=sequential|mixed`。顺序模式执行：

```sh
iozone -t 4 -i 0 -i 1 -r 1k -s 4m
```

提交级隔离结果如下。时间来自 guest `/proc/uptime`，单位为秒：

| kernel | 完成的 run | 结果 |
| --- | --- | --- |
| `5a3d3e2`（集成前） | 16.94 | 第 2 轮已打印表格但未输出结束标记，观察 120 秒后终止。 |
| `72ee89b`（+MM） | 17.58、17.46 | 第 3 轮未完成，观察 120 秒后终止。 |
| `9b8059d`（+FD） | 17.49、17.47、17.56 | 3/3 rc=0，中位数 17.49。 |
| `33fcb6b`（TTY mutex 实验） | 17.44、17.98、17.59 | 3/3 rc=0，中位数 17.59。 |
| `eb4b543`（最终 HEAD） | 无完整 run | 首轮 initial write 为 6714 KiB/s，随后读阶段无进展，短窗口后人工终止。 |

这组数据只支持两个保守结论：

- `9b8059d` 的一次隔离运行解决了三轮 close/析构压力下的稳定完成问题，但最终
  HEAD 仍复现非确定性卡顿，所以不能宣称 FD 修改已经修复全部 SMP/ext4 hang；
- 集成前唯一完整轮次为 16.94 秒，`9b8059d` 中位数 17.49 秒，后者约慢 3.2%；
  没有顺序吞吐提升证据。FD 修改的合入依据是锁边界正确性和聚焦回归，而不是
  IOZone 跑分。

mixed 模式还执行 `-i 0 -i 2`。`5a3d3e2` 在随机阶段超过 288 秒没有完整标记；
`33fcb6b` 曾有单轮 42.85 秒完成，但后续三轮复测又在第一轮随机写收尾处超过
5 分钟。它在同一代码上也不稳定，因此不计算“提升倍数”。

对应日志：

```text
.tmp/iozone/clean-base-sequential.log
.tmp/iozone/clean-mm-sequential.log
.tmp/iozone/clean-files-sequential.log
.tmp/iozone/clean-current-sequential.log
.tmp/iozone/final-head-sequential.log
```

### 7.5 CAgent（初始诊断，最终结果见 10.5）

获得用户授权清理遗留 QEMU 后重新隔离，结果仍然显示 8-hart 非确定性卡顿：

- `9b8059d` 有一次官方 CAgent 完成 10/10、200/200，单项 1.273--2.347 秒；
- 集成前 `5a3d3e2` 的 8-hart 精确基线只完成 factorial 一项（1649 ms），随后
  120 秒硬超时，judge 为 1/10；
- 移除 TTY 锁后的当前语义版本多次在 agent 启动后停滞；xtrace 显示进程已进入
  fork/exec/wait/grep 一带，而不是启动脚本没有运行；
- 同一版本改成单 hart 后很快完成 9/10，仅 fs-readwrite 未通过，进一步指向
  SMP 并发路径而非动态链接器或整套 CAgent 不可运行。

`5a3d3e2` 的对照证明该卡顿早于本次 MM/FD 集成，但最终分支仍会复现，所以不能
把单次 200/200 写成“最终 8-hart 稳定通过”。本批次没有继续扩大范围去修复
ext4/IRQ/scheduler 的残余死锁；这应作为后续独立 P0 诊断。

关键日志：

```text
.tmp/final-runs/20260801-195749-riscv64-cagent/serial.log
.tmp/final-runs/20260801-202517-riscv64-cagent/serial.log
.tmp/final-runs/20260801-201818-riscv64-cagent/serial.log
.tmp/final-runs/20260801-200911-riscv64-cagent/serial.log
```

### 7.6 未运行项目

按用户要求没有运行 BuildStorm，也没有运行 unixbench、libcbench 或完整
IOZone 套件。没有把工具链检查、最小编译或高 CPU 运行状态冒充 BuildStorm
成功。

## 8. 复现

并发性能与回归工具支持分阶段运行，避免每个候选重复执行长压力：

```sh
CONCURRENCY_PHASE=benchmark CONCURRENCY_SAMPLES=5 \
  tools/run_concurrency_focus.sh <workspace> \
  .tmp/iozone/iozone-root.img <benchmark.log>

CONCURRENCY_PHASE=regression CONCURRENCY_SAMPLES=1 \
  tools/run_concurrency_focus.sh <workspace> \
  .tmp/iozone/iozone-root.img <regression.log>

tools/analyze_concurrency_focus.py <baseline.log> <candidate.log>
```

IOZone 工具默认三轮 mixed；提交级隔离和顺序阶段可分别选择：

```sh
IOZONE_RUNS=1 IOZONE_WORKLOAD=mixed \
  IOZONE_SKIP_BUILD=1 IOZONE_KERNEL_ELF=<kernel-elf> \
  tools/run_iozone_focus.sh <workspace> \
  .tmp/iozone/iozone-root.img <iozone.log>

IOZONE_RUNS=3 IOZONE_WORKLOAD=sequential \
  IOZONE_SKIP_BUILD=1 IOZONE_KERNEL_ELF=<kernel-elf> \
  tools/run_iozone_focus.sh <workspace> \
  .tmp/iozone/iozone-root.img <iozone.log>
```

CAgent：

```sh
ARCH=riscv64 SMP=8 MEM=8G IMAGE_MODE=snapshot \
  BOOT_TIMEOUT=900 TEST_TIMEOUT=600 LOG=error ./run.sh cagent
```

## 9. AI 使用说明

AI 用于：比较候选 diff、检索本地 Linux 参考树、推导锁与对象生命周期、生成
聚焦测试工具、执行静态/运行态验证并整理报告。所有保留或拒绝结论均基于本地
源码、可复现日志和实际命令；没有生成伪造 benchmark、硬编码测试名返回值、
修改评分脚本或篡改 `/proc/uptime`。

## 10. 后续 P0 卡顿诊断与最终修复

### 10.1 卡顿根因不是单一的块设备等待

初始筛选结束后，8-hart CAgent 和 mixed IOZone 仍会非确定性停滞。为避免继续
猜测 VirtIO 或 scheduler，使用 QEMU HMP 读取所有 vCPU 的内核栈和锁对象：

- 7 个 hart 同时停在 `KernelRwSemaphore::write()`，调用者是
  `OSInode::drop()`；
- 该 inode rwsem 当时有 2 个 reader 和 7 个 writer waiter；
- 卡住对象的 inode 号是 16243，宿主只读 `debugfs ncheck` 映射为
  `/usr/lib/riscv64-linux-gnu/libc.so.6`；
- reader 是文件映射缺页路径，持有 inode read side 等待块 I/O；退出清理在 idle
  上析构只读 `OSInode`，旧 `Drop` 却无条件请求同一 inode 的 write side。

因此现场是“只读 fput 人为制造 inode writer 队列，并与睡眠中的 file fault
reader 相互阻塞”，而不是 VirtIO 请求没有提交或没有完成。

同时还发现第二条锁顺序问题：若调用者持有 PCB ticket lock 再等待
`MemorySet`，procfs 的全进程扫描又按相反方向取得 PCB/mm，容易让真正的 mm
owner 无法继续运行。旧 `/proc/meminfo` 每次读取 `Committed_AS` 还会遍历所有
进程和 VMA，把这条高风险锁边放大成全局热点。

最后，内核实际完成的 10 个 CAgent 子项曾只被 judge 识别 5 个。串口日志表明
多进程 stdout 在字节级交错，破坏了 `testcase ... pass` 行；这不是测试失败，
而是缺少 Linux TTY 的单次 write 原子边界。

### 10.2 Linux 源码对照

参考树仍是 `exampleOs/linux` 的
`fc02acf6ac0ccde0c805c2daa9148683cdd01ba8`：

| 问题 | Linux 代码 | 本地采用的语义 |
| --- | --- | --- |
| PCB 锁和 mm 锁嵌套 | `kernel/fork.c:get_task_mm()`、`include/linux/mmap_lock.h` | task lock 只用于 pin mm；后续独立等待可睡眠的 mm lock。 |
| `/proc/meminfo` 扫描全部 mm | `mm/util.c:vm_memory_committed()`、`include/linux/mman.h:vm_acct_memory()` | 在 VMA mutation 时计账，读取全局计数，不在 procfs 读取时扫描进程。 |
| 最后一个 file 引用的释放 | `fs/file_table.c:__fput()` | 可能睡眠的 release 工作必须运行在可睡眠上下文，不能依赖 idle 析构。 |
| ext4 只读 close 争抢写锁 | `fs/ext4/file.c:ext4_release_file()` | 只在最后 writer 的特殊清理需要 write-side 数据同步；只读 release 不取 inode write lock。 |
| stdout 行字节交错 | `drivers/tty/tty_io.c:tty_write_lock()`、`iterate_tty_write()` | 一个 userspace write 作为串行单位；阻塞锁可睡眠，`O_NONBLOCK` 竞争返回 `EAGAIN`。 |
| allocator spinlock 被中断重入 | `mm/page_alloc.c:rmqueue_buddy()`、`mm/slub.c` | 元数据 spinlock 使用 irq-save；页清零等耗时初始化继续留在 free-list 锁外。 |

这里复用 Linux 的可观察语义和锁边界，没有复制完整 task-work、percpu counter、
VMA maple tree 或 TTY line discipline。

### 10.3 分类别提交

后续内核修改已拆成四个可独立审查的提交：

| 提交 | 类别 | 内容 |
| --- | --- | --- |
| `ac12c47` | allocator | frame free-list 和 heap shard 的元数据锁增加 local irq-save；清页仍在锁外。 |
| `c58f935` | mm/procfs | `MmRef` 改用可睡眠 `KernelMutex`；PCB 只短暂 pin `MmRef`；VMA 内缓存匿名私有可写字节，`Committed_AS` 改为 O(1) 全局计数；mmap、procfs、exec 和 mmap 镜像路径统一锁顺序。 |
| `f169acc` | ext4/fput | 最后 writable fd 在 close 语义阶段刷新写缓冲；只读 `OSInode::drop()` 不再取得 inode write semaphore。 |
| `d21154e` | TTY | 在用户页翻译前取得可睡眠 stdout write mutex，并覆盖整次 write；nonblock 竞争返回 `EAGAIN`。 |

### 10.4 正确性边界

`MmRef` 的全局提交量由 `MmState` 生命周期维护：创建 mm 时增加，VMA/brk
变更后的 guard drop 只提交增量，最后一个引用销毁时扣除。所有 VMA map 的直接
`insert/remove` 已收敛到带计账的两个入口；无变化的 mm guard 不再写全局原子
变量。Relaxed ordering 足以提供 Linux 同样允许近似读取的统计值，不参与资源
所有权判定。

本地 `MemorySet` 当前仍是 exclusive sleeping mutex，而不是 Linux 可并发 reader
的 `mmap_lock` rwsem；这牺牲一部分读并行度，但修复了“持有会睡眠/I/O 的 mm
路径却使用 spin lock”的正确性问题。完整 delayed-fput/task-work 尚未实现；本次
把实际会产生写 I/O 的最后 writable close 前移到语义 close，上述 libc 只读
析构路径则完全不再请求 write side。

stdout 锁目前是唯一 console 对象的全局锁，不等价于完整 per-tty mutex；当前
ABI 下 stdin/stdout/stderr 指向同一 console，因此这一范围与可观察对象一致。

### 10.5 最终 CAgent

精确最终代码 `d21154e` 使用标准入口运行：

```sh
ARCH=riscv64 SMP=8 MEM=8G IMAGE_MODE=snapshot ./run.sh cagent
```

环境与结果：

- final test source：`1eac61d3becaa592c8ef12a7535f0ec6bb9e3e36`；
- RISC-V 镜像 SHA-256：
  `d899fe43d333d1d17ad8a5f8a8b74b68117b8c1ceacfc3843bfeadb1ca705bd1`；
- QEMU 11.0.3，8 hart，8 GiB，snapshot；本次 OpenSBI 从非 0 的 hart 5 启动；
- 10/10 子项通过，judge 退出成功；单项耗时 1566--3715 ms；
- judge JSON 的现行权重合计 199.1；本报告记录 10/10，不把它四舍五入伪报为
  200；
- 日志和 JSON：
  `.tmp/final-runs/20260801-223907-riscv64-cagent/`。

最终串口仍有 `remove_from_pid2process: ... already reaped?` 警告，但没有导致
子项失败或停滞；它是独立的重复 reap 诊断，不在本批锁修复范围内。

### 10.6 mixed IOZone

按用户要求不运行 BuildStorm，改用现有四进程文件 I/O 聚焦组。配置为 RISC-V
8 hart、2 GiB、snapshot、4 MiB/worker、1 KiB record；每轮先执行顺序
write/read，再执行 random read/write：

```sh
IOZONE_RUNS=3 IOZONE_WORKLOAD=mixed \
  bash tools/run_iozone_focus.sh \
  /home/shiyicong/temp/CongCore \
  .tmp/iozone/iozone-root.img \
  .tmp/iozone/linux-mm-fput-tty-final.log
```

| 轮次 | guest uptime 区间 | 耗时 | 顺序 rc | 随机 rc |
| --- | --- | ---: | ---: | ---: |
| 1 | 0.44--63.02 | 62.58 s | 0 | 0 |
| 2 | 63.02--129.88 | 66.86 s | 0 | 0 |
| 3 | 129.88--200.26 | 70.38 s | 0 | 0 |

三轮均出现 `IOZONE_FOCUSED_DONE rc=0`，中位数 66.86 秒。与 7.31 在同一聚焦
工作负载记录的稳定 ext4/IRQ 基线中位数 76.23 秒相比，缩短 9.37 秒，即
12.29%；与更早全局 `EXT4_LOCK` 的 81.13 秒相比缩短 17.59%。后一个数字跨越
多批修改，只作累计背景，不归因给单个提交。由于 QEMU 宿主调度会有波动，
12.29% 也只作为集成态测量；比绝对吞吐更强的证据是旧最终 HEAD 会在 mixed
随机阶段非确定性停滞，而本次 3/3 完整结束。

### 10.7 静态检查与未覆盖项

最终 `d21154e` 的以下检查均以退出码 0 完成：

```sh
cargo check --offline --locked --manifest-path ../os/Cargo.toml \
  --target riscv64gc-unknown-none-elf
cargo check --offline --locked --manifest-path ../os/Cargo.toml \
  --target loongarch64-unknown-none-softfloat
rustfmt --edition 2024 --check <本批 13 个 Rust 文件>
git -C ../os diff --check
```

仓库级 `cargo fmt --check` 仍会报告四个本批未修改的既有格式差异：两个架构
`mod.rs`、`main.rs` 和 `syscall/filesystem/perm_utils.rs`；没有为了让本批提交看似
全绿而混入这些无关改动。

未运行项目：完整 BuildStorm（遵守用户要求）、LoongArch QEMU 运行态、完整
IOZone 套件、unixbench 和 libcbench。LoongArch 只记录为 softfloat 静态构建
通过，不伪造运行态结论。
