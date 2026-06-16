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
    "fs_di -d /fs_di_ltp",
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
pub const UNRUN_NET_IPV6_LIB_TASKS: [&str; 5] =
    ["asapi_02", "asapi_03", "getaddrinfo_01", "in6_01", "in6_02"];

// The bundled musl getprotobyname() table does not resolve "hopopt", while
// glibc reads the rootfs protocols database and reaches the kernel checks.
pub const UNRUN_NET_IPV6_LIB_GLIBC_ONLY_TASKS: [&str; 1] = ["asapi_01"];

// Input subsystem / evdev tests.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/input
pub const UNRUN_INPUT_TASKS: [&str; 6] = [
    "input01", "input02", "input03", "input04", "input05", "input06",
];

// Scheduler / deadline / cfs tests.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/sched
pub const UNRUN_SCHED_TASKS: [&str; 3] = [
    "autogroup01",
    "proc_sched_rt01",
    "starvation -l 50000 -t 60",
];

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
pub const UNRUN_SMACK_TASKS: [&str; 2] = ["smack_set_socket_labels", "smack_notroot"];

// Memory allocator utility tests.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/syscalls
pub const UNRUN_MALLOC_TASKS: [&str; 2] = ["mallinfo01", "mallocstress"];

// Security hardening / CPU side-channel tests.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/cve (meltdown)
//                testsuits-for-oskernel/ltp-full-20240524/runtest/mm (stack_clash)
pub const UNRUN_SECURITY_HARDENING_TASKS: [&str; 2] = ["meltdown", "stack_clash"];

// cgroup memory controller tests not in the existing UNRUN_CONTROLLERS_TASKS.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/controllers
pub const UNRUN_MEMCG_CTRL_TASKS: [&str; 4] = [
    "memcg_test_1",
    "memcg_test_2",
    "memcg_test_4",
    "memctl_test01",
];

// Virtual-socket (AF_VSOCK) test.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/syscalls
pub const UNRUN_VSOCK_TASKS: [&str; 1] = ["vsock01"];

// Network traffic control / routing binary tests.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/net.ipv6 and net.tcp_cmds
pub const UNRUN_NET_TC_ROUTE_TASKS: [&str; 6] = [
    "nft02",
    "route-change-netlink",
    "route4-rmmod",
    "route6-rmmod",
    "tcindex01",
    "icmp_rate_limit01",
];

// Power-management scheduler helper.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/testcases/kernel/power_management
pub const UNRUN_PM_BINARY_TASKS: [&str; 1] = ["pm_get_sched_values"];

// Miscellaneous device / ioctl tests.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/syscalls
pub const UNRUN_MISC_DEVICE_TASKS: [&str; 2] = ["eject_check_tray", "test_ioctl"];

// Commands / shell-wrapper tests (LTP "commands" runtest suite).
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/commands
pub const UNRUN_COMMANDS_TASKS: [&str; 28] = [
    "ar01.sh",
    "cp_tests.sh",
    "cpio_tests.sh",
    "df01.sh",
    "du01.sh",
    "file01.sh",
    "gdb01.sh",
    "gzip_tests.sh",
    "insmod01.sh",
    "keyctl01.sh",
    "ld01.sh",
    "ldd01.sh",
    "ln_tests.sh",
    "logrotate_tests.sh",
    "lsmod01.sh",
    "mkdir_tests.sh",
    "mkfs01.sh",
    "mkswap01.sh",
    "mv_tests.sh",
    "nm01.sh",
    "sendfile01.sh",
    "sysctl01.sh",
    "sysctl02.sh",
    "tar_tests.sh",
    "unshare01.sh",
    "unzip01.sh",
    "wc01.sh",
    "which01.sh",
];

