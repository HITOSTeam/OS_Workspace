type LtpGroup = &'static [&'static str];

// Enable a group by uncommenting its line under the target architecture.
// Test names live in tasks/*.rs; this file only selects already packaged groups.
const RISCV_LTP_GROUPS: &[LtpGroup] = &[
    // 基础烟测
    // &super::LTP_TEST_POINTS,
    //
    // 进程生命周期 / exec / wait / 线程
    // &super::FORK_TASKS,
    // &super::WAITPID_TASKS,
    // &super::WAITID_TASKS,
    // &super::CLONE_WAIT_EXIT_CORE_TASKS,
    // &super::EXEC_FAMILY_CORE_TASKS,
    //
    // NOTE: CLONE_EXEC_CHROOT_GROUPS_TASKS 在默认 musl runtime 下会卡在
    // clone08，失败发生在进入内核前。官方 musl 的公开 clone() wrapper 会拒绝
    // CLONE_THREAD / CLONE_CHILD_CLEARTID / CLONE_SETTLS，并在用户态直接返回
    // EINVAL；glibc 版 clone08 能进入内核并通过。若要把这一组当作正常双 libc
    // 回归批次，需要先重建 LTP/raw clone wrapper，或者临时显式启用 LD_PRELOAD
    // adapter。可选 adapter 保留在 /extra/libltp_clone_fix.so，源码为
    // ext4-fs-packer/extra/libltp_clone_fix.S；submit_script.rs 默认不启用。
    //
    // &super::CLONE_EXEC_CHROOT_GROUPS_TASKS,
    // &super::THREADING_PTRACE_TASKS,
    // &super::PIDFD_PRCTL_TASKS,
    //
    // 进程身份 / 会话 / 调度
    // &super::PROCINFO_TASKS,
    // &super::PGRP_SESSION_TASKS,
    // &super::SETPGRP_TASKS,
    // &super::GETPRIORITY_TASKS,
    // &super::SETPRIORITY_TASKS,
    // &super::PROC_TID_TASKS,
    // &super::SCHED_NICE_CORE_TASKS,
    // &super::SCHED_TC_TASKS,
    // &["starvation"],
    // &super::UNRUN_SCHED_TASKS,
    //
    // 凭证 / capability / key
    // &super::GETRES_TASKS,
    // &super::CRED_SET_CORE_TASKS,
    // &super::CRED_SET_RES_TASKS,
    // &super::CRED_FS_TASKS,
    // &super::CRED_EGID_TASKS,
    // &super::CRED_KEY_CAP_CORE_TASKS,
    // &super::CAP_CRED16_QUERY_TASKS,
    // &super::CRED16_MUTATION_TASKS,
    //
    // 信号 / futex / eventfd / timerfd / epoll
    // &super::SIGACTION_SIGNAL_CORE_TASKS,
    // &super::KILL_PAUSE_TGKILL_TASKS,
    // &super::CLOCK_TIMERFD_SIGNALFD_TASKS,
    // &super::EVENTFD_FUTEX_TIMERFD_TASKS,
    // &super::EPOLL_CORE_TASKS,
    //
    // 基础文件 I/O / fd / fcntl
    // &super::CWD_DIR_TASKS,
    // &super::ACCESS_TASKS,
    // &super::FACCESSAT_TASKS,
    // &super::CLOSE_TASKS,
    // &super::OPEN_CORE_TASKS,
    // &super::OPEN_EXT_TASKS,
    // &super::OPENAT_CORE_TASKS,
    // &super::CLOSE_RANGE_CORE_TASKS,
    // &super::UMASK_TASKS,
    // &super::DUP_CORE_TASKS,
    // &super::DUP_FCNTL_TASKS,
    // &super::READ_WRITE_LSEEK_TASKS,
    // &super::PREAD_PWRITE_PREADV_TASKS,
    // &super::PREADV2_PWRITEV2_TASKS,
    // &super::FCNTL_BASIC_TASKS,
    // &super::FCNTL_EXTENDED_TASKS,
    // &super::FCNTL_MISC_TASKS,
    // &super::FCNTL_LEASE_TASKS,
    //
    // 文件元数据 / 目录树 / 链接 / xattr
    // &super::CHOWN_TASKS,
    // &super::CHMOD_TASKS,
    // &super::FCHMOD_TASKS,
    // &super::FCHOWN_TASKS,
    // &super::FCHDIR_TASKS,
    // &super::CREAT_CORE_TASKS,
    // &super::FSTAT_TASKS,
    // &super::FSTATFS_TASKS,
    // &super::STATFS_TASKS,
    // &super::STATX_BASIC_TASKS,
    // &super::STAT_TASKS,
    // &super::MKNODAT_TASKS,
    // &super::MKNOD_CORE_TASKS,
    // &super::MKDIR_CORE_TASKS,
    // &super::RMDIR_TASKS,
    // &super::LINK_CORE_TASKS,
    // &super::SYMLINK_CORE_TASKS,
    // &super::READLINK_CORE_TASKS,
    // &super::COPY_TRUNCATE_CORE_TASKS,
    // &super::CREAT_USERFAULTFD_TASKS,
    // &super::RENAME_UNLINK_TASKS,
    // &super::XATTR_CORE_TASKS,
    // &super::FS_META_CHOWN_XATTR_TASKS,
    // &super::FS_META_INOTIFY_XATTR_TASKS,
    // &super::STAT_LFS_EXT_TASKS,
    // &super::STATX_EXT_TASKS,
    //
    // 高级文件 I/O / pipe / splice / AIO / io_uring
    // &super::FALLOCATE_FSYNC_SYNC_TASKS,
    // &super::PIPE_CORE_TASKS,
    // &super::PIPE_SENDFILE_SPLICE_TASKS,
    // &super::TEE_VMSPLICE_FADVISE_TASKS,
    // &super::AIO_DIO_CORE_TASKS,
    // &["dma_thread_diotest"],
    // &["dma_thread_diotest -w 2"],
    // &super::IOCTL_IOURING_OPEN_TASKS,
    // &["fsync02"],
    // &["openat04"],
    //
    // mount / namespace / fanotify / proc-sysfs
    // &super::NS_MOUNT_CORE_TASKS,
    // &super::MOUNT_API_TASKS,
    // &super::NS_MOUNT_FOLLOWUP_TASKS,
    // &super::PIDNS_MODULE_TASKS,
    // &super::FANOTIFY_CORE_TASKS,
    // &super::UNAME_SYSFS_ASLR_TASKS,
    //
    // 内存管理
    // &super::MMAP_MPROTECT_CORE_TASKS,
    // verified on glibc; musl sbrk01 is an expected libc/runtime limitation
    // because this musl reports brk() as not implemented while raw brk syscall
    // and the mmap/mprotect cases pass. A separate RISC-V musl lane also
    // currently fails with status 255 when the image pairs double-float LTP
    // binaries with a soft-float /lib/ld-musl-riscv64.so.1; that is an image
    // ABI limitation, not a MemorySet/brk/mmap/mprotect semantic failure.
    // mprotect01 now returns EACCES for O_RDONLY /dev/zero MAP_SHARED
    // write-upgrade.
    // &super::MLOCK_MADVISE_CORE_TASKS,
    // verified on musl+glibc; madvise01/02 unsupported advice subcases and
    // process_madvise01 CONFIG_SWAP check are expected LTP TCONF in this image.
    // &super::MPROTECT_MREMAP_MSYNC_TASKS,
    // &super::MM_MMAP_MADVISE_TASKS,
    // verified on glibc; musl mmapstress02/03/05/06 are expected
    // libc/runtime sbrk limitations in this image. mmap3 uses a smaller
    // -l 1 -n 4 profile because LTP's fixed 60s stress workload must let
    // worker threads finish cleanup before the 90s harness timeout.
    // mmap16, mmapstress08, mlock2, memory-failure, memcg, core-dump, and
    // hugepage-only subcases are expected TCONF for the current image/config.
    // &super::MM_OOM_TASKS,
    // verified on musl+glibc; oom02-05 are expected TCONF in this image
    // because the bundled LTP lacks libnuma development support.
    // UNRUN_MM_TASKS status:
    // data_space, kallsyms, mem02, mtest01, stack_space, thp01, vma01, and
    // min_free_kbytes pass on musl+glibc.
    // ksm01/03/05/07 are expected CONFIG_KSM TCONF in this image;
    // ksm02/04/06 are expected libnuma TCONF in this image.
    // swapping01 is expected CONFIG_SWAP TCONF in this image.
    // thp02-04 are expected TCONF because transparent huge pages / huge
    // pages are unsupported in the current kernel config.
    // vma02/vma04 are expected TCONF in this image because the bundled LTP
    // lacks libnuma development support; vma03 is expected TCONF on riscv64
    // because it is a 32-bit mmap2 overflow regression test.
    // max_map_count passes on musl+glibc after MAP_SHARED anonymous mmap
    // became lazy and procfs read caches large /proc/<pid>/maps snapshots.
    // &super::UNRUN_MM_TASKS,
    // UNRUN_HUGETLB_TASKS are expected TCONF in this image because
    // hugetlbfs/huge pages are unsupported; hugeshmat04 also requires at
    // least 2048MB MemAvailable.
    // &super::UNRUN_HUGETLB_TASKS,
    // UNRUN_NUMA_TASKS are expected TCONF in this image: migrate_pages*,
    // move_pages*, and set_mempolicy01-04 require libnuma development
    // support; set_mempolicy05 is arch-gated by LTP.
    // &super::UNRUN_NUMA_TASKS,
    // numa01.sh is expected TCONF in this image because bc is absent.
    // &super::UNRUN_NUMA_SHELL_TASKS,
    // &super::UNRUN_MALLOC_TASKS,
    // vma05.sh is expected TCONF in this image because gdb is absent; a
    // real run would also need coredump/core_uses_pid support.
    // &super::UNRUN_VMA_SHELL_TASKS,
    //
    // IPC / POSIX MQ / SysV IPC
    // &super::SYSV_IPC_CORE_TASKS,
    // &super::SYSV_IPC_EXT_TASKS,
    // &super::POSIX_MQ_SYSV_MSG_SEM_TASKS,
    // &["shmat02", "shmat03", "shmat04", "shmdt01", "shmdt02", "shmt02"],
    // &super::SYSV_SHM_CORE_TASKS,
    // &super::SYSV_SHM_FOLLOWUP_TASKS,
    // &super::IPC_NAMESPACE_TASKS,
    // &super::SYSV_MSG_STRESS_TASKS,
    //
    // 网络 / socket / net command
    // &super::NET_SOCKET_CONN_TASKS,
    // &super::NET_SEND_RECV_TASKS,
    // recvmmsg01/sendmsg01 are glibc-only below: the bundled musl wrappers
    // dereference intentionally bad user pointers before entering the kernel.
    // &super::NET_SOCKOPT_POLL_TASKS,
    // &super::UNRUN_NET_SCTP_TASKS,
    // &super::UNRUN_NET_IPV6_LIB_TASKS,
    // &super::UNRUN_NET_FEATURES_TASKS,
    // &super::UNRUN_VSOCK_TASKS,
    // &super::UNRUN_NET_TC_ROUTE_TASKS,
    // &super::UNRUN_NET_CMDS_TASKS,
    // &super::UNRUN_NET_MULTICAST_TASKS,
    // &super::UNRUN_NET_NFS_TASKS,
    // &super::UNRUN_NET_TCP_CC_TASKS,
    // &super::UNRUN_NET_SCTP_DCCP_TASKS,
    // &super::UNRUN_NET_TUNNEL_TASKS,
    // &super::UNRUN_NET_OVERLAY_TASKS,
    // &super::UNRUN_NET_VIRT_TASKS,
    // &super::UNRUN_NET_BROKEN_IP_TASKS,
    // &super::UNRUN_CAN_TASKS,
    // &super::UNRUN_NFT_TASKS,
    // &super::UNRUN_TRACEROUTE_TASKS,
    //
    // 时间 / 资源 / 系统信息
    // &super::GETITIMER_TASKS,
    // &super::SETITIMER_TASKS,
    // &super::GETRUSAGE_TASKS,
    // &super::CLOCK_GETTIME_TASKS,
    // &super::CLOCK_SETTIME_TASKS,
    // &super::CLOCK_RES_TASKS,
    // &super::CLOCK_NANOSLEEP_TASKS,
    // &super::TIME_MISC_TASKS,
    // &super::ALARM_TASKS,
    // &super::POSIX_TIMER_TASKS,
    // &super::ADJTIMEX_SETTIMEOFDAY_UTIME_TASKS,
    // &super::UTIME_UNAME_ARCH_PRCTL_TASKS,
    // &super::SETRLIMIT_TASKS,
    // &super::GETRLIMIT_TASKS,
    // &super::ROBUST_TID_TASKS,
    // NOTE：
    // pathconf02 is expected to fail on musl: musl pathconf() ignores the
    // path argument for _PC_LINK_MAX and returns a constant instead of errno.
    //
    // &super::IO_PERF_SYSINFO_PATHCONF_TASKS,
    // &super::KCMP_TASKS,
    //
    // UTS / random / misc identity
    // &super::UTS_NAME_TASKS,
    // &super::UTS_QUERY_TASKS,
    // &super::GETRANDOM_TASKS,
    //
    // cgroup / controller / freezer / resource controllers
    // &super::CGROUP_CORE_CPU_TASKS,
    // &super::CGROUP_MEM_PID_TASKS,
    // &super::UNRUN_CONTROLLERS_TASKS,
    // &super::UNRUN_CONTAINERS_TASKS,
    // &super::UNRUN_MEMCG_CTRL_TASKS,
    // &super::UNRUN_CGROUP_REGRESSION_TASKS,
    // &super::UNRUN_CGROUP_CTRL_SHELL_TASKS,
    //
    // security / crypto / CVE / LSM
    // &super::SECURITY_KEYS_CRYPTO_TASKS,
    // &super::SECURITY_BPF_CVE_TASKS,
    // &super::UNRUN_SMACK_TASKS,
    // &super::UNRUN_SMACK_SHELL_TASKS,
    // &super::UNRUN_SECURITY_HARDENING_TASKS,
    // &super::UNRUN_IMA_TASKS,
    // &super::UNRUN_SOUND_CVE_TASKS,
    //
    // 命令 / 设备 / 内核杂项 / 压力类
    // &super::UNRUN_SYSCALLS_TASKS,
    // &super::UNRUN_FS_TASKS,
    // &super::UNRUN_KERNEL_MISC_TASKS,
    // &super::UNRUN_MATH_TASKS,
    // &super::UNRUN_PTY_TASKS,
    // &super::UNRUN_WATCHQUEUE_TASKS,
    // &super::UNRUN_INPUT_TASKS,
    // &super::UNRUN_CRASHME_TASKS,
    // &super::UNRUN_UEVENT_TASKS,
    // &super::UNRUN_IRQ_TASKS,
    // &super::UNRUN_PM_BINARY_TASKS,
    // &super::UNRUN_MISC_DEVICE_TASKS,
    // &super::UNRUN_COMMANDS_TASKS,
    // &super::UNRUN_FS_BIND_TASKS,
    // &super::UNRUN_FS_RACER_TASKS,
    // &super::UNRUN_FSX_TASKS,
    // &super::UNRUN_FS_ADVANCED_TASKS,
    // &super::UNRUN_CPUHOTPLUG_TASKS,
    // &super::UNRUN_PM_SHELL_TASKS,
    // &super::UNRUN_TPM_TASKS,
    // &super::UNRUN_TRACING_TASKS,
    // &super::UNRUN_MISC_KERNEL_SHELL_TASKS,
    // &super::UNRUN_ZRAM_TASKS,
    // &super::UNRUN_LOCK_RCU_TASKS,
    // &super::UNRUN_FS_LINK_TASKS,
    // &super::UNRUN_FS_READONLY_TASKS,
    //
    // 综合批次
    // &super::CORE100_TASKS,
];

