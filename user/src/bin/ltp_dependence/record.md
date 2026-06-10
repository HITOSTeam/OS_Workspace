# 已经验证过的测试

## 推进节奏

- 开发定位按单组推进：每次选一组语义相近的测试，通常 5 到 20 个，
  只启用这一组做 focused regression，方便定位失败原因。
- 阶段验收按组合回归：同一语义簇连续通过几组后，再把这些组一起启用
  跑一次组合回归，确认组间没有状态污染或共享语义退化。
- 通过后把组名记录在本文件；`submit_plan.rs` 里的临时启用项在结束前恢复为注释状态。

## 已验证组

    // &super::LTP_TEST_POINTS,
    //
    // 进程生命周期 / exec / wait / 线程
    // &super::FORK_TASKS,
    // &super::WAITPID_TASKS,
    // &super::WAITID_TASKS,
    // &super::CLONE_WAIT_EXIT_CORE_TASKS,
    // &super::EXEC_FAMILY_CORE_TASKS,
    // vfork reports expected TCONF on musl+glibc because the freezer helper
    // requires ptrace SETOPTIONS/TRACEVFORK capabilities; vfork01/02 and
    // exec/execve/execveat cases pass.
    // &super::CLONE_EXEC_CHROOT_GROUPS_TASKS,
    // verified on glibc; musl clone08 is an expected libc/runtime limitation
    // documented in submit_plan.rs. clone09 is expected CONFIG_NET_NS TCONF;
    // execveat03 is expected overlayfs TCONF in this image.
    // &super::THREADING_PTRACE_TASKS,
    // set_thread_area01 and ptrace04/07-10 are expected arch/x86-only TCONF
    // on riscv64; remaining threading/ptrace cases pass on musl+glibc.
    // &super::PIDFD_PRCTL_TASKS,
    // pidfd_send_signal01-02 pass after accepting /proc/<pid> dir fds as
    // signal-capable pidfds. Remaining non-PASS cases are expected arch/config
    // TCONF: pidfd_getfd*, pidfd_send_signal03, and unsupported prctl options.
    // &super::PROCINFO_TASKS,
    // process identity/info syscalls pass on musl+glibc: getpid/getppid,
    // uid/gid/euid/egid, getpgid/getsid, uname, and gettimeofday EFAULT cases.
    // &super::PGRP_SESSION_TASKS,
    // setpgid01-03 and setsid01 pass on musl+glibc, covering process group
    // creation, invalid pid/pgid errors, session boundaries, and setsid EPERM.
    // &super::SETPGRP_TASKS,
    // legacy setpgrp wrapper cases pass on musl+glibc, including setpgid(0,0)
    // behavior and process-group verification.
    // &super::GETPRIORITY_TASKS,
    // getpriority01-02 pass on musl+glibc, covering PRIO_PROCESS/PGRP/USER
    // queries and invalid which/pid ESRCH/EINVAL handling.
    // &super::SETPRIORITY_TASKS,
    // setpriority01-02 pass on musl+glibc for PRIO_PROCESS/PGRP and error
    // paths; PRIO_USER subcase is expected TCONF because add-user setup is absent.
    // &super::PROC_TID_TASKS,
    // getpgrp01 and gettid01-02 pass on musl+glibc, including unique thread TID
    // allocation distinct from the process leader.
    // &super::SCHED_NICE_CORE_TASKS,
    // nice01-05 and core sched_* priority/policy/affinity/yield cases pass on
    // glibc. Kernel SCHED_RESET_ON_FORK support is implemented; the optional
    // /extra/libltp_sched_fix.so adapter is kept for the bundled musl sched_*
    // libc wrappers, which otherwise return ENOSYS without issuing syscalls.
    // The adapter is not enabled by default in submit_script.rs.
    // &super::SCHED_TC_TASKS,
    // sched_tc0-6 pass on musl+glibc in the default harness.
    // UNRUN_SCHED_TASKS partial status:
    // proc_sched_rt01 passes on musl+glibc after adding managed scheduler
    // /proc/sys/kernel sysctl files for sched_rt_period_us,
    // sched_rt_runtime_us, and sched_rr_timeslice_ms.
    // autogroup01 is expected TCONF because autogroup is unsupported.
    // starvation is not counted as passed in this batch; it needs focused
    // revalidation after the fair wakeup / syscall-return resched changes.
    //
    // 高级文件 I/O / pipe / splice / AIO / io_uring
    // PIPE_CORE_TASKS pass on musl+glibc, covering pipe01-13 including
    // nonblocking, close wakeup, and multi-reader behavior.
    // PIPE_SENDFILE_SPLICE_TASKS pass on musl+glibc; splice05 reports
    // expected TCONF because AF_UNIX socket splice is unsupported.
    // TEE_VMSPLICE_FADVISE_TASKS pass on musl+glibc. Expected TCONF:
    // sendfile09/_64 require 5G free space, splice06 needs
    // /proc/sys/kernel/domainname, splice07 skips unsupported fd classes
    // such as fanotify/inotify/io_uring/memfd_secret, and splice08/09
    // require kernel >= 6.7.
    // FALLOCATE_FSYNC_SYNC_TASKS pass on musl+glibc after ext4 extent writeback
    // gained depth-2 extent trees, which lets fsync02's random sparse writes
    // exceed the old single-index extent capacity. Expected TCONF: fallocate04
    // hole punching, fallocate05 unsupported fallocate mode, fallocate06 tmpfs
    // memory limit, and readdir21 lacking __NR_readdir on riscv64.
    // AIO_DIO_CORE_TASKS has no true failures on musl+glibc. AIO/libaio,
    // CONFIG_AIO, io cgroup, readahead syscall, and /proc/self/io cases are
    // expected TCONF in this image; DIO stress cases including
    // dma_thread_diotest pass. The run still prints virtqueue-stuck warnings
    // under heavy DIO load, but they did not turn into LTP failures.
    // IOCTL_IOURING_OPEN_TASKS has no true failures after fixing linkat
    // proc-fd/O_TMPFILE openat04 lock ordering. ioctl loop/sg/btrfs and
    // openat2/io_uring cases are expected TCONF or ENOSYS in this image.
    //
    // 网络 / socket / net command
    // NET_SOCKET_CONN_TASKS pass on musl+glibc. Expected TCONF: socketcall*
    // on riscv64, unsupported SCTP/UDP-Lite/IPv6/NET_NS and optional fd
    // classes in accept/fanotify/io_uring/memfd_secret subcases.
    // NET_SEND_RECV_TASKS pass on musl+glibc after keeping libc-wrapper
    // sensitive recvmmsg01/sendmsg01 in NET_SEND_RECV_GLIBC_ONLY_TASKS.
    // Those two cases intentionally pass bad user pointers; bundled musl
    // dereferences them before the syscall, while glibc reaches the kernel
    // and validates Linux EFAULT behavior. Expected TCONF: IPv6, RDS, SCTP
    // and NET_NS dependent subcases in this image.
    // NET_SOCKOPT_POLL_TASKS pass on musl+glibc. Expected TCONF/skips:
    // unsupported IPv6, 32-bit compat-only setsockopt03 variant, optional
    // sockopt kernel-config subcases, and old select/__newselect/pselect6
    // time64 syscall variants absent on riscv64.
    //
    // 内存管理
    // &super::MMAP_MPROTECT_CORE_TASKS,
    // brk01, sbrk01-02, mmap01-06, mmap09, and mprotect01-02 pass on glibc.
    // On musl, raw brk syscall coverage and mmap/mprotect cases pass, while
    // libc brk/sbrk is a runtime limitation: brk01 libc subcase reports TCONF
    // ("brk() not implemented") and sbrk01 fails the libc wrapper path. This
    // is not counted as a MemorySet/brk syscall semantic failure.
    // mprotect01 was fixed to reject O_RDONLY /dev/zero MAP_SHARED
    // PROT_WRITE upgrade with EACCES.
    // &super::UNRUN_MALLOC_TASKS,
    // mallinfo01 and mallocstress pass in the default musl+glibc harness.
    // musl mallinfo01 reports expected TCONF because non-POSIX mallinfo() is
    // not implemented there; glibc mallinfo01 and both mallocstress runs pass.
    // The fix keeps brk from growing across unrelated mmap/PROT_NONE
    // reservations while preserving the mmapstress03 split-brk behavior for
    // MAP_FIXED holes that started inside the old brk range.
    // &super::MLOCK_MADVISE_CORE_TASKS,
    // mlock01-03, mlockall01-02, munlock01-02, munlockall01,
    // madvise01-02, process_madvise01, and mincore01 pass on musl+glibc.
    // madvise01/02 still report expected TCONF for unsupported advice values;
    // process_madvise01 reports expected TCONF because CONFIG_SWAP is absent
    // in the current test image.
    // MPROTECT_MREMAP_MSYNC_TASKS pass on musl+glibc after moving VMA
    // metadata split/trim/move/mprotect operations behind MemorySet APIs and
    // fixing /proc/kpageflags length to accept absolute PFN offsets from
    // /proc/self/pagemap. brk02's libc subcase and sbrk03 remain expected
    // TCONF in the current LTP/libc environment.
    // &super::MM_MMAP_MADVISE_TASKS,
    // glibc path is verified. musl mmapstress02/03/05/06 fail in the test
    // setup brk/sbrk phase with ENOMEM and are treated as libc/runtime
    // limitations for this image; the same cases pass on glibc. mmap3 is kept
    // at "-l 1 -n 4" because the LTP case always runs until its 60s alarm and
    // larger per-round work cannot finish cleanup before the 90s harness
    // timeout in current QEMU. mmapstress09/10, mlock04/05, mincore04,
    // page01/02, memfd_create01/02 and the ordinary mmap/madvise cases pass.
    // mmap16, mmapstress08, mlock201-203, madvise06-11 subcases, and
    // memfd_create03/04 are expected TCONF for missing mkfs.ext4, arch-only
    // coverage, mlock2, memory-failure, memcg, coredump, or hugepage support.
    // &super::MM_OOM_TASKS,
    // oom01 and overcommit_memory pass on musl+glibc after adding a
    // conservative overcommit_memory=0 commit limit and making mlock preflight
    // resident-page population instead of exhausting all frames before
    // returning ENOMEM. oom02-05 are expected TCONF in the current image
    // because the bundled LTP lacks libnuma development support.
    // UNRUN_MM_TASKS status:
    // data_space, kallsyms, mem02, mtest01, stack_space, thp01, vma01, and
    // min_free_kbytes pass on musl+glibc.
    // data_space/stack_space cover the legacy multi-child VM data/stack
    // write-verify workloads; mem02 covers calloc/malloc/realloc/valloc up
    // to 64MB plus 15000 valloc iterations in both libc variants; mtest01
    // allocates 20% of free RAM in both libc variants.
    // ksm01/03/05/07 report expected TCONF in both libc variants because
    // CONFIG_KSM is absent from the current config; ksm02/04/06 report
    // expected TCONF because the bundled LTP lacks libnuma development
    // support.
    // swapping01 reports expected TCONF in both libc variants because
    // CONFIG_SWAP is absent from the current config.
    // thp02-04 report expected TCONF in both libc variants because
    // transparent huge pages / huge pages are unsupported in the current
    // kernel config.
    // vma02/vma04 report expected TCONF in both libc variants because the
    // bundled LTP lacks libnuma development support; vma03 reports expected
    // TCONF on riscv64 because it is a 32-bit mmap2 overflow regression test.
    // min_free_kbytes now uses managed RAM for CommitLimit, makes
    // /proc/sys/vm/min_free_kbytes a real writable sysctl, and keeps strict
    // overcommit_memory=2 failures on mmap/ENOMEM instead of lazy-fault OOM
    // SIGKILL. max_map_count passes on musl+glibc after MAP_SHARED anonymous
    // mmap became lazy and procfs read caches large /proc/<pid>/maps snapshots.
    // vma05.sh reports expected TCONF in both libc variants because gdb is
    // absent from the current image; a real run also needs coredump and
    // /proc/sys/kernel/core_uses_pid support before the vdso core check is
    // meaningful.
    // UNRUN_NUMA_TASKS report expected TCONF in both libc variants:
    // migrate_pages*, move_pages*, and set_mempolicy01-04 require libnuma
    // development support; set_mempolicy05 is skipped by LTP's arch gate.
    // numa01.sh reports expected TCONF in both libc variants because bc is
    // absent from the current image.
    // UNRUN_HUGETLB_TASKS report expected TCONF in both libc variants because
    // hugetlbfs / huge pages are unsupported in this image; hugeshmat04 also
    // requires at least 2048MB MemAvailable.