// Bind-mount shared-subtree tests (LTP "fs_bind" runtest suite).
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/fs_bind
pub const UNRUN_FS_BIND_TASKS: [&str; 87] = [
    "fs_bind07-2.sh",
    "fs_bind09.sh",
    "fs_bind10.sh",
    "fs_bind11.sh",
    "fs_bind12.sh",
    "fs_bind13.sh",
    "fs_bind14.sh",
    "fs_bind15.sh",
    "fs_bind16.sh",
    "fs_bind17.sh",
    "fs_bind18.sh",
    "fs_bind19.sh",
    "fs_bind20.sh",
    "fs_bind21.sh",
    "fs_bind22.sh",
    "fs_bind23.sh",
    "fs_bind24.sh",
    "fs_bind_cloneNS01.sh",
    "fs_bind_cloneNS02.sh",
    "fs_bind_cloneNS03.sh",
    "fs_bind_cloneNS04.sh",
    "fs_bind_cloneNS05.sh",
    "fs_bind_cloneNS06.sh",
    "fs_bind_cloneNS07.sh",
    "fs_bind_move01.sh",
    "fs_bind_move02.sh",
    "fs_bind_move03.sh",
    "fs_bind_move04.sh",
    "fs_bind_move05.sh",
    "fs_bind_move06.sh",
    "fs_bind_move07.sh",
    "fs_bind_move08.sh",
    "fs_bind_move09.sh",
    "fs_bind_move10.sh",
    "fs_bind_move11.sh",
    "fs_bind_move12.sh",
    "fs_bind_move13.sh",
    "fs_bind_move14.sh",
    "fs_bind_move15.sh",
    "fs_bind_move16.sh",
    "fs_bind_move17.sh",
    "fs_bind_move18.sh",
    "fs_bind_move19.sh",
    "fs_bind_move20.sh",
    "fs_bind_move21.sh",
    "fs_bind_move22.sh",
    "fs_bind_rbind01.sh",
    "fs_bind_rbind02.sh",
    "fs_bind_rbind03.sh",
    "fs_bind_rbind04.sh",
    "fs_bind_rbind05.sh",
    "fs_bind_rbind06.sh",
    "fs_bind_rbind07-2.sh",
    "fs_bind_rbind07.sh",
    "fs_bind_rbind08.sh",
    "fs_bind_rbind09.sh",
    "fs_bind_rbind10.sh",
    "fs_bind_rbind11.sh",
    "fs_bind_rbind12.sh",
    "fs_bind_rbind13.sh",
    "fs_bind_rbind14.sh",
    "fs_bind_rbind15.sh",
    "fs_bind_rbind16.sh",
    "fs_bind_rbind17.sh",
    "fs_bind_rbind18.sh",
    "fs_bind_rbind19.sh",
    "fs_bind_rbind20.sh",
    "fs_bind_rbind21.sh",
    "fs_bind_rbind22.sh",
    "fs_bind_rbind23.sh",
    "fs_bind_rbind24.sh",
    "fs_bind_rbind25.sh",
    "fs_bind_rbind26.sh",
    "fs_bind_rbind27.sh",
    "fs_bind_rbind28.sh",
    "fs_bind_rbind29.sh",
    "fs_bind_rbind30.sh",
    "fs_bind_rbind31.sh",
    "fs_bind_rbind32.sh",
    "fs_bind_rbind33.sh",
    "fs_bind_rbind34.sh",
    "fs_bind_rbind35.sh",
    "fs_bind_rbind36.sh",
    "fs_bind_rbind37.sh",
    "fs_bind_rbind38.sh",
    "fs_bind_rbind39.sh",
    "fs_bind_regression.sh",
];

// Filesystem race/stress tests.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/fs
pub const UNRUN_FS_RACER_TASKS: [&str; 10] = [
    "fs_racer.sh",
    "fs_racer_dir_create.sh",
    "fs_racer_dir_test.sh",
    "fs_racer_file_concat.sh",
    "fs_racer_file_create.sh",
    "fs_racer_file_link.sh",
    "fs_racer_file_list.sh",
    "fs_racer_file_rename.sh",
    "fs_racer_file_rm.sh",
    "fs_racer_file_symlink.sh",
];