const RISCV_GLIBC_ONLY_LTP_GROUPS: &[LtpGroup] = &[
    // 网络 / socket libc-wrapper-sensitive error paths
    // &super::NET_SEND_RECV_GLIBC_ONLY_TASKS,
];

const LOONGARCH_LTP_GROUPS: &[LtpGroup] = &[
    // 基础烟测
    // &super::LTP_TEST_POINTS,
    //
    // 进程生命周期 / exec / wait / 线程
    // &super::FORK_TASKS,
    // &super::WAITPID_TASKS,
    // &super::WAITID_TASKS,
    // &super::CLONE_WAIT_EXIT_CORE_TASKS,
    // &super::EXEC_FAMILY_CORE_TASKS,
    // &super::CLONE_EXEC_CHROOT_GROUPS_TASKS,
    // &super::THREADING_PTRACE_TASKS,
    // &super::PIDFD_PRCTL_TASKS,
    //
    // 进程身份 / 会话 / 调度
    // &super::PROCINFO_TASKS,
    // &super::PGRP_SESSION_TASKS,
    // &super::SETPGRP_TASKS,
    // &super::GETPRIORITY_TASKS,
    // &super::SETPRIORITY_TASKS,
    // &super::PROC_TID_TASKS,
    // &super::SCHED_NICE_CORE_TASKS,
    // &super::SCHED_TC_TASKS,
    // &super::UNRUN_SCHED_TASKS,
    //
    // 凭证 / capability / key
    // &super::GETRES_TASKS,
    // &super::CRED_SET_CORE_TASKS,
    // &super::CRED_SET_RES_TASKS,
    // &super::CRED_FS_TASKS,
    // &super::CRED_EGID_TASKS,
    // &super::CRED_KEY_CAP_CORE_TASKS,
    // &super::CAP_CRED16_QUERY_TASKS,
    // &super::CRED16_MUTATION_TASKS,
    //
    // 信号 / futex / eventfd / timerfd / epoll
    // &super::SIGACTION_SIGNAL_CORE_TASKS,
    // &super::KILL_PAUSE_TGKILL_TASKS,
    // &super::CLOCK_TIMERFD_SIGNALFD_TASKS,
    // &super::EVENTFD_FUTEX_TIMERFD_TASKS,
    // &super::EPOLL_CORE_TASKS,
    //
    // 基础文件 I/O / fd / fcntl
    // &super::CWD_DIR_TASKS,
    // &super::ACCESS_TASKS,
    // &super::FACCESSAT_TASKS,
    // &super::CLOSE_TASKS,
    // &super::OPEN_CORE_TASKS,
    // &super::OPEN_EXT_TASKS,
    // &super::OPENAT_CORE_TASKS,
    // &super::CLOSE_RANGE_CORE_TASKS,
    // &super::UMASK_TASKS,
    // &super::DUP_CORE_TASKS,
    // &super::DUP_FCNTL_TASKS,
    // &super::READ_WRITE_LSEEK_TASKS,
    // &super::PREAD_PWRITE_PREADV_TASKS,
    // &super::PREADV2_PWRITEV2_TASKS,
    // &super::FCNTL_BASIC_TASKS,
    // &super::FCNTL_EXTENDED_TASKS,
    // &super::FCNTL_MISC_TASKS,
    // &super::FCNTL_LEASE_TASKS,
    //
    // 文件元数据 / 目录树 / 链接 / xattr
    // &super::CHOWN_TASKS,
    // &super::CHMOD_TASKS,
    // &super::FCHMOD_TASKS,
    // &super::FCHOWN_TASKS,
    // &super::FCHDIR_TASKS,
    // &super::CREAT_CORE_TASKS,
    // &super::FSTAT_TASKS,
    // &super::FSTATFS_TASKS,
    // &super::STATFS_TASKS,
    // &super::STATX_BASIC_TASKS,
    // &super::STAT_TASKS,
    // &super::MKNODAT_TASKS,
    // &super::MKNOD_CORE_TASKS,
    // &super::MKDIR_CORE_TASKS,
    // &super::RMDIR_TASKS,
    // &super::LINK_CORE_TASKS,
    // &super::SYMLINK_CORE_TASKS,
    // &super::READLINK_CORE_TASKS,
    // &super::COPY_TRUNCATE_CORE_TASKS,
    // &super::CREAT_USERFAULTFD_TASKS,
    // &super::RENAME_UNLINK_TASKS,
    // &super::XATTR_CORE_TASKS,
    // &super::FS_META_CHOWN_XATTR_TASKS,
    // &super::FS_META_INOTIFY_XATTR_TASKS,
    // &super::STAT_LFS_EXT_TASKS,
    // &super::STATX_EXT_TASKS,
    //
    // 高级文件 I/O / pipe / splice / AIO / io_uring
    // &super::FALLOCATE_FSYNC_SYNC_TASKS,
    // &super::PIPE_CORE_TASKS,
    // &super::PIPE_SENDFILE_SPLICE_TASKS,
    // &super::TEE_VMSPLICE_FADVISE_TASKS,
    // &super::AIO_DIO_CORE_TASKS,
    // &super::IOCTL_IOURING_OPEN_TASKS,
    //
    // mount / namespace / fanotify / proc-sysfs
    // &super::NS_MOUNT_CORE_TASKS,
    // &super::MOUNT_API_TASKS,
    // &super::NS_MOUNT_FOLLOWUP_TASKS,
    // &super::PIDNS_MODULE_TASKS,
    // &super::FANOTIFY_CORE_TASKS,
    // &super::UNAME_SYSFS_ASLR_TASKS,
    //
    // 内存管理
    // &super::MMAP_MPROTECT_CORE_TASKS,
    // &super::MLOCK_MADVISE_CORE_TASKS,
    // &super::MPROTECT_MREMAP_MSYNC_TASKS,
    // &super::MM_MMAP_MADVISE_TASKS,
    // &super::MM_OOM_TASKS,
    // &super::UNRUN_MM_TASKS,
    // &super::UNRUN_HUGETLB_TASKS,
    // &super::UNRUN_NUMA_TASKS,
    // &super::UNRUN_NUMA_SHELL_TASKS,
    // &super::UNRUN_MALLOC_TASKS,
    // &super::UNRUN_VMA_SHELL_TASKS,
    //
    // IPC / POSIX MQ / SysV IPC
    // &super::SYSV_IPC_CORE_TASKS,
    // &super::SYSV_IPC_EXT_TASKS,
    // &super::POSIX_MQ_SYSV_MSG_SEM_TASKS,
    // &super::SYSV_SHM_CORE_TASKS,
    // &super::SYSV_SHM_FOLLOWUP_TASKS,
    // &super::IPC_NAMESPACE_TASKS,
    // &super::SYSV_MSG_STRESS_TASKS,
    //
    // 网络 / socket / net command
    // &super::NET_SOCKET_CONN_TASKS,
    // &super::NET_SEND_RECV_TASKS,
    // &super::NET_SOCKOPT_POLL_TASKS,
    // &super::UNRUN_NET_SCTP_TASKS,
    // &super::UNRUN_NET_IPV6_LIB_TASKS,
    // &super::UNRUN_NET_FEATURES_TASKS,
    // &super::UNRUN_VSOCK_TASKS,
    // &super::UNRUN_NET_TC_ROUTE_TASKS,
    // &super::UNRUN_NET_CMDS_TASKS,
    // &super::UNRUN_NET_MULTICAST_TASKS,
    // &super::UNRUN_NET_NFS_TASKS,
    // &super::UNRUN_NET_TCP_CC_TASKS,
    // &super::UNRUN_NET_SCTP_DCCP_TASKS,
    // &super::UNRUN_NET_TUNNEL_TASKS,
    // &super::UNRUN_NET_OVERLAY_TASKS,
    // &super::UNRUN_NET_VIRT_TASKS,
    // &super::UNRUN_NET_BROKEN_IP_TASKS,
    // &super::UNRUN_CAN_TASKS,
    // &super::UNRUN_NFT_TASKS,
    // &super::UNRUN_TRACEROUTE_TASKS,
    //
    // 时间 / 资源 / 系统信息
    // &super::GETITIMER_TASKS,
    // &super::SETITIMER_TASKS,
    // &super::GETRUSAGE_TASKS,
    // &super::CLOCK_GETTIME_TASKS,
    // &super::CLOCK_SETTIME_TASKS,
    // &super::CLOCK_RES_TASKS,
    // &super::CLOCK_NANOSLEEP_TASKS,
    // &super::TIME_MISC_TASKS,
    // &super::ALARM_TASKS,
    // &super::POSIX_TIMER_TASKS,
    // &super::ADJTIMEX_SETTIMEOFDAY_UTIME_TASKS,
    // &super::UTIME_UNAME_ARCH_PRCTL_TASKS,
    // &super::SETRLIMIT_TASKS,
    // &super::GETRLIMIT_TASKS,
    // &super::ROBUST_TID_TASKS,
    // NOTE:
    // pathconf02 is expected to fail on musl: musl pathconf() ignores the
    // path argument for _PC_LINK_MAX and returns a constant instead of errno.
    //
    // &super::IO_PERF_SYSINFO_PATHCONF_TASKS,
    // &super::KCMP_TASKS,
    //
    // UTS / random / misc identity
    // &super::UTS_NAME_TASKS,
    // &super::UTS_QUERY_TASKS,
    // &super::GETRANDOM_TASKS,
    //
    // cgroup / controller / freezer / resource controllers
    // &super::CGROUP_CORE_CPU_TASKS,
    // &super::CGROUP_MEM_PID_TASKS,
    // &super::UNRUN_CONTROLLERS_TASKS,
    // &super::UNRUN_CONTAINERS_TASKS,
    // &super::UNRUN_MEMCG_CTRL_TASKS,
    // &super::UNRUN_CGROUP_REGRESSION_TASKS,
    // &super::UNRUN_CGROUP_CTRL_SHELL_TASKS,
    //
    // security / crypto / CVE / LSM
    // &super::SECURITY_KEYS_CRYPTO_TASKS,
    // &super::SECURITY_BPF_CVE_TASKS,
    // &super::UNRUN_SMACK_TASKS,
    // &super::UNRUN_SMACK_SHELL_TASKS,
    // &super::UNRUN_SECURITY_HARDENING_TASKS,
    // &super::UNRUN_IMA_TASKS,
    // &super::UNRUN_SOUND_CVE_TASKS,
    //
    // 命令 / 设备 / 内核杂项 / 压力类
    // &super::UNRUN_SYSCALLS_TASKS,
    // &super::UNRUN_FS_TASKS,
    // &super::UNRUN_KERNEL_MISC_TASKS,
    // &super::UNRUN_MATH_TASKS,
    // &super::UNRUN_PTY_TASKS,
    // &super::UNRUN_WATCHQUEUE_TASKS,
    // &super::UNRUN_INPUT_TASKS,
    // &super::UNRUN_CRASHME_TASKS,
    // &super::UNRUN_UEVENT_TASKS,
    // &super::UNRUN_IRQ_TASKS,
    // &super::UNRUN_PM_BINARY_TASKS,
    // &super::UNRUN_MISC_DEVICE_TASKS,
    // &super::UNRUN_COMMANDS_TASKS,
    // &super::UNRUN_FS_BIND_TASKS,
    // &super::UNRUN_FS_RACER_TASKS,
    // &super::UNRUN_FSX_TASKS,
    // &super::UNRUN_FS_ADVANCED_TASKS,
    // &super::UNRUN_CPUHOTPLUG_TASKS,
    // &super::UNRUN_PM_SHELL_TASKS,
    // &super::UNRUN_TPM_TASKS,
    // &super::UNRUN_TRACING_TASKS,
    // &super::UNRUN_MISC_KERNEL_SHELL_TASKS,
    // &super::UNRUN_ZRAM_TASKS,
    // &super::UNRUN_LOCK_RCU_TASKS,
    // &super::UNRUN_FS_LINK_TASKS,
    // &super::UNRUN_FS_READONLY_TASKS,
    //
    // 综合批次
    // &super::CORE100_TASKS,
];

