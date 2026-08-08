# 7-31 多核内存与调度优化

## 问题概述

并行编译和频繁 `fork()/exit()` 会同时放大三个全局路径：内核堆只有一把锁，物理页引用计数存放在全局 `BTreeMap`，新任务又倾向留在创建它的处理器核心。
```rust
#[global_allocator]
static HEAP_ALLOCATOR: LockedHeap = LockedHeap::empty();   // 唯一全局分配器
// 全局计数，用来维护页的生命周期。
lazy_static! {
    static ref FRAME_REFCOUNTS: Mutex<BTreeMap<usize, usize>> = Mutex::new(BTreeMap::new());
}
impl Drop for FrameTracker {
    fn drop(&mut self) {
        let mut rc = FRAME_REFCOUNTS.lock();
        // cnt <= 1 → remove + frame_dealloc；否则 cnt -= 1
    }

}
// 留在创建者的hart 上，而不是更合理的。
if kind == EnqueueKind::Initial && (allowed_mask & (1usize << current_hart)) != 0 {
    task.set_cpu_id(current_hart);   // 初始入队直接钉在 fork 的 hart 上
    return current_hart;
}
```

除此之外， 写时复制（Copy-on-Write，COW）页表被一个核心替换后，其他核心TLB中的旧映射。

## 如何发现

使用AI 工具 进行代码分析发现。

## 怎么解决

内核堆按 `MAX_HARTS` 划分成地址互不重叠的 arena：分配优先锁当前核心对应的 arena，
空间不足时再尝试其他 arena；释放根据指针地址回到原 arena，避免任务迁移后放错。
简而言之，就是每个核心都有一个 heap。
为什么用spimutex ? 这个区域满了怎么办，可以查看下面的代码，注释写的很清楚。

