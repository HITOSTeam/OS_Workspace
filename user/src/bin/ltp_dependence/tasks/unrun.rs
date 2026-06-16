//! UNRUN — LTP tests present in `ltp_all.md` but not yet wired into
//! `submit_plan.rs`.
//!
//! Every array in this module represents an LTP *runtest suite* chunk
//! that the workspace has **not** tried on our kernel yet. Promote any
//! group by adding a `RUN_*` toggle + `run_for_both_libcs(...)` call in
//! [`super::super::submit_plan`]. Groups stay here (not in an active
//! batch file) until their syscall prerequisites are implemented and
//! regression-checked.
//!
//! Regenerate this file with:
//!
//! ```text
//! python3 tools/find_unimplement_ltp.py --no-compress --output /tmp/missing.txt
//! ```

#![allow(dead_code)]

// Miscellaneous syscalls not covered by the per-family lists in other tasks/*.rs files.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/syscalls
pub const UNRUN_SYSCALLS_TASKS: [&str; 92] = [
    "cacheflush01",
    "dirtyc0w",
    "dirtyc0w_shmem",
    "dirtypipe",
    "flock01",
    "flock02",
    "flock03",
    "flock04",
    "flock06",
    "get_mempolicy01",
    "get_mempolicy02",
    "getcontext01",
    "gethostbyname_r01",
    "gethostid01",
    "gethostname02",
    "getrusage03",
    "getrusage04",
    "ioctl_ns01",
    "ioctl_ns02",
    "ioctl_ns03",
    "ioctl_ns04",
    "ioctl_ns05",
    "ioctl_ns06",
    "ioctl_ns07",
    "kcmp02",
    "kcmp03",
    "lseek11",
    "mallinfo02",
    "mallinfo2_01",
    "mallopt01",
    "mbind01",
    "mbind02",
    "mbind03",
    "mbind04",
    "memcmp01",
    "memcpy01",
    "memset01",
    "modify_ldt01",
    "modify_ldt02",
    "modify_ldt03",
    "name_to_handle_at01",
    "name_to_handle_at02",
    "nftw01",
    "nftw6401",
    "open_by_handle_at01",
    "open_by_handle_at02",
    "pivot_root01",
    "pkey01",
    "posix_fadvise01_64",
    "posix_fadvise02_64",
    "posix_fadvise03_64",
    "posix_fadvise04_64",
    "preadv203",
    "preadv203_64",
    "process_vm_readv02",
    "process_vm_readv03",
    "process_vm_writev02",
    "profil01",
    "prot_hsymlinks",
    "pwritev03",
    "pwritev03_64",
    "quotactl01",
    "quotactl02",
    "quotactl03",
    "quotactl04",
    "quotactl05",
    "quotactl06",
    "quotactl07",
    "quotactl08",
    "quotactl09",
    "realpath01",
    "reboot01",
    "reboot02",
    "remap_file_pages01",
    "remap_file_pages02",
    "sgetmask01",
    "ssetmask01",
    "statvfs01",
    "statvfs02",
    "string01",
    "swapoff01",
    "swapoff02",
    "swapon01",
    "swapon02",
    "swapon03",
    "syslog11",
    "syslog12",
    "ulimit01",
    "ustat01",
    "ustat02",
    "vhangup01",
    "vhangup02",
];

// Huge-page / hugetlbfs backed mmap/shm tests.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/hugetlb
pub const UNRUN_HUGETLB_TASKS: [&str; 48] = [
    "hugefallocate01",
    "hugefallocate02",
    "hugefork01",
    "hugefork02",
    "hugemmap01",
    "hugemmap02",
    "hugemmap04",
    "hugemmap05",
    "hugemmap06",
    "hugemmap07",
    "hugemmap08",
    "hugemmap09",
    "hugemmap10",
    "hugemmap11",
    "hugemmap12",
    "hugemmap13",
    "hugemmap14",
    "hugemmap15",
    "hugemmap16",
    "hugemmap17",
    "hugemmap18",
    "hugemmap19",
    "hugemmap20",
    "hugemmap21",
    "hugemmap22",
    "hugemmap23",
    "hugemmap24",
    "hugemmap25",
    "hugemmap26",
    "hugemmap27",
    "hugemmap28",
    "hugemmap29",
    "hugemmap30",
    "hugemmap31",
    "hugemmap32",
    "hugeshmat01",
    "hugeshmat02",
    "hugeshmat03",
    "hugeshmat04",
    "hugeshmat05",
    "hugeshmctl01",
    "hugeshmctl02",
    "hugeshmctl03",
    "hugeshmdt01",
    "hugeshmget01",
    "hugeshmget02",
    "hugeshmget03",
    "hugeshmget05",
];

// SCTP protocol tests.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/net.sctp
pub const UNRUN_NET_SCTP_TASKS: [&str; 41] = [
    "test_1_to_1_accept_close",
    "test_1_to_1_addrs",
    "test_1_to_1_connect",
    "test_1_to_1_connectx",
    "test_1_to_1_events",
    "test_1_to_1_initmsg_connect",
    "test_1_to_1_nonblock",
    "test_1_to_1_recvfrom",
    "test_1_to_1_recvmsg",
    "test_1_to_1_rtoinfo",
    "test_1_to_1_send",
    "test_1_to_1_sendmsg",
    "test_1_to_1_sendto",
    "test_1_to_1_shutdown",
    "test_1_to_1_socket_bind_listen",
    "test_1_to_1_sockopt",
    "test_1_to_1_threads",
    "test_assoc_abort",
    "test_assoc_shutdown",
    "test_autoclose",
    "test_basic",
    "test_basic_v6",
    "test_connect",
    "test_connectx",
    "test_fragments",
    "test_fragments_v6",
    "test_getname",
    "test_getname_v6",
    "test_inaddr_any",
    "test_inaddr_any_v6",
    "test_peeloff",
    "test_peeloff_v6",
    "test_recvmsg",
    "test_sctp_sendrecvmsg",
    "test_sctp_sendrecvmsg_v6",
    "test_sockopt",
    "test_sockopt_v6",
    "test_tcp_style",
    "test_tcp_style_v6",
    "test_timetolive",
    "test_timetolive_v6",
];