// fsx filesystem exerciser.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/fs
pub const UNRUN_FSX_TASKS: [&str; 1] = ["fsx.sh"];

// Advanced filesystem tests (iso9660, quota).
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/fs
pub const UNRUN_FS_ADVANCED_TASKS: [&str; 2] = ["isofs.sh", "quota_remount_test01.sh"];

// cgroup regression shell tests.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/controllers
pub const UNRUN_CGROUP_REGRESSION_TASKS: [&str; 6] = [
    "cgroup_regression_3_1.sh",
    "cgroup_regression_3_2.sh",
    "cgroup_regression_5_1.sh",
    "cgroup_regression_5_2.sh",
    "cgroup_regression_6_1.sh",
    "cgroup_regression_6_2.sh",
];

// CPU hotplug tests.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/cpuhotplug
pub const UNRUN_CPUHOTPLUG_TASKS: [&str; 9] = [
    "cpuhotplug01.sh",
    "cpuhotplug02.sh",
    "cpuhotplug03.sh",
    "cpuhotplug04.sh",
    "cpuhotplug05.sh",
    "cpuhotplug06.sh",
    "cpuhotplug07.sh",
    "cpuhotplug_hotplug.sh",
    "cpuhotplug_testsuite.sh",
];

// Power-management shell tests.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/power_management_tests
pub const UNRUN_PM_SHELL_TASKS: [&str; 15] = [
    "runpwtests01.sh",
    "runpwtests02.sh",
    "runpwtests03.sh",
    "runpwtests04.sh",
    "runpwtests05.sh",
    "runpwtests06.sh",
    "runpwtests_exclusive01.sh",
    "runpwtests_exclusive02.sh",
    "runpwtests_exclusive03.sh",
    "runpwtests_exclusive04.sh",
    "runpwtests_exclusive05.sh",
    "pm_cpu_consolidation.py",
    "pm_ilb_test.py",
    "pm_sched_domain.py",
    "pm_sched_mc.py",
];

// IMA (Integrity Measurement Architecture) tests.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/ima
pub const UNRUN_IMA_TASKS: [&str; 9] = [
    "ima_conditionals.sh",
    "ima_kexec.sh",
    "ima_keys.sh",
    "ima_measurements.sh",
    "ima_policy.sh",
    "ima_selinux.sh",
    "ima_setup.sh",
    "ima_tpm.sh",
    "ima_violations.sh",
];

// Smack LSM shell tests.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/smack
pub const UNRUN_SMACK_SHELL_TASKS: [&str; 1] = ["smack_file_access.sh"];

// TPM (Trusted Platform Module) tests.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/tpm_tools
pub const UNRUN_TPM_TASKS: [&str; 40] = [
    "tpm_changeauth_tests.sh",
    "tpm_changeauth_tests_exp01.sh",
    "tpm_changeauth_tests_exp02.sh",
    "tpm_changeauth_tests_exp03.sh",
    "tpm_clear_tests.sh",
    "tpm_clear_tests_exp01.sh",
    "tpm_getpubek_tests.sh",
    "tpm_getpubek_tests_exp01.sh",
    "tpm_restrictpubek_tests.sh",
    "tpm_restrictpubek_tests_exp01.sh",
    "tpm_restrictpubek_tests_exp02.sh",
    "tpm_restrictpubek_tests_exp03.sh",
    "tpm_selftest_tests.sh",
    "tpm_takeownership_tests.sh",
    "tpm_takeownership_tests_exp01.sh",
    "tpm_version_tests.sh",
    "tpmtoken_import_tests.sh",
    "tpmtoken_import_tests_exp01.sh",
    "tpmtoken_import_tests_exp02.sh",
    "tpmtoken_import_tests_exp03.sh",
    "tpmtoken_import_tests_exp04.sh",
    "tpmtoken_import_tests_exp05.sh",
    "tpmtoken_import_tests_exp06.sh",
    "tpmtoken_import_tests_exp07.sh",
    "tpmtoken_import_tests_exp08.sh",
    "tpmtoken_init_tests.sh",
    "tpmtoken_init_tests_exp00.sh",
    "tpmtoken_init_tests_exp01.sh",
    "tpmtoken_init_tests_exp02.sh",
    "tpmtoken_init_tests_exp03.sh",
    "tpmtoken_objects_tests.sh",
    "tpmtoken_objects_tests_exp01.sh",
    "tpmtoken_protect_tests.sh",
    "tpmtoken_protect_tests_exp01.sh",
    "tpmtoken_protect_tests_exp02.sh",
    "tpmtoken_setpasswd_tests.sh",
    "tpmtoken_setpasswd_tests_exp01.sh",
    "tpmtoken_setpasswd_tests_exp02.sh",
    "tpmtoken_setpasswd_tests_exp03.sh",
    "tpmtoken_setpasswd_tests_exp04.sh",
];