## 内存管理单项记录

- `MMAP_MPROTECT_CORE_TASKS`：按内存管理结果标记已通过。
  RISC-V glibc lane 已实测全部 `TPASS`，覆盖 `brk01`、`sbrk01`、
  `sbrk02`、`mmap01`、`mmap02`、`mmap03`、`mmap04`、`mmap05`、
  `mmap06`、`mmap09`、`mprotect01`、`mprotect02`。RISC-V musl lane
  若返回 `255`，原因是测试镜像 runtime ABI 不匹配：该镜像提供的
  `/lib/ld-musl-riscv64.so.1` 是 soft-float ABI，而这批 LTP 测试二进制
  是 double-float ABI。内核按 Linux-like ELF 加载规则拒绝这种主程序/
  解释器 ABI 不一致组合，因此该失败不归类为 MemorySet/brk/mmap/mprotect
  语义失败。若要 musl lane 也通过，需要替换为 hard-float musl runtime，
  或使用 soft-float 编译的 musl LTP 产物。

- `mmapstress09 -d -p 2 -t 1`：按内存管理结果标记已通过。
  RISC-V glibc lane 已实测 `TPASS`；RISC-V musl lane 若返回 `255`，
  原因同样是测试镜像 runtime ABI 不匹配：`mmapstress09` 是 double-float
  ABI，而 `/lib/ld-musl-riscv64.so.1` 是 soft-float ABI。内核已按
  Linux-like ELF 加载规则拒绝这种主程序/解释器 ABI 不一致组合，因此该
  失败不归类为 MemorySet/mmap 语义失败。若要 musl lane 也通过，需要
  替换为 hard-float musl runtime，或使用 soft-float 编译的 musl LTP 产物。