// Advanced memory-management cases (OOM/huge/KSM/THP/shm).
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/mm
pub const UNRUN_MM_TASKS: [&str; 23] = [
    "data_space",
    "kallsyms",
    "ksm01",
    "ksm02",
    "ksm03",
    "ksm04",
    "ksm05",
    "ksm06",
    "ksm07",
    "max_map_count",
    "mem02",
    "min_free_kbytes",
    "mtest01",
    "stack_space",
    "swapping01",
    "thp01",
    "thp02",
    "thp03",
    "thp04",
    "vma01",
    "vma02",
    "vma03",
    "vma04",
];

// NUMA policy / mbind / migrate_pages tests.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/numa
pub const UNRUN_NUMA_TASKS: [&str; 19] = [
    "migrate_pages01",
    "migrate_pages02",
    "migrate_pages03",
    "move_pages01",
    "move_pages02",
    "move_pages03",
    "move_pages04",
    "move_pages05",
    "move_pages06",
    "move_pages07",
    "move_pages09",
    "move_pages10",
    "move_pages11",
    "move_pages12",
    "set_mempolicy01",
    "set_mempolicy02",
    "set_mempolicy03",
    "set_mempolicy04",
    "set_mempolicy05",
];

// Filesystem integration, quota, AIO, lazytime, ftest stress.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/fs
pub const UNRUN_FS_TASKS: [&str; 18] = [
    "fs_di",
    "fs_fill",
    "ftest01",
    "ftest02",
    "ftest03",
    "ftest04",
    "ftest05",
    "ftest06",
    "ftest07",
    "ftest08",
    "inode01",
    "inode02",
    "squashfs01",
    "stream01",
    "stream02",
    "stream03",
    "stream04",
    "stream05",
];

// Miscellaneous kernel facility tests (uaccess, ftrace, tracing).
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/kernel_misc
pub const UNRUN_KERNEL_MISC_TASKS: [&str; 12] = [
    "block_dev",
    "cpufreq_boost",
    "fw_load",
    "kmsg01",
    "ltp_acpi",
    "rtc01",
    "rtc02",
    "tbio",
    "tpci",
    "uaccess",
    "umip_basic_test",
    "zram03",
];

// Arch-math smoke (float01/abs/atof/...).
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/math
pub const UNRUN_MATH_TASKS: [&str; 9] = [
    "atof01",
    "float_bessel",
    "float_exp_log",
    "float_iperb",
    "float_power",
    "float_trigo",
    "fptest01",
    "fptest02",
    "nextafter01",
];

// Pseudoterminal / line-discipline tests.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/pty
pub const UNRUN_PTY_TASKS: [&str; 9] = [
    "hangup01", "ptem01", "pty01", "pty02", "pty03", "pty04", "pty05", "pty06", "pty07",
];

// Keyctl watch-queue notification tests.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/watchqueue
pub const UNRUN_WATCHQUEUE_TASKS: [&str; 9] = [
    "wqueue01", "wqueue02", "wqueue03", "wqueue04", "wqueue05", "wqueue06", "wqueue07", "wqueue08",
    "wqueue09",
];

// IPv6 protocol library tests.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/net.ipv6_lib
pub const UNRUN_NET_IPV6_LIB_TASKS: [&str; 6] = [
    "asapi_01",
    "asapi_02",
    "asapi_03",
    "getaddrinfo_01",
    "in6_01",
    "in6_02",
];

// Input subsystem / evdev tests.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/input
pub const UNRUN_INPUT_TASKS: [&str; 6] = [
    "input01", "input02", "input03", "input04", "input05", "input06",
];

// Scheduler / deadline / cfs tests.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/sched
pub const UNRUN_SCHED_TASKS: [&str; 3] = ["autogroup01", "proc_sched_rt01", "starvation"];

// Controller-Area-Network socket tests.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/can
pub const UNRUN_CAN_TASKS: [&str; 3] = ["can_bcm01", "can_filter", "can_rcv_own_msgs"];

// Random-instruction crash-me stress.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/crashme
pub const UNRUN_CRASHME_TASKS: [&str; 3] = ["crash01", "crash02", "f00f"];

// Udev / uevent tests.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/uevent
pub const UNRUN_UEVENT_TASKS: [&str; 3] = ["uevent01", "uevent02", "uevent03"];

// Network feature probes.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/net.features
pub const UNRUN_NET_FEATURES_TASKS: [&str; 1] = ["fanout01"];

// Interrupt-handling tests.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/irq
pub const UNRUN_IRQ_TASKS: [&str; 1] = ["irqbalance01"];

// cgroup-controller scenarios not in cgroup batches.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/controllers
pub const UNRUN_CONTROLLERS_TASKS: [&str; 1] = ["memcg_test_3"];

// Container / namespace stress not in NS batches.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/containers
pub const UNRUN_CONTAINERS_TASKS: [&str; 1] = ["netns_netlink"];

// Smack LSM tests.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/smack
pub const UNRUN_SMACK_TASKS: [&str; 1] = ["smack_set_socket_labels"];

// Total UNRUN entries: 309