// Kernel tracing regression tests.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/tracing
pub const UNRUN_TRACING_TASKS: [&str; 3] = [
    "ftrace_regression01.sh",
    "ftrace_regression02.sh",
    "ftrace_stress_test.sh",
];

// Kernel misc: binfmt_misc and dynamic_debug.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/kernel_misc
pub const UNRUN_MISC_KERNEL_SHELL_TASKS: [&str; 3] =
    ["binfmt_misc01.sh", "binfmt_misc02.sh", "dynamic_debug01.sh"];

// Network command-line tool tests (ping, ifconfig, routing, iptables, tc, etc.).
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/net.tcp_cmds and net.ipv6
pub const UNRUN_NET_CMDS_TASKS: [&str; 29] = [
    "arping01.sh",
    "icmp-uni-basic.sh",
    "icmp-uni-vti.sh",
    "if-addr-adddel.sh",
    "if-addr-addlarge.sh",
    "if-mtu-change.sh",
    "if-route-adddel.sh",
    "if-route-addlarge.sh",
    "if-updown.sh",
    "if4-addr-change.sh",
    "ip_tests.sh",
    "ipneigh01.sh",
    "iptables01.sh",
    "netns_breakns.sh",
    "netns_comm.sh",
    "netns_sysfs.sh",
    "netstat01.sh",
    "ping01.sh",
    "ping02.sh",
    "route-change-dst.sh",
    "route-change-gw.sh",
    "route-change-if.sh",
    "route-change-netlink-dst.sh",
    "route-change-netlink-gw.sh",
    "route-change-netlink-if.sh",
    "route-redirect.sh",
    "tc01.sh",
    "tcp_fastopen_run.sh",
    "tcpdump01.sh",
];

// Multicast network tests.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/net.multicast
pub const UNRUN_NET_MULTICAST_TASKS: [&str; 12] = [
    "mcast-group-multiple-socket.sh",
    "mcast-group-same-group.sh",
    "mcast-group-single-socket.sh",
    "mcast-group-source-filter.sh",
    "mcast-pktfld01.sh",
    "mcast-pktfld02.sh",
    "mcast-queryfld01.sh",
    "mcast-queryfld02.sh",
    "mcast-queryfld03.sh",
    "mcast-queryfld04.sh",
    "mcast-queryfld05.sh",
    "mcast-queryfld06.sh",
];

// NFS tests.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/net.nfs
pub const UNRUN_NET_NFS_TASKS: [&str; 11] = [
    "nfs01.sh",
    "nfs02.sh",
    "nfs03.sh",
    "nfs04.sh",
    "nfs05.sh",
    "nfs06.sh",
    "nfs07.sh",
    "nfs08.sh",
    "nfs09.sh",
    "nfslock01.sh",
    "nfsstat01.sh",
];

// TCP congestion-control and DCCP tests.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/net.tcp_cmds
pub const UNRUN_NET_TCP_CC_TASKS: [&str; 3] = ["bbr01.sh", "bbr02.sh", "dctcp01.sh"];