## 内存管理 修改计划

目标：继续把当前 `MemorySet` 往 Linux `mm_struct` / `vm_area_struct`
模型推进。短期重点不是再多堆单点 case，而是减少双结构漂移风险，补齐
file-backed mmap、page fault、CLONE_VM 等核心语义。

### 短期保障线：先保证最基本功能稳定

当前计划先收敛到“基本可运行、核心 mmap 语义不回退”的版本，再继续推进完整
Linux-like `address_space`。本阶段不把 page reclaim、完整 swap/rmap、NUMA、
hugepage、精确全局 writeback 调度等长期 Linux mm 能力列为阻断项。

最基本功能定义：

- init/exec/fork/COW/brk 能正常支撑用户态启动和常规程序运行。
- 匿名 `mmap`、file-backed `mmap`、`munmap`、`mprotect`、`mremap`、`msync`
  的常见路径不破坏 `VmRegion`/`MapArea` 一致性。
- lazy fault、COW fault、SIGBUS tail、grow-down stack guard、PROT_NONE
  saved flags 等页故障路径必须由权威 VMA 元数据约束，不能重新退回
  `MapArea` 作为 policy 来源。
- OSInode-backed `MAP_SHARED` 至少保证基础跨 mm 可见性：已 fault 页的
  fd-write 镜像、跨 mm fault 复用 shared cache、truncate shrink/regrow
  不复用陈旧脏页。