```rust
// 分片内核堆 — os/src/mm/heap_allocator.rs
const HEAP_SHARD_SIZE: usize = KERNEL_HEAP_SIZE / MAX_HARTS;

/// Per-hart buddy heaps.
///
/// Rust's `buddy_system_allocator::LockedHeap` serializes every allocation and
/// free through one ticket lock.  Fork-heavy builds create and destroy enough
/// Arcs and vectors for that lock to dominate all harts.  Linux uses per-CPU
/// allocator fast paths for the same reason.  Keep buddy allocation semantics,
/// but partition the fixed heap into independently locked arenas.  Allocation
/// falls back to another arena when the local one is full; deallocation routes
/// by address, so tasks may migrate freely between the two operations.
struct ShardedHeap {
    shards: [SpinMutex<Heap>; MAX_HARTS],
}

impl ShardedHeap {
    const fn empty() -> Self {
        Self {
            // Use the non-ticket spin mutex explicitly. A global allocator
            // cannot sleep, and ticket head-of-line blocking is especially
            // costly when QEMU schedules virtual harts cooperatively.
            shards: [const { SpinMutex::new(Heap::new()) }; MAX_HARTS],
        }
    }

    unsafe fn init(&self, start: usize, size: usize) {
        debug_assert_eq!(size, KERNEL_HEAP_SIZE);
        debug_assert_eq!(size % MAX_HARTS, 0);
        for (index, shard) in self.shards.iter().enumerate() {
            let shard_start = start + index * HEAP_SHARD_SIZE;   // 地址互不重叠
            unsafe {
                shard.lock().init(shard_start, HEAP_SHARD_SIZE);
            }
        }
    }

    fn shard_for_ptr(&self, ptr: *mut u8) -> Option<&SpinMutex<Heap>> {
        let heap_start = addr_of!(HEAP_SPACE) as usize;
        let offset = (ptr as usize).checked_sub(heap_start)?;   // 按指针地址定位原 arena
        if offset >= KERNEL_HEAP_SIZE {
            return None;
        }
        self.shards.get(offset / HEAP_SHARD_SIZE)
    }
}

unsafe impl GlobalAlloc for ShardedHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let local = crate::arch::hart_id() % MAX_HARTS;         // 优先当前核心
        for offset in 0..MAX_HARTS {
            let index = (local + offset) % MAX_HARTS;           // 不足时轮转其他 arena
            if let Ok(allocation) = self.shards[index].lock().alloc(layout) {
                return allocation.as_ptr();
            }
        }
        core::ptr::null_mut()
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let shard = self.shard_for_ptr(ptr)
            .expect("global allocator received a pointer outside HEAP_SPACE");
        shard.lock().dealloc(unsafe { NonNull::new_unchecked(ptr) }, layout);
    }
}

#[global_allocator]
//static HEAP_ALLOCATOR: ShardedHeap = ShardedHeap::empty();
//初始化入口（init_heap）把 HEAP_SPACE（[u8; KERNEL_HEAP_SIZE]，bss 段）按 shard 顺序切分交给每个 Heap。

//2. Arc 物理页所有权 — os/src/mm/frame_allocator.rs
static FRAME_OWNER_COUNT: AtomicUsize = AtomicUsize::new(0);

struct FrameOwner {
    ppn: PhysPageNum,
}

impl Drop for FrameOwner {
    fn drop(&mut self) {
        FRAME_OWNER_COUNT.fetch_sub(1, Ordering::Relaxed);
        frame_dealloc(self.ppn);          // 最后一个 owner 才归还物理页
    }
}

/// manage a frame which has the same lifecycle as the tracker
#[derive(Clone)]
pub struct FrameTracker {
    pub ppn: PhysPageNum,
    owner: Arc<FrameOwner>,
}

impl FrameTracker {
    pub fn new(ppn: PhysPageNum) -> Self {
        // page cleaning
        let bytes_array = ppn.get_bytes_array();
        for i in bytes_array {
            *i = 0;
        }
        FRAME_OWNER_COUNT.fetch_add(1, Ordering::Relaxed);
        Self {
            ppn,
            owner: Arc::new(FrameOwner { ppn }),
        }
    }

    pub fn refcount(&self) -> usize {
        Arc::strong_count(&self.owner)   // 替代旧的 frame_refcount(ppn) 查表
    }
}
// 配套的分配/释放入口（仍在全局 FRAME_ALLOCATOR 互斥锁下）：
pub fn frame_alloc() -> Option<FrameTracker> {
    let mut allocator = FRAME_ALLOCATOR.lock();
    if let Some(ppn) = allocator.alloc() {
        return Some(FrameTracker::new(ppn));
    }
    // 失败 → 回收共享文件页缓存 → 重试 → DEBUG_PERF 下记录诊断
    ...
}

pub fn frame_alloc_contiguous(pages: usize) -> Option<Vec<FrameTracker>> {
    let start = FRAME_ALLOCATOR.lock().alloc_contiguous(pages)?;
    let mut frames = Vec::with_capacity(pages);
    for i in 0..pages {
        frames.push(FrameTracker::new(PhysPageNum(start.0 + i)));
    }
    Some(frames)
}

/// deallocate a frame
pub fn frame_dealloc(ppn: PhysPageNum) {
    FRAME_ALLOCATOR.lock().dealloc(ppn);
}
// 关键点：#[derive(Clone)] 让 FrameTracker 克隆只做 Arc 强引用 +1（原子操作，无锁无树操作），Drop 由 Arc 自动接替；FrameOwner::drop 里的 frame_dealloc 是全局唯一的释放点，天然消除了旧实现"漏删/重复释放"的风险。

```
这里之所以还保留 ppn的原因，而不是直接使用Arc<owner> 的原因是，兼容老代码 + 读写开销，arc 会有1次额外访存开销。
克隆只增加 `Arc` 强引用，最后一个 owner 才归还物理页。



可能的问题 没有根据负载来合理分配 堆的大小。

Linux 的每处理器页缓存、伙伴分配器、`struct page` 引用计数、调度域和
`mm_cpumask()` 比本项目完整得多。本批只实现能由当前数据结构支持的最小对应：分散
锁竞争、用对象所有权代替全局树、改进初始放置、按活动地址空间集合发送失效。固定
arena 仍可能产生分片，后续必须用真实空闲分布和性能数据决定是否改成共享大块来源。 