// SCTP/DCCP protocol tests.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/net.sctp
pub const UNRUN_NET_SCTP_DCCP_TASKS: [&str; 2] = ["dccp01.sh", "sctp01.sh"];

// Network tunnelling tests (GRE, FOU, GENEVE, SIT).
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/net.ipv6
pub const UNRUN_NET_TUNNEL_TASKS: [&str; 6] = [
    "fou01.sh",
    "geneve01.sh",
    "geneve02.sh",
    "gre01.sh",
    "gre02.sh",
    "sit01.sh",
];

// MACsec / MPLS / VLAN / VXLAN network tests.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/net.ipv6
pub const UNRUN_NET_OVERLAY_TASKS: [&str; 14] = [
    "macsec01.sh",
    "macsec02.sh",
    "macsec03.sh",
    "mpls01.sh",
    "mpls02.sh",
    "mpls03.sh",
    "mpls04.sh",
    "vlan01.sh",
    "vlan02.sh",
    "vlan03.sh",
    "vxlan01.sh",
    "vxlan02.sh",
    "vxlan03.sh",
    "vxlan04.sh",
];

// Virtual NIC / WireGuard / busy-poll network tests.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/net.ipv6
pub const UNRUN_NET_VIRT_TASKS: [&str; 8] = [
    "ipvlan01.sh",
    "macvlan01.sh",
    "macvtap01.sh",
    "wireguard01.sh",
    "wireguard02.sh",
    "busy_poll01.sh",
    "busy_poll02.sh",
    "busy_poll03.sh",
];

// Broken-IP packet injection tests.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/net_stress.broken_ip
pub const UNRUN_NET_BROKEN_IP_TASKS: [&str; 8] = [
    "broken_ip-checksum.sh",
    "broken_ip-dstaddr.sh",
    "broken_ip-fragment.sh",
    "broken_ip-ihl.sh",
    "broken_ip-nexthdr.sh",
    "broken_ip-plen.sh",
    "broken_ip-protcol.sh",
    "broken_ip-version.sh",
];

// NUMA topology tests.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/numa
pub const UNRUN_NUMA_SHELL_TASKS: [&str; 1] = ["numa01.sh"];

// zram device-driver tests (additional .sh entries alongside binary zram03).
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/kernel_misc
pub const UNRUN_ZRAM_TASKS: [&str; 2] = ["zram01.sh", "zram02.sh"];

// Kernel locking and RCU torture tests.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/kernel_misc
pub const UNRUN_LOCK_RCU_TASKS: [&str; 2] = ["lock_torture.sh", "rcu_torture.sh"];

// VMA / virtual-memory-area test (shell wrapper).
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/mm
pub const UNRUN_VMA_SHELL_TASKS: [&str; 1] = ["vma05.sh"];

// Filesystem link stress test.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/fs
pub const UNRUN_FS_LINK_TASKS: [&str; 1] = ["linktest.sh"];

// NFT (nftables) network test.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/net.tcp_cmds
pub const UNRUN_NFT_TASKS: [&str; 1] = ["nft01.sh"];

// Network traceroute / tracepath tests.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/net.tcp_cmds
pub const UNRUN_TRACEROUTE_TASKS: [&str; 2] = ["tracepath01.sh", "traceroute01.sh"];

// Sound subsystem CVE regression tests.
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/cve
pub const UNRUN_SOUND_CVE_TASKS: [&str; 2] = ["snd_seq01", "snd_timer01"];

// cgroup controller functional tests (shell).
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/controllers
pub const UNRUN_CGROUP_CTRL_SHELL_TASKS: [&str; 1] = ["test_controllers.sh"];

// Read-only filesystem bind tests (fs_readonly runtest suite).
// Runtest source: testsuits-for-oskernel/ltp-full-20240524/runtest/fs_readonly
pub const UNRUN_FS_READONLY_TASKS: [&str; 1] = ["test_robind.sh"];

// Total UNRUN entries: 649 (309 original + 340 new additions)