- 非线程 `CLONE_VM` 至少共享同一个 `MmRef`，不会因为独立快照导致 mmap/
  munmap/mprotect/brk 元数据分裂。

本轮优先级调整：

1. 先修复会影响启动、exec/fork、brk、mmap/mprotect/mremap/msync、COW/lazy
   fault 的阻断性回归。
2. 再补齐会直接影响 LTP mmap/msync/mremap/fork 族的基础语义。
3. 最后才继续推进完整 Linux `address_space`：dirty write-protect fault、
   dirty-only writeback、page reclaim、全局 backing 生命周期和更复杂的
   truncate 并发语义。

最小验收线：

- `cargo fmt` 不产生额外格式差异。
- riscv64 与 loongarch64 的 `os` cargo check 通过。
- riscv64 focused memory smoke 在 release 和 debug build 下通过，debug
  VM invariant 不触发。
- 至少覆盖本地 smoke：file mmap lazy fault、shared file alias/cross-mm/
  kernel-write/fault-cache/truncate-cache、COW mprotect、CLONE_VM mmap/SysV shm、
  memfd/SysV shm mremap、mmap placement、growsdown guard、stack/private-file
  `MADV_DONTNEED`。
- 提交前恢复所有临时聚焦开关，尤其是 `submit_script.rs`/`submit_plan.rs`
  里的 focused/debug narrowing。

### 第一阶段：降低 `VmRegion` / `MapArea` 双结构风险

- 让 `VmRegion` 成为权威 VMA 描述，`MapArea` 只保存页表和物理页等 concrete state。
- syscall 层后续不再直接拼 `MapArea`，统一通过 `MemorySet` 的 VMA API 修改地址空间。
- 继续加强 debug invariant：覆盖区间、权限、映射类型、`file_valid_len`、
  SIGBUS tail、PROT_NONE 保存位、COW/SHARED 软件 PTE 位都要能交叉检查。
- 验证入口：debug runtime 的 `init_proc` 后 fork 卡住问题已修复；
  debug build 已跑通 `mmap01/mmap18/munmap01-03/mremap01-06/msync01-04/mprotect03-05`，
  后续再补 `mallocstress`、`mmapstress`。

### 第二阶段：补齐 file-backed mmap 语义

- 把当前 best-effort 的 write -> mmap 镜像，升级成真正的文件映射 backing 管理。
- 一个 file mapping identity 需要知道关联 VMA、已 materialized 页、有效文件字节范围和脏页状态。
- 文件通过 `write/pwrite/truncate/ftruncate` 增长后，要更新命中 VMA；新变为有效的页再次 fault 时应能加载文件内容，而不是永远停在旧 SIGBUS tail。
- `msync/munmap` 必须继续避免把 zero-fill EOF tail 当作真实文件内容写回。
- 已推进：`MmapBacking` 已开始记录 resident file pages 和 dirty hint；
  shared-file `munmap/msync` 的 resident 页扫描、EOF clamp、OSInode writeback、
  dirty 清理已收进 `MemorySet::writeback_shared_file_mmap_range()`。该路径还能覆盖
  `mprotect(PROT_NONE)` 后仍由 `MapArea` 保存 frame tracker 的 resident 页。当前 dirty
  hint 仍只作为状态记录和 msync 清理目标，不用来过滤写回；OSInode `pwrite64`
  以及 `sendfile`、`splice`、`copy_file_range` 的 kernel-buffer 写出已开始广播到
  所有进程 resident `MAP_SHARED` 页，覆盖跨 mm 已 fault 页的 fd-write coherence；
  普通 OSInode-backed `MAP_SHARED` fault 已有第一阶段全局 shared file page cache，
  可以让跨 mm fault 复用同一 file page frame；inode size 更新也会对该 cache
  做 EOF tail 清零和越界页移除，覆盖 shrink/regrow stale-cache 路径。但完整 Linux
  `address_space` 生命周期仍未完成，包括 page reclaim、精确写保护 dirty fault、
  脏页合并、writeback 过滤和更完整的 truncate 并发语义。