const LOONGARCH_GLIBC_ONLY_LTP_GROUPS: &[LtpGroup] = &[
    // 网络 / socket libc-wrapper-sensitive error paths
    // &super::NET_SEND_RECV_GLIBC_ONLY_TASKS,
];

/// 对所有groups 里的组逐个运行测试
/// groups是一个二维数组
/// 使用 run_group 对其 运行测试
fn run_ltp_groups(run_group: fn(&str, &[&str]), groups: &[LtpGroup]) {
    for &tasks in groups {
        run_group("/musl", tasks);
        run_group("/glibc", tasks);
    }
}

fn run_ltp_groups_in_dir(run_group: fn(&str, &[&str]), dir: &str, groups: &[LtpGroup]) {
    for &tasks in groups {
        run_group(dir, tasks);
    }
}

pub fn run_riscv_ltp_groups(run_group: fn(&str, &[&str])) {
    run_ltp_groups(run_group, RISCV_LTP_GROUPS);
    run_ltp_groups_in_dir(run_group, "/glibc", RISCV_GLIBC_ONLY_LTP_GROUPS);
}

pub fn run_non_riscv_ltp_groups(run_group: fn(&str, &[&str])) {
    run_ltp_groups(run_group, LOONGARCH_LTP_GROUPS);
    run_ltp_groups_in_dir(run_group, "/glibc", LOONGARCH_GLIBC_ONLY_LTP_GROUPS);
}
