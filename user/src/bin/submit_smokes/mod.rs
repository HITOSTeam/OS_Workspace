pub const RUN_VFS_SMOKES: bool = false;
pub const VFS_SMOKES: [&str; 3] = [
    "/user/path_cache_invalidation_smoke.bin",
    "/user/pending_write_stat_smoke.bin",
    "/user/exec_write_count_smoke.bin",
];

pub const RUN_READINESS_SMOKES: bool = false;
pub const READINESS_SMOKES: [&str; 15] = [
    "/user/nested_epoll_smoke.bin",
    "/user/nested_epoll_ctl_wakeup_smoke.bin",
    "/user/nested_epoll_ctl_del_smoke.bin",
    "/user/nested_epoll_et_smoke.bin",
    "/user/nested_epoll_et_maxevents_smoke.bin",
    "/user/nested_epoll_oneshot_smoke.bin",
    "/user/nested_epoll_parent_oneshot_smoke.bin",
    "/user/epoll_ctl_wakeup_smoke.bin",
    "/user/eventfd_epoll_smoke.bin",
    "/user/mq_epoll_smoke.bin",
    "/user/mq_notify_signal_smoke.bin",
    "/user/mq_unlink_epoll_smoke.bin",
    "/user/timerfd_epoll_smoke.bin",
    "/user/regular_file_select_smoke.bin",
    "/user/dup3_lock_cleanup_smoke.bin",
];

pub const RUN_PROCFS_SMOKES: bool = false;
pub const PROCFS_SMOKES: [&str; 3] = [
    "/user/proc_magic_links_smoke.bin",
    "/user/mount_namespace_smoke.bin",
    "/user/proc_maps_stack_smoke.bin",
];

pub const RUN_MEMORY_SMOKES: bool = false;
pub const MEMORY_SMOKES: [&str; 15] = [
    "/user/file_mmap_lazy_fault_smoke.bin",
    "/user/shared_file_alias_smoke.bin",
    "/user/shared_file_cross_mm_smoke.bin",
    "/user/shared_file_kernel_write_smoke.bin",
    "/user/shared_file_fault_cache_smoke.bin",
    "/user/shared_file_truncate_cache_smoke.bin",
    "/user/cow_mprotect_smoke.bin",
    "/user/clone_vm_mmap_smoke.bin",
    "/user/clone_vm_sysv_shm_smoke.bin",
    "/user/memfd_mremap_shared_smoke.bin",
    "/user/sysv_shm_mremap_smoke.bin",
    "/user/mmap_placement_smoke.bin",
    "/user/growsdown_guard_smoke.bin",
    "/user/stack_madvise_dontneed_smoke.bin",
    "/user/private_file_madvise_dontneed_smoke.bin",
];