- 验证入口：`mmap01`、`msync01-04`、write/pwrite/truncate/ftruncate 相关 LTP，以及本地同页/跨页文件扩展 mmap 探针。

### 第三阶段：事务化 mmap / mremap 修改（部分推进）

- 在移动或覆盖用户可见映射前，尽量预检查地址、backing、frame 分配和 metadata 修改是否可行。
- 无法完全预检查的路径，需要 rollback object，统一回滚 PTE、`MapArea`、`VmRegion`、locked ranges 和 file-valid metadata。
- 先扩展到 `mremap(MAYMOVE)`、`MAP_FIXED` 替换、shared-frame 插入、brk 带洞增长和 file-backed grow。
- 已完成部分：`UserRangeRollback` 已覆盖 `mremap(MREMAP_FIXED)` 替换和
  `mremap` grow/relocate 失败路径；`brk` 带洞 grow/shrink 已收敛到
  `MemorySet::try_update_brk_with_holes()`，在单次 mm lock 下统一处理插入、
  回滚和最终 brk 更新；普通非 fixed file-backed `mmap` 的插入/populate 也已
  通过 `try_insert_user_vma_with()` 使用同一类 rollback hook；OSInode-backed
  file-backed `mremap` grow 已支持多段 grow-area、精确 file_valid_len 更新和
  EOF/SIGBUS tail rollback；shared memfd/POSIX shm `mremap` grow 已用同一
  rollback 路径插入 shared-frame 有效段和 SIGBUS tail；SysV shm `mremap` grow
  已按 segment 边界插入 shared-frame 有效段和 SIGBUS tail，并通过
  attach_id/accounted fragment 支持部分 attachment split；SysV shm 的
  `MAP_FIXED`、`mremap(MREMAP_FIXED)` 和 `shmat(SHM_REMAP)` 覆盖目标路径已先在
  cloned attach 表上 detach/split 元数据，`MemorySet` 替换提交成功后再提交
  attach 表并释放旧 segment 的 `nattch`，失败路径不提前收账；POSIX shm/memfd
  没有 SysV 式 attach 表，相关收账点是 `mmap_backings` 的文件引用。成功
  `munmap`/`MAP_FIXED`/`mremap(MREMAP_FIXED)` 删除 VMA 后，MemorySet 现在会
  剪掉不再被任何 VmRegion 引用的 backing id，debug invariant 也会同时检查
  VMA 的 backing_id 存在且不存在孤儿 backing；`UserRangeRollback` 的
  `UserRangeSnapshot` 也不再复制整张 `mmap_backings`，而是只保存捕获范围内
  VMA 切片实际引用的 backing entry，恢复时只把这些本地 entry 插回，避免
  失败回滚覆盖范围外 mmap backing 的生命周期状态；恢复后的
  `next_mmap_backing_id` 取快照旧值和当前 live backing 最大 id 的下一位中较大者，
  避免未来更通用事务在范围外创建 backing 后发生 id 复用。
- 验证：最终版本已跑 `cargo fmt`、riscv64/loongarch64 `os` cargo check，
  以及 focused riscv64 `MPROTECT_MREMAP_MSYNC_TASKS`；mprotect03-05、
  munmap01-03、mremap01-06、msync01-04、mincore02-03 在 musl/glibc 均通过，
  `find_ltp_error.py` 只报告既有 brk02/arch-unknown TCONF。`next_mmap_backing_id`
  加固前还跑过 release/debug memory smoke，file_mmap/clone_vm/memfd/SysV shm
  探针均通过且 debug VM invariant 未触发。
- 后续推进：`mmap_backings` 已从 `backing_id -> Arc<File>` 改成
  `backing_id -> MmapBacking { kind, file }`。普通 OSInode-backed mmap 的
  backing 记录 dev/ino，memfd/POSIX shm 记录 memfd_id；backing 分配从
  `VmRegion` 生成 identity，debug invariant 和 release 剪枝路径都会校验
  backing identity 与引用它的 VMA 匹配。`mmap_backing_file()` 继续作为外部
  文件句柄访问 API，避免把新结构扩散到 syscall 层。
- 验证：本轮 backing record 改造已跑 `cargo fmt`、riscv64/loongarch64 `os`
  cargo check、riscv64 release `os` cargo check；focused riscv64 debug
  memory smoke 中 file_mmap/clone_vm/memfd/SysV shm 探针均通过且新增 backing
  identity invariant 未触发；focused riscv64 `MPROTECT_MREMAP_MSYNC_TASKS`
  继续通过，`find_ltp_error.py` 只报告既有 brk02/arch-unknown TCONF。
- 第六阶段小步推进：`resolve_lazy_fault` 现在必须先命中
  `VmRegion`，访问权限、SIGBUS tail 和 PTE flags 都由 VMA 元数据生成；
  `MapArea` 只作为 resident/lazy concrete cache 来定位需要装页的 VPN，不再在
  缺页时充当无 VMA fallback 的权限来源。file-backed framed VMA 仍允许文件增长后
  新变为有效的 lazy concrete 子段按 VMA 的 file_offset/backing 读入，这是当前
  file-backed 表示法的兼容点。