另一个问题的修复 
1. 任务放置：不再固定为入队的hart，根据负载 = 就绪队列长度 + 正在运行的任务,来选择最适合的来 ，且仍尊重 cpu_affinity_mask。
```rust
pub(super) fn resolve_enqueue_hart(task, current_hart, mask, _kind) -> usize {
    let affinity_mask = {
        let inner = task.borrow_mut();
        if inner.scheduling.cpu_affinity_mask == 0 { mask }
        else { inner.scheduling.cpu_affinity_mask & mask }   // 亲和性检查
    };
    let allowed_mask = if affinity_mask == 0 { mask } else { affinity_mask };
    if matches!(task_queue_slot(task), ReadyQueueSlot::Fair) {
        let picked = pick_least_loaded_hart_from_mask(allowed_mask);
        task.set_cpu_id(picked);
        return picked;
    }
    ...
}

pub(super) fn pick_least_loaded_hart_from_mask(mask: usize) -> usize {
    let mut best_hart = None;
    let mut best_len = usize::MAX;
    for hart_id in 0..MAX_HARTS {
        // mask = 0 不选
        if (mask & (1usize << hart_id)) == 0 { continue; }
        // 空队列的核可能正在跑 CPU 密集任务：把运行任务也计入负载
        let running = usize::from(
            crate::task::processor::current_task_on_hart(hart_id)
                .is_some_and(|task| task.get_cpu_id() == hart_id),
        );
        // 就绪队列 + 正在运行的，决定最佳人选 
        let len = TASK_MANAGER.ready_queues[hart_id]
            .lock().len()
            .saturating_add(running);
        if len < best_len { best_len = len; best_hart = Some(hart_id); }
    }
    best_hart.unwrap_or_else(|| pick_online_hart(0))
}
```

2. COW 远端失效：COW 提交后对实际驻留该地址空间的在线核（resident_harts，对应 Linux mm_cpumask）发页粒度 remote_sfence_vma()；所有目标共享同一 ASID 时用带 ASID 标记的 sfence.vma。

```rust
  
  let mut batch: PageTableUpdateBatch = self.begin_page_table_update();
  let Some(changed) = self.page_table.remap_deferred_changed(plan.vpn, frame.ppn, new_flags) else {
      return CowFaultCommit::Retry;
  };
  if changed {
      batch.record_page(fault_va);   // 只记账，不立刻发通知
  }
  #[cfg(target_arch = "riscv64")]
  if new_flags.contains(PTEFlags::X) {
      batch.mark_icache_stale();     // 可执行页才标记 icache 脏
  }
  self.areas[area_idx].replace_tracked_frame_batched(plan.vpn, frame.clone(), &mut batch);
  batch.commit();                    // 触发实际的远端失效
  
```

任务初始放置把 ready queue 和当前运行任务都计入负载，同时继续检查处理器亲和性。RISC-V 写时复制提交后，
只向实际使用该地址空间的在线核心发送页粒度 `remote_sfence_vma()`。
这部分的一些细节见下。


## 对应提交

- 内核实现：`43a786f perf(mm): improve multicore allocation and scheduling`。
- 顶层内核指针更新：`f02e26f1`。
- 文档提交：`38e32c422eafa4e92c4252832dfce9a500532be3`。

## 对比提升

未测量。 仅仅验证编译 可行性。


---

问题二 补充
# 关键汇编指令
sfence.vma

sfence.vma            ; 刷全部 TLB
sfence.vma rs1       ; 只刷包含 rs1 指定虚拟地址的项
sfence.vma rs1, rs2  ; 只刷与 rs2 指定 ASID 相关的项（rs1=0 表示全部地址）
## TLB  的样子
```md
┌──── Tag ─────┬───────────────────── Data ────────────────────┐
│  VPN(虚拟页号) │ ASID │ V │ PPN(物理页号) │ 权限 RWX │ A │ D │
├──────────────┼──────┼───┼──────────────┼─────────┼───┼───┤
│   0x1000     │  1   │ 1 │   0x5000     │  R W X  │ 1 │ 1 │
│   0x1000     │  2   │ 1 │   0x9000     │  R W X  │ 1 │ 1 │
│   0x2000     │  2   │ 1 │   0x6000     │  R - -  │ 1 │ 0 │
│   ...        │ ...  │...│   ...        │  ...    │...│...│
└──────────────┴──────┴───┴──────────────┴─────────┴───┴───┘

```

# icache?

指令缓存.

