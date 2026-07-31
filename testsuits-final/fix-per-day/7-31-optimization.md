# 7-31 多核内存与调度优化

本批次针对并行编译、频繁 `fork/exit` 和多线程内存分配路径，减少全局锁竞争，
改善任务在多个 hart 之间的分布，并补齐 COW 页在多核并发下的 TLB 一致性。
对应内核提交为：

```text
43a786f perf(mm): improve multicore allocation and scheduling
```

## 1. 内核堆从单锁改为分片 buddy heap

原实现使用一个 `LockedHeap` 管理全部内核堆。所有 hart 上的 `Arc`、`Vec`、
任务对象和文件系统对象分配、释放都要竞争同一把锁。并行构建会频繁创建进程和
内核对象，这条全局串行路径会限制 SMP 扩展。

当前实现按 `MAX_HARTS` 将固定内核堆划分为互不重叠的 buddy arena：

- 分配优先访问当前 hart 对应的 shard；
- 本地 shard 空间不足时依次尝试其他 shard；
- 释放时根据指针地址定位原 shard，不依赖任务是否迁移；
- OOM 统计汇总全部 shard，保留原有诊断信息。

这样没有改变 `GlobalAlloc` 接口和 buddy 分配语义，但常见情况下不同 hart
可以在不同锁域中并行分配。

需要注意：固定分片可能产生 shard 间空间不均衡。当前通过跨 shard fallback
保证剩余空间仍可利用；后续应结合实际碎片率和锁等待时间决定是否增加更细的
per-hart cache。

## 2. 物理页引用计数移除全局 `BTreeMap` 锁

原 `FrameTracker::clone/drop` 每次都要访问全局
`Mutex<BTreeMap<ppn, refcount>>`。COW、共享 mmap 和页缓存会大量复制
`FrameTracker`，因此普通引用计数也被全局锁和树操作串行化。

现在每个物理页由 `Arc<FrameOwner>` 表示共享所有权：

```text
FrameTracker clone
        ↓
Arc strong count +1
        ↓
最后一个 FrameOwner drop
        ↓
frame_dealloc(ppn)
```

主要效果：

- clone/drop 不再访问全局映射；
- 最后一个引用负责唯一一次物理页释放；
- `FrameTracker::refcount()` 继续为共享页缓存回收和 mmap 写回判断提供引用数；
- 全局只保留一个原子 owner 数量，用于 OOM 和泄漏诊断。

这同时消除了手工维护引用计数表时漏删、重复释放以及锁范围扩大的风险。

## 3. 新建任务按实际负载分散

原公平调度路径会优先把新建任务留在执行 `fork` 的当前 hart。缺少 Linux
sched-domain 周期均衡机制时，并行编译产生的子进程容易集中在少数 hart。

现在公平任务的 initial enqueue 和 wakeup 都使用 affinity 范围内的最小负载
hart。负载计算除 ready queue 长度外，也计入该 hart 正在运行的任务，避免多个
空队列都被误判为空闲并反复选择第一个 hart。

该实现仍然遵守 CPU affinity；它是当前简化调度器上的初始放置改进，不等价于
Linux 完整的周期负载均衡、NUMA 拓扑和迁移成本模型。

## 4. 修复多核 COW 与 TLB 一致性

同一地址空间可能同时在多个 hart 上运行。一个 hart 完成 COW、替换 PTE 后，
其他 hart 仍可能缓存旧的只读映射或旧物理页翻译。

本批次增加：

- 获取内存空间锁后重新检查 PTE；若另一 hart 已经完成 COW，则把当前 fault
  视为 stale fault，刷新本地页表项后重试；
- RISC-V 上完成 COW 后，对其他在线 hart 发起页粒度 remote
  `sfence.vma`；
- SBI v0.2 extension call 同时接收 `a0` 的 error 和 `a1` 的 value，避免
  错误解释返回寄存器；
- 共享文件页缓存直接使用 `FrameTracker::refcount()` 判断外部引用。

这一部分首先是并发正确性修复，也避免旧 TLB 映射导致重复 fault、错误复制或
物理页过早复用。

## 涉及文件

- `os/src/mm/heap_allocator.rs`
- `os/src/mm/frame_allocator.rs`
- `os/src/mm/memory_set/backing.rs`
- `os/src/mm/memory_set/fault.rs`
- `os/src/mm/memory_set/file_mmap.rs`
- `os/src/task/manager/run_queue.rs`
- `os/src/sbi/mod.rs`

## 已完成验证

- VFS host tests：14/14 通过；
- RISC-V `riscv64gc-unknown-none-elf` 静态 `cargo check` 通过；
- 两个仓库 `git diff --check` 通过。

这些结果只能证明当前代码能够通过已有主机测试和 RISC-V 静态构建，不能直接
证明 BuildStorm 已获得确定的性能提升。

## 后续验收

仍需在相同镜像、QEMU、SMP 和内存配置下补齐：

1. 并行 `fork/exit` 与内核堆压力测试；
2. 多 hart 共享地址空间的 COW fault 压力测试；
3. BuildStorm 修改前后耗时、各 hart 利用率和 allocator 锁等待对比；
4. RISC-V CAgent/LTP 回归；
5. LoongArch 静态构建与运行验证；当前 remote TLB shootdown 仅接入 RISC-V。

在上述运行证据完成前，不记录未经测量的加速百分比。