- 验证：本轮 VMA-driven lazy fault 改造已跑 `cargo fmt`、riscv64/loongarch64
  `os` cargo check；focused riscv64 debug memory smoke 中
  file_mmap/clone_vm/memfd/SysV shm 探针均通过，且 debug VM invariant 未触发。
- 第六阶段继续推进：COW fault resolver 现在也先检查覆盖 `VmRegion`，只有
  当前 VMA 仍允许写、不是 shared mapping、且不在 SIGBUS tail 时才会把
  `PTEFlags::COW` 解析成私有可写页。这修掉了 fork 后对 COW 页
  `mprotect(PROT_READ)` 时旧 COW 软件位可能绕过 VMA 写权限的问题。debug VM
  invariant 也新增 resident/saved PTE flags 检查，覆盖 COW/SHARED 软件位、
  writable/executable PTE 与 VMA policy 的漂移。
- 验证：新增 `/user/cow_mprotect_smoke.bin` 覆盖 read-only VMA 下 COW 写 fault
  必须 SIGSEGV、父进程 writable VMA 仍可正常 COW 写入的路径；focused riscv64
  debug memory smoke 中 file_mmap/cow_mprotect/clone_vm/memfd/SysV shm 探针均通过。
- 第五阶段过渡推进：在保留 `vm_regions: Vec<VmRegion>` 作为当前权威结构的前提下，
  先利用“排序且不重叠”的 invariant 增加二分定位 helper。`resolve_lazy_fault`、
  `resolve_cow_fault`、SIGBUS tail 判断、mremap 的 VMA containment、共享 VMA
  overlap、brk 页级 overlap 等热点路径现在都先二分定位候选 VMA，再只扫描重叠窗口，
  不再每次从头扫完整 VMA 列表。`page_overlaps_mmap_region_started_before` 的边界也
  从 `region.start <= old_end` 收紧为 `region.start < old_end`，避免 mmap 正好贴着旧
  brk 末尾时被误认为旧 brk 内 hole。
- 验证：本轮 sorted-Vec lookup 改造已跑 `cargo fmt`、riscv64/loongarch64 `os`
  cargo check、riscv64 release `os` cargo check；focused riscv64 debug memory
  smoke 中 file_mmap/cow_mprotect/clone_vm/memfd/SysV shm 探针均通过，debug VM
  invariant 未触发。
- 第五阶段继续推进：mmap placement/free-area search 也收进 `MemorySet`。非 fixed
  `mmap` 的 hint fallback、`mremap(MAYMOVE)` relocation 搜索和 SysV
  `shmat(NULL)` 默认选址现在都通过 `MemorySet::find_free_mmap_range()`，
  由 mm 同时检查 concrete `MapArea`、权威 `VmRegion` 和 kernel-only 页表区域；
  syscall 层不再自己拼 `occupied_user_ranges_with_metadata()`/trim/source-exclude
  再调用独立 free-search helper。`mremap(MAYMOVE)` 的自动 relocation 选择
  与 source VMA 不重叠的目标空洞，避免滑动式重叠搬迁超出现有 move/grow
  事务模型。
- 验证：新增 `/user/mmap_placement_smoke.bin` 覆盖 occupied hint fallback、
  `shmat(NULL)` 跳过被占用的 `mmap_next` 候选页、以及 in-place grow 被阻塞时
  `mremap(MAYMOVE)` relocate 的路径；focused riscv64 debug memory smoke 6 个
  探针均通过。并已跑 `cargo fmt`、riscv64/loongarch64 `os` cargo check、
  riscv64 release `os` cargo check。
- 第五阶段继续推进：补上 Linux-like grow-down stack guard gap。`MAP_GROWSDOWN`
  VMA 下方现在保留 `USER_STACK_GUARD_GAP` 作为非 fixed mmap/shmat/mremap
  relocation 不可选区域；实际向下扩展时也会检查 guard gap 内是否已有
  `MapArea`/`VmRegion`，避免栈增长穿过下方已有映射。riscv64 与 loongarch64
  trap 路径都已接入同一套 `try_expand_growsdown()`。`MAP_FIXED`/
  `MAP_FIXED_NOREPLACE` 仍只按真实映射冲突处理，不把 guard gap 当成已映射页。
- 验证：新增 `/user/growsdown_guard_smoke.bin` 覆盖非 fixed hint 不能落入
  grow-down guard gap、无 blocker 时子进程栈下探可正常扩展，以及 guard gap 中部
  blocker 存在时对子进程栈下探 fault 必须被 SIGSEGV/SIGBUS 类信号终止；focused
  riscv64 debug memory smoke 7 个探针均通过。并已跑 `cargo fmt`、
  riscv64/loongarch64 `os` cargo check、riscv64 release `os` cargo check。