# Asid?
ASID（Address Space Identifier，地址空间标识符）是操作系统/处理器中用于标识进程地址空间的一个编号（在 x86 上类似 PCID，在 ARM 上叫 ASID）。
作用：TLB（页表缓存）中标记某个页表项属于哪个进程。上下文切换时无需完全清空 TLB，只需更新 ASID，避免每次切换进程都刷新 TLB 导致的性能损失。
- ARM：TTBR0_EL1 的 ASID 字段，通常 8/16 位，配合 CONTEXTIDR_EL1 使用
- Linux：mm->context.id 存储 ASID，ASID 耗尽时会触发 TLB 全清并重新分配
如果你问的是别的领域的 ASID（比如室内设计师协会 ASID），告诉我一下即可。
## ASID 是如何被感知的
靠 satp 寄存器。RISC-V 里当前进程的 ASID 就存在 satp 中，和页表基址一起：
satp（S 态寄存器，64 位，Sv39 为例）:
┌──────────────┬──────────┬──────────────────────────┐
│ MODE (63-60) │ ASID(59-44)│ PPN (43-0)  页表物理地址 │
│   8 = Sv39   │  16 位    │                       │
└──────────────┴──────────┴──────────────────────────┘
机制：
1. 切换进程时，内核执行 csrw satp, ...，把新进程的页表物理地址和 ASID 一起写进去。硬件做 TLB 查找时，拿的就是 satp.ASID 去和 TLB 条目里的 ASID 比对。
2. 所以在某个时刻，"当前 ASID" = satp.ASID，它是 CPU 的一个全局状态，不属于哪个具体进程——是进程切换时由内核（调度器）改写的。
3. ASID 分配由内核管理：Linux 里每个进程分配一个 ASID（16 位，最多 65536 个），用完就回收重分配（此时必须 sfence.vma 刷掉旧进程残留的 TLB 项）。
4. ASID 0 通常保留给内核/全局地址空间，配合页表项的 G 位使用——G=1 的条目不比对 ASID，对任何进程有效。
所以完整链路是：
进程切换 → 内核写 satp(新页表, 新ASID) → 之后的 TLB 查找自动用新 ASID 过滤
                                        → 旧进程的 TLB 条目自然失效（不匹配）

# 相关数据结构

asid.rs Asid Context.
专门管理 多核之间地址空间的一致性。
每个memspace 对应一个 
```rust
pub struct AsidContext {
    context: AtomicUsize,          // 分配到的 ASID + 世代号
    
    hart_contexts: [AtomicUsize; MAX_HARTS], // 每个核实际加载的 ASID（可能不同世代）
    resident_harts: AtomicUsize,   // ★ 哪些核可能还在用这个 mm 的页表（对应 Linux mm_cpumask）
    active_harts: AtomicUsize,     // 哪些核此刻在用户态跑这个 mm（对应 Linux mm_users? 不，是活动的）
    invalidation_sequence: AtomicUsize, // even/odd 事务锁，防与返用户态竞态
    icache_stale_mask: AtomicUsize,
}
```
context 最终编码，由于ASID 只有65535 个，不够 所以需要来区分 context = f(generation,ASID)
这个东西由一个全局函数来进行，当达到最大限度的时候，会给全局generation 递增，然后再从0 开始 
```rust
let max_asid = HW_ASID_MASK.load(Ordering::Acquire);
    if allocator.next > max_asid {
        let mut generation = allocator.generation.wrapping_add(1);
        if generation == 0 {
            generation = 1;
        }
        allocator.generation = generation;
        allocator.next = FIRST_USER_ASID;
        ASID_GENERATION.store(generation, Ordering::Release);
        PENDING_LOCAL_FLUSH.fetch_or(possible_hart_mask(), Ordering::AcqRel);
        crate::perf::record_tlb_asid_wrap();
    }
    let asid = allocator.next;
    allocator.next = allocator.next.saturating_add(1);
    encode_context(asid, allocator.generation)


```
可以看到 一旦触发了最大分配，会设置一个标志，要求所有人更新TLB。（旧的ASID 不再实用）

hart_contexts 访问过这地址空间的使用的ASID，用来确定刷新的时候是否可以使用优化的带ASID的刷新指令,同时可以用来检查自己的ASID 是否是陈旧的.
resident_harts. 哪些核可能有残留。刷新这些核心TLB（可能是懒惰的（通过一个全局标志来实现）。
active_harts. 哪些核心可能正在用这个。刷新这些核心(这个往往是直接IPI)
invalidation_.. 这是个 简单的锁设计 用来控制AsidContext的修改。
icache_stale_mask:哪些核心的icache可能需要刷新