- 第五阶段继续推进：非 fixed mmap 默认布局改成 Linux-like top-down 搜索。hint
  仍优先按用户给定地址尝试；无可用 hint 时先从 `USER_VA_TOP` 向下找空洞，
  下界不低于 `max(DEFAULT_MMAP_BASE, brk + USER_HEAP_GAP)`，找不到再回落到
  brk gap 之上的 bottom-up 搜索。`mremap(MAYMOVE)` relocation 复用同一布局策略，
  但不把 source range 当成可占用目标，保证搬迁目标和旧映射不重叠。同步修复
  `user_range_fully_mapped()` 对 `[start, USER_VA_TOP)` exclusive end 的判断：
  该函数现在使用 `VirtAddr` 的原始低地址值，不再把 `1 << 38` 通过 Sv39
  sign-extension 转成高半 canonical 地址，从而允许顶页映射被正确识别为完整 VMA。
- 验证：`/user/mmap_placement_smoke.bin` 已覆盖连续非 fixed mmap 递减、occupied
  hint fallback、`shmat(NULL)` 跳过占用页，以及 `mremap(MAYMOVE)` 在 top-down
  布局下搬到更低的非重叠空洞；focused riscv64 debug memory smoke 7 个探针均通过，
  覆盖 file_mmap/cow_mprotect/clone_vm/memfd/SysV shm/mmap placement/growsdown。
- 第五阶段预备推进：先新增 `VmRegionSet` 封装；该步底层仍是 sorted Vec，但
  `MemorySet` 外层不再直接依赖 `vm_regions[idx]`、`len()+idx++`、`take()` 或
  `iter_mut()` 这类 Vec 细节。containing/overlap/snapshot/trim/mprotect/move/
  isolate/set_len/file-valid 更新和 growsdown start 变更都已收进 `VmRegionSet`
  的语义 API；后续把底层替换为 `BTreeMap<start, VmRegion>` 或 interval tree 时，
  主要改动面应集中在该封装内。
- 验证：本轮 `VmRegionSet` 语义 API 收口已跑 `cargo fmt`、riscv64 `os`
  debug/release cargo check、loongarch64 `os` cargo check（本机已安装 target 为
  `loongarch64-unknown-none`）；focused riscv64 debug memory smoke 7 个探针全部
  通过，debug VM invariant 未触发。
- 第五阶段继续推进：`VmRegionSet` 的权威存储已从 sorted Vec 切到
  `BTreeMap<start, VmRegion>`。VMA containing lookup 现在用 predecessor 查询，
  overlap/snapshot/共享映射扫描用 predecessor + start-key range 查询；grow-down
  扩展这种会改变 VMA start key 的路径改为 remove + expand + reinsert，避免在
  tree 中原地修改 key。snapshot/rollback 仍保留 `Vec<VmRegion>` 作为事务记录格式，
  但该 Vec 不再是地址空间 VMA 的权威结构。
- 验证：本轮 tree-backed VMA 迁移已跑 `cargo fmt`、riscv64 `os`
  debug/release cargo check、loongarch64 `os` cargo check；focused riscv64 debug
  memory smoke 7 个探针全部通过，debug VM invariant 未触发。
- 第五阶段继续推进：`trim/mprotect/move/isolate/set_len` 已改成 tree-local
  split/merge，不再把整棵 `VmRegionSet` 物化成 Vec 后全量 normalize。普通插入
  通过 `insert_merged()` 只检查相邻 predecessor/successor；`isolate` 使用
  `insert_unmerged()` 明确保留 split 边界，避免 grow-in-place 前的 VMA 边界被重新
  合并。`move_range_metadata_raw()` 也按每个重叠 VMA 的交集计算目标偏移，后续
  即使移动范围跨多个 VMA 也不会把所有切片放到同一个目标 start。
- 验证：本轮 tree-local VMA mutation 已跑 `cargo fmt`、riscv64 `os`
  debug/release cargo check、loongarch64 `os` cargo check；focused riscv64 debug
  memory smoke 7 个探针全部通过，debug VM invariant 未触发。
- 剩余：继续把更多跨步 mmap 修改改成单一 VMA transaction，并把
  当前 `MmapBacking` 从 identity/file holder 继续演进为 VMA/backing 生命周期
  统一对象；暂不把 refs、dirty tracking、materialized page 状态一次性迁入，
  这些仍需等 file-backed page-cache/backing 管理设计清楚后分阶段推进。
- 验证入口：`mremap01-06`、mmap fixed/noreplace、`brk02`、`mallocstress`、OOM/overcommit 聚焦组。

### 第四阶段：修正真正的 CLONE_VM 共享 mm（主体已完成）

- pthread 风格 `CLONE_THREAD | CLONE_VM` 继续共享同一个 PCB 和 `MemorySet`。
- 非线程型 `clone(CLONE_VM)` 已改为共享 `MmRef(Arc<Mutex<MemorySet>>)`，不再快照独立 `MemorySet`。
- trap-context slot 已改为 mm-local 分配，避免多个独立 PCB 的 tid 0 在同一共享 mm 中撞同一 TRAP_CONTEXT VA。
- process-private 与 mm-shared 资源已初步拆开：信号、fd、父子关系跟 clone flags 走；VMA、brk、mmap_next、mlock、用户页表生命周期跟共享 mm 走。
- exec/exit 已做基础生命周期收口：exec 安装新 MmRef 并重置 trap slot，process-style CLONE_VM 子进程退出只释放自身 trap slot，不销毁父进程仍持有的 shared mm。
- 后续改进：把跨多步的 mmap/mremap/brk 修改进一步收敛到单次 mm lock/事务入口，补 glibc `clone08`/robust futex/exec-after-CLONE_VM 专项回归。
- 验证入口：`clone02`、`clone05`、pthread/robust futex、fork/COW 回归、exec/exit mm refcount 清理、本地 `clone_vm_mmap_smoke`。

### 第五阶段：替换线性 Vec VMA 查找（主体已推进）

- `VmRegionSet` 已成为 start-keyed tree-backed VMA 集合，`Vec<VmRegion>` 只保留为
  snapshot/rollback/procfs 输出等派生视图，不再作为权威结构。
- VMA containing、overlap、snapshot、共享映射扫描和 grow-down 精确候选已使用
  predecessor/range 查询；page fault lookup、mremap containment、brk overlap 等
  外层入口都通过 `VmRegionSet` 的语义 API。
- mmap layout policy 已有 top-down/bottom-up 搜索、stack guard gap、brk gap、
  `MAP_FIXED_NOREPLACE`、`MAP_GROWSDOWN`、arch 用户 VA 限制和 per-mm ASLR。
  每个 `MemorySet` 保存 page-aligned `mmap_aslr_offset`；fork/CLONE_VM 继承同一
  地址空间布局，exec/reset 重新生成。无 hint 的非 fixed mmap 从
  `USER_VA_TOP - offset` 开始 top-down 搜索，hint 和 `MAP_FIXED` 语义不变，
  失败后仍回到底部 fallback；`shmat(NULL)` 和 `mremap(MAYMOVE)` 的自动选址也
  复用同一个 mm-local placement 入口。
- 验证：本轮 per-mm ASLR 接入已跑 `cargo fmt`、riscv64 `os` debug/release
  cargo check、loongarch64 `os` cargo check；focused riscv64 debug memory smoke
  7 个探针全部通过，debug VM invariant 未触发。
- 第五阶段继续推进：free-area search 已去掉 `occupied_user_ranges_with_metadata()`
  的临时 Vec 构造。`find_free_mmap_range()` 现在通过正向/反向 merged occupied
  scanner 实时合并 concrete `MapArea` 和权威 `VmRegionSet` 两个有序来源，再在
  hole 内做 top-down/bottom-up placement 检查。语义上仍保留 grow-down guard、
  ASLR ceiling、hint 优先、`MAP_FIXED` 外部路径和 bottom-up fallback，但后续替换为
  gap tracking / interval tree 时改动面会集中在 scanner 内。反向 scanner 按
  range end 降序取下一个候选，保证重叠桥接区间会在 emit 前被合并，输出等价于
  normalized ranges 的反向遍历。
- 验证：本轮 gap scanner 改造已跑 `cargo fmt`、riscv64 `os` debug/release
  cargo check、loongarch64 `os` cargo check；focused riscv64 debug memory smoke
  7 个探针全部通过，debug VM invariant 未触发。
- 第五阶段继续推进：`VmRegionSet::replace_normalized()` 和旧的 Vec
  `normalize_vm_region_list()` 已删除。fork/clone 继承直接 clone tree-backed
  `VmRegionSet`；snapshot/rollback 恢复、普通 VMA 插入和 file-valid metadata 更新
  都走局部 `insert_merged()`/remove+reinsert，不再把整棵 VMA tree 物化成 Vec 后
  重建。`set_file_valid_by_identity()` 也改成局部 remove + update + merge，
  保留文件增长/缩小时与相邻 VMA 的合并机会。
- 验证：本轮 normalize removal 已跑 `cargo fmt`、riscv64 `os` debug/release
  cargo check、loongarch64 `os` cargo check；focused riscv64 debug memory smoke
  7 个探针全部通过，debug VM invariant 未触发。
- 验证入口：`mmapstress`、`mallocstress`、proc maps/smaps、mincore/mlock/madvise，以及长时间混合 mmap/munmap 压测。

### 第六阶段：page fault 从 VMA 元数据驱动

- 缺页处理应先查 VMA：访问权限、grow-down、COW/shared/private、file offset、SIGBUS tail、lazy allocation 都从 `VmRegion` 推导。
- `MapArea` 逐步退化为 resident page / frame tracker 缓存，不再作为 mmap policy 的第二来源。
- 这是消除 `map_type/map_perm` 双份权威的关键步骤。
- 验证入口：mmap/mprotect/COW/fork/page-fault 重压测试、stack growth、userfaultfd 后续测试，以及 debug invariant 全开回归。
