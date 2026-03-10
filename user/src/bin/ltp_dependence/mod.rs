mod submit_plan;
pub use submit_plan::{run_non_riscv_ltp_groups, run_riscv_ltp_groups};

// Basic LTP test cases
pub const LTP_TEST_POINTS: [&str; 10] = [
    "abort01", "abs01", "accept01", "access01", "access02", "access03", "access04", "acct01",
    "acct02", "write01",
];

// FORK test cases
// aligned with ltp_all.md
pub const FORK_TASKS: [&str; 10] = [
    "fork01", "fork03", "fork04", "fork05", "fork07", "fork08", "fork09", "fork10", "fork13",
    "fork14",
];

// WAIT_PID test cases
// aligned with ltp_all.md
pub const WAITPID_TASKS: [&str; 11] = [
    "waitpid01",
    "waitpid03",
    "waitpid04",
    "waitpid06",
    "waitpid07",
    "waitpid08",
    "waitpid09",
    "waitpid10",
    "waitpid11",
    "waitpid12",
    "waitpid13",
];

// WAITID test cases
// aligned with ltp_all.md
pub const WAITID_TASKS: [&str; 11] = [
    "waitid01", "waitid02", "waitid03", "waitid04", "waitid05", "waitid06", "waitid07", "waitid08",
    "waitid09", "waitid10", "waitid11",
];

// PROC identity/info test cases
// aligned with ltp_all.md
pub const PROCINFO_TASKS: [&str; 19] = [
    "getpid01",
    "getpid02",
    "getppid01",
    "getppid02",
    "getuid01",
    "getuid03",
    "geteuid01",
    "geteuid02",
    "getgid01",
    "getgid03",
    "getegid01",
    "getegid02",
    "getpgid01",
    "getpgid02",
    "getsid01",
    "getsid02",
    "uname01",
    "uname02",
    "gettimeofday01",
];

// Process group/session management test cases.
// Chosen as next step after getpgid/getsid pass.
pub const PGRP_SESSION_TASKS: [&str; 4] = ["setpgid01", "setpgid02", "setpgid03", "setsid01"];

// Legacy setpgrp wrappers (setpgid(0,0) behavior).
pub const SETPGRP_TASKS: [&str; 2] = ["setpgrp01", "setpgrp02"];

// Nice/priority controls.
pub const SETPRIORITY_TASKS: [&str; 2] = ["setpriority01", "setpriority02"];
pub const GETPRIORITY_TASKS: [&str; 2] = ["getpriority01", "getpriority02"];
pub const PROC_TID_TASKS: [&str; 3] = ["getpgrp01", "gettid01", "gettid02"];

// Linux-like scheduler/nice focus batch (Section 3 + nice core).
pub const NEXT_TARGET_SCHED_NICE_CORE_TASKS: [&str; 31] = [
    "nice01",
    "nice02",
    "nice03",
    "nice04",
    "nice05",
    "sched_get_priority_max01",
    "sched_get_priority_max02",
    "sched_get_priority_min01",
    "sched_get_priority_min02",
    "sched_getaffinity01",
    "sched_getattr01",
    "sched_getattr02",
    "sched_getparam01",
    "sched_getparam03",
    "sched_getscheduler01",
    "sched_getscheduler02",
    "sched_rr_get_interval01",
    "sched_rr_get_interval02",
    "sched_rr_get_interval03",
    "sched_setaffinity01",
    "sched_setattr01",
    "sched_setparam01",
    "sched_setparam02",
    "sched_setparam03",
    "sched_setparam04",
    "sched_setparam05",
    "sched_setscheduler01",
    "sched_setscheduler02",
    "sched_setscheduler03",
    "sched_setscheduler04",
    "sched_yield01",
];

// Credential query test cases.
// Skip *_16 variants for now.
pub const GETRES_TASKS: [&str; 6] = [
    "getresuid01",
    "getresuid02",
    "getresuid03",
    "getresgid01",
    "getresgid02",
    "getresgid03",
];

// Credential mutation tests (non-*_16 variants).
pub const CRED_SET_CORE_TASKS: [&str; 17] = [
    "setuid01",
    "setuid03",
    "setuid04",
    "setgid01",
    "setgid02",
    "setgid03",
    "setreuid01",
    "setreuid02",
    "setreuid03",
    "setreuid04",
    "setreuid05",
    "setreuid06",
    "setreuid07",
    "setregid01",
    "setregid02",
    "setregid03",
    "setregid04",
];

pub const CRED_SET_RES_TASKS: [&str; 9] = [
    "setresuid01",
    "setresuid02",
    "setresuid03",
    "setresuid04",
    "setresuid05",
    "setresgid01",
    "setresgid02",
    "setresgid03",
    "setresgid04",
];

// Credential mutation tests (non-*_16 variants).
pub const CRED_SET_TASKS: [&str; 26] = [
    "setuid01",
    "setuid03",
    "setuid04",
    "setgid01",
    "setgid02",
    "setgid03",
    "setreuid01",
    "setreuid02",
    "setreuid03",
    "setreuid04",
    "setreuid05",
    "setreuid06",
    "setreuid07",
    "setregid01",
    "setregid02",
    "setregid03",
    "setregid04",
    "setresuid01",
    "setresuid02",
    "setresuid03",
    "setresuid04",
    "setresuid05",
    "setresgid01",
    "setresgid02",
    "setresgid03",
    "setresgid04",
];

// Filesystem credential setters.
pub const CRED_FS_TASKS: [&str; 6] = [
    "setfsuid01",
    "setfsuid02",
    "setfsuid03",
    "setfsuid04",
    "setfsgid01",
    "setfsgid02",
];

// Effective gid focused tests.
pub const CRED_EGID_TASKS: [&str; 2] = ["setegid01", "setegid02"];

// UTS nodename/domainname setters.
pub const UTS_NAME_TASKS: [&str; 6] = [
    "sethostname01",
    "sethostname02",
    "sethostname03",
    "setdomainname01",
    "setdomainname02",
    "setdomainname03",
];
pub const UTS_QUERY_TASKS: [&str; 2] = ["gethostname01", "getdomainname01"];
// Special case: gethostname02 expects glibc behavior (ENAMETOOLONG on
// truncation). The shipped musl wrapper truncates and returns success.
// Run it explicitly in glibc-only lanes from submit_plan.
pub const GETRANDOM_TASKS: [&str; 5] = [
    "getrandom01",
    "getrandom02",
    "getrandom03",
    "getrandom04",
    "getrandom05",
];
pub const GETITIMER_TASKS: [&str; 2] = ["getitimer01", "getitimer02"];
pub const SETITIMER_TASKS: [&str; 2] = ["setitimer01", "setitimer02"];
pub const GETRUSAGE_TASKS: [&str; 2] = ["getrusage01", "getrusage02"];
pub const CWD_DIR_TASKS: [&str; 6] = [
    "getcwd01", "getcwd02", "getcwd03", "getcwd04", "chdir01", "chdir04",
];
pub const ACCESS_TASKS: [&str; 4] = ["access01", "access02", "access03", "access04"];
pub const FACCESSAT_TASKS: [&str; 4] =
    ["faccessat01", "faccessat02", "faccessat201", "faccessat202"];
pub const CLOSE_TASKS: [&str; 2] = ["close01", "close02"];
pub const OPEN_CORE_TASKS: [&str; 6] = ["open01", "open02", "open03", "open04", "open06", "open07"];
pub const OPEN_EXT_TASKS: [&str; 5] = ["open08", "open09", "open10", "open11", "open13"];
// open12 probes large sparse-file behavior (4G+ hole write). Keep isolated until
// sparse-file accounting is fully Linux-like and does not pollute later tests.
// open14 validates true O_TMPFILE unlink-then-link semantics; keep it isolated
// until the VFS supports anonymous tmp inode lifetime correctly.
// openat02 stresses large sparse-file behavior (4G+ seek/write). Keep it separate
// from the default openat lane until sparse-hole accounting is fully aligned.
pub const OPENAT_CORE_TASKS: [&str; 2] = ["openat01", "openat03"];
// openat04 requires LTP mount-device allocation (`.all_filesystems = 1`) and
// currently TBROKs in this environment before validating syscall semantics.
// close_range01 requires mount-device allocation and all-filesystem coverage;
// keep close_range02 as the default portable subset.
pub const CLOSE_RANGE_CORE_TASKS: [&str; 1] = ["close_range02"];
pub const UMASK_TASKS: [&str; 1] = ["umask01"];

// Resource limit setters.
pub const SETRLIMIT_TASKS: [&str; 6] = [
    "setrlimit01",
    "setrlimit02",
    "setrlimit03",
    "setrlimit04",
    "setrlimit05",
    "setrlimit06",
];
pub const GETRLIMIT_TASKS: [&str; 3] = ["getrlimit01", "getrlimit02", "getrlimit03"];

// Thread robust-list and TID setup helpers.
pub const ROBUST_TID_TASKS: [&str; 3] = [
    "set_robust_list01",
    "get_robust_list01",
    "set_tid_address01",
];

// Clock-related tests grouped by syscall family.
pub const CLOCK_GETTIME_TASKS: [&str; 4] = [
    "clock_gettime01",
    "clock_gettime02",
    "clock_gettime03",
    "clock_gettime04",
];
pub const CLOCK_SETTIME_TASKS: [&str; 3] =
    ["clock_settime01", "clock_settime02", "clock_settime03"];
pub const CLOCK_RES_TASKS: [&str; 1] = ["clock_getres01"];
pub const CLOCK_NANOSLEEP_TASKS: [&str; 4] = [
    "clock_nanosleep01",
    "clock_nanosleep02",
    "clock_nanosleep03",
    "clock_nanosleep04",
];

// Time-related non-clock tests.
pub const TIME_MISC_TASKS: [&str; 5] = [
    "gettimeofday02",
    "times01",
    "nanosleep01",
    "nanosleep02",
    "nanosleep04",
];

// Alarm syscall test cases
// aligned with ltp_all.md
pub const ALARM_TASKS: [&str; 5] = ["alarm02", "alarm03", "alarm05", "alarm06", "alarm07"];

// Ownership change tests (skip legacy *_16 compatibility variants).
pub const CHOWN_TASKS: [&str; 5] = ["chown01", "chown02", "chown03", "chown04", "chown05"];
pub const CHMOD_TASKS: [&str; 5] = ["chmod01", "chmod03", "chmod05", "chmod06", "chmod07"];
pub const FCHMOD_TASKS: [&str; 8] = [
    "fchmod01",
    "fchmod02",
    "fchmod03",
    "fchmod04",
    "fchmod05",
    "fchmod06",
    "fchmodat01",
    "fchmodat02",
];
pub const FCHOWN_TASKS: [&str; 7] = [
    "fchown01",
    "fchown02",
    "fchown03",
    "fchown04",
    "fchown05",
    "fchownat01",
    "fchownat02",
];
pub const FCHDIR_TASKS: [&str; 3] = ["fchdir01", "fchdir02", "fchdir03"];
// creat07 (ETXTBSY) and creat09 (setgid-inherit/CVE, mount-device heavy) are
// still tracked as special cases and should be run explicitly when needed.
pub const CREAT_CORE_TASKS: [&str; 6] = [
    "creat01", "creat03", "creat04", "creat05", "creat06", "creat08",
];

// File descriptor duplication tests.
pub const DUP_CORE_TASKS: [&str; 9] = [
    "dup01", "dup02", "dup03", "dup04", "dup05", "dup06", "dup07", "dup3_01", "dup3_02",
];
pub const DUP_FCNTL_TASKS: [&str; 7] = [
    "dup201", "dup202", "dup203", "dup204", "dup205", "dup206", "dup207",
];

// Stat-family descriptor tests.
pub const FSTAT_TASKS: [&str; 3] = ["fstat02", "fstat03", "fstatat01"];
// fstatfs01 requires free mount devices from LTP harness and currently breaks
// in this environment with TBROK before reaching syscall checks.
pub const FSTATFS_TASKS: [&str; 1] = ["fstatfs02"];
// statfs01 depends on mount-device allocation from LTP harness.
pub const STATFS_TASKS: [&str; 2] = ["statfs02", "statfs03"];
pub const STATX_BASIC_TASKS: [&str; 3] = ["statx01", "statx02", "statx03"];
pub const STAT_TASKS: [&str; 3] = ["stat01", "stat02", "stat03"];
// mknodat02 depends on LTP mount-device allocation (`tst_acquire_device`) and
// currently TBROKs in this environment before reaching syscall checks.
pub const MKNODAT_TASKS: [&str; 1] = ["mknodat01"];
// mknod07 mounts a temporary read-only filesystem and needs LTP free device
// allocation in this environment, so keep it separate from default runs.
pub const MKNOD_CORE_TASKS: [&str; 8] = [
    "mknod01", "mknod02", "mknod03", "mknod04", "mknod05", "mknod06", "mknod08", "mknod09",
];
pub const MKDIR_CORE_TASKS: [&str; 7] = [
    "mkdir02",
    "mkdir03",
    "mkdir04",
    "mkdir05",
    "mkdir09",
    "mkdirat01",
    "mkdirat02",
];
pub const RMDIR_TASKS: [&str; 3] = ["rmdir01", "rmdir02", "rmdir03"];
pub const LINK_CORE_TASKS: [&str; 6] = [
    "link02", "link04", "link05", "link08", "linkat01", "linkat02",
];
// symlink01 is a legacy compound scenario touching many non-symlink paths.
// Keep the note here, but run it in the same symlink subgroup.
pub const SYMLINK_CORE_TASKS: [&str; 5] = [
    "symlink01",
    "symlink02",
    "symlink03",
    "symlink04",
    "symlinkat01",
];
pub const READLINK_CORE_TASKS: [&str; 4] =
    ["readlink01", "readlink03", "readlinkat01", "readlinkat02"];

// POSIX timer tests grouped together.
pub const POSIX_TIMER_TASKS: [&str; 7] = [
    "timer_settime01",
    "timer_settime02",
    "timer_settime03",
    "timer_gettime01",
    "timer_delete01",
    "timer_delete02",
    "timer_getoverrun01",
];

// One-shot 100-case batch for the next LTP push, grouped by subsystem.
pub const NEXT100_PROCESS_WAIT_TASKS: [&str; 12] = [
    "clone01",
    "clone02",
    "clone03",
    "clone04",
    "clone301",
    "clone302",
    "wait01",
    "wait02",
    "wait401",
    "wait402",
    "exit01",
    "exit_group01",
];

pub const NEXT100_EXEC_TASKS: [&str; 12] = [
    "vfork",
    "vfork01",
    "vfork02",
    "execl01",
    "execle01",
    "execv01",
    "execve01",
    "execve02",
    "execve03",
    "execve04",
    "execveat01",
    "execveat02",
];

pub const NEXT100_CRED_TASKS: [&str; 12] = [
    "acct02",
    "add_key01",
    "add_key02",
    "add_key03",
    "add_key04",
    "add_key05",
    "acl1",
    "capget01",
    "capget02",
    "capset01",
    "capset02",
    "check_keepcaps",
];

pub const NEXT100_FILE_TASKS: [&str; 12] = [
    "copy_file_range01",
    "copy_file_range02",
    "copy_file_range03",
    "ftruncate01",
    "ftruncate01_64",
    "ftruncate03",
    "ftruncate03_64",
    "ftruncate04",
    "ftruncate04_64",
    "truncate02",
    "truncate02_64",
    "truncate03",
];

pub const NEXT100_VM_MAP_TASKS: [&str; 12] = [
    "brk01",
    "sbrk01",
    "sbrk02",
    "mmap01",
    "mmap02",
    "mmap03",
    "mmap04",
    "mmap05",
    "mmap06",
    "mmap09",
    "mprotect01",
    "mprotect02",
];

pub const NEXT100_VM_LOCK_TASKS: [&str; 12] = [
    "mlock01",
    "mlock02",
    "mlock03",
    "mlockall01",
    "mlockall02",
    "munlock01",
    "munlock02",
    "munlockall01",
    "madvise01",
    "madvise02",
    "process_madvise01",
    "mincore01",
];

pub const NEXT100_IPC_TASKS: [&str; 12] = [
    "msgctl01", "msgctl02", "msgget01", "msgget02", "msgrcv01", "msgsnd01", "semctl01", "semctl02",
    "semop01", "semop02", "semget01", "shmat01",
];

pub const NEXT100_TIME_SIGNAL_TASKS: [&str; 12] = [
    "clock_adjtime01",
    "clock_settime01",
    "clock_settime02",
    "timerfd01",
    "timerfd02",
    "timerfd_create01",
    "timerfd_gettime01",
    "timerfd_settime01",
    "timerfd_settime02",
    "alarm02",
    "alarm03",
    "signalfd01",
];

pub const NEXT100_MISC_TASKS: [&str; 4] = ["utime01", "utimensat01", "uname04", "arch_prctl01"];

// Next 100-target campaign (5 x 20) for network core + xattr core.
// Batch order is arranged for incremental bring-up:
// 1) socket lifecycle/connectivity, 2) send/recv + query basics,
// 3) sockopt + select/poll, 4) epoll core paths, 5) xattr + network wrap-up.
pub const NEXT_TARGET_BATCH1_SOCKET_CONN_TASKS: [&str; 20] = [
    "socket01",
    "socket02",
    "socketcall01",
    "socketcall02",
    "socketcall03",
    "socketpair01",
    "socketpair02",
    "sockioctl01",
    "accept02",
    "accept03",
    "accept4_01",
    "bind01",
    "bind02",
    "bind03",
    "bind04",
    "bind05",
    "bind06",
    "connect01",
    "connect02",
    "listen01",
];

pub const NEXT_TARGET_BATCH2_SEND_RECV_QUERY_TASKS: [&str; 20] = [
    "recv01",
    "recvfrom01",
    "recvmmsg01",
    "recvmsg01",
    "recvmsg02",
    "recvmsg03",
    "send01",
    "send02",
    "sendmmsg01",
    "sendmmsg02",
    "sendmsg01",
    "sendmsg02",
    "sendmsg03",
    "sendto01",
    "sendto02",
    "sendto03",
    "getpeername01",
    "getsockname01",
    "getsockopt01",
    "getsockopt02",
];

pub const NEXT_TARGET_BATCH3_SOCKOPT_MUX_TASKS: [&str; 20] = [
    "setsockopt01",
    "setsockopt02",
    "setsockopt03",
    "setsockopt04",
    "setsockopt05",
    "setsockopt06",
    "setsockopt07",
    "setsockopt08",
    "setsockopt09",
    "setsockopt10",
    "select01",
    "select02",
    "select03",
    "select04",
    "poll01",
    "poll02",
    "ppoll01",
    "pselect01",
    "pselect01_64",
    "pselect02",
];

pub const NEXT_TARGET_BATCH4_EPOLL_CORE_TASKS: [&str; 20] = [
    "epoll_create01",
    "epoll_create02",
    "epoll_create1_01",
    "epoll_create1_02",
    "epoll_ctl01",
    "epoll_ctl02",
    "epoll_ctl03",
    "epoll_ctl04",
    "epoll_ctl05",
    "epoll_wait01",
    "epoll_wait02",
    "epoll_wait03",
    "epoll_wait04",
    "epoll_wait05",
    "epoll_wait06",
    "epoll_wait07",
    "epoll_pwait01",
    "epoll_pwait02",
    "epoll_pwait03",
    "epoll_pwait04",
];

pub const NEXT_TARGET_BATCH5_XATTR_WRAPUP_TASKS: [&str; 20] = [
    "getxattr01",
    "getxattr02",
    "getxattr03",
    "getxattr04",
    "getxattr05",
    "setxattr01",
    "setxattr02",
    "setxattr03",
    "listxattr01",
    "listxattr02",
    "listxattr03",
    "removexattr01",
    "removexattr02",
    "lgetxattr01",
    "lgetxattr02",
    "pselect02_64",
    "pselect03",
    "pselect03_64",
    "epoll_pwait05",
    "epoll-ltp",
];

// Next focused 100-target campaign (2026-02-24) from uncovered tests.
// 5 batches x 20, prioritized by ltp_test_summary Sections 1/2/4/5/8/7.
pub const NEXT_TARGET_FOCUS_BATCH1_PROCESS_CRED_TASKS: [&str; 20] = [
    "clone05",
    "clone06",
    "clone07",
    "clone08",
    "clone09",
    "clone303",
    "execlp01",
    "execvp01",
    "execve05",
    "execve06",
    "execveat03",
    "wait403",
    "exit02",
    "chroot01",
    "chroot02",
    "chroot03",
    "chroot04",
    "getgroups01",
    "setgroups01",
    "setgroups02",
];

pub const NEXT_TARGET_FOCUS_BATCH2_SIGNAL_TASKS: [&str; 20] = [
    "sigaction01",
    "sigaction02",
    "sigaltstack01",
    "sigaltstack02",
    "signal01",
    "signal02",
    "signal03",
    "signal04",
    "signal05",
    "signal06",
    "sigpending02",
    "sigprocmask01",
    "sigrelse01",
    "sigsuspend01",
    "sigtimedwait01",
    "sigwait01",
    "sigwaitinfo01",
    "rt_sigaction01",
    "rt_sigaction02",
    "rt_sigaction03",
];

pub const NEXT_TARGET_FOCUS_BATCH3_FS_IO_TASKS: [&str; 20] = [
    "read01", "read02", "read03", "read04", "readv01", "readv02", "write02", "write03", "write04",
    "write05", "write06", "writev01", "writev02", "writev03", "writev05", "writev06", "writev07",
    "lseek01", "lseek02", "lseek07",
];

pub const NEXT_TARGET_FOCUS_BATCH4_SYSV_IPC_TASKS: [&str; 20] = [
    "msgctl03", "msgctl04", "msgctl05", "msgctl06", "msgget03", "msgget04", "msgrcv02", "msgrcv03",
    "msgsnd02", "msgsnd05", "semctl03", "semctl04", "semctl05", "semctl06", "semctl07", "semctl08",
    "semop03", "semop04", "semop05", "semget05",
];

pub const NEXT_TARGET_FOCUS_BATCH5_MEMORY_TASKS: [&str; 20] = [
    "brk02",
    "sbrk03",
    "mprotect03",
    "mprotect04",
    "mprotect05",
    "munmap01",
    "munmap02",
    "munmap03",
    "mremap01",
    "mremap02",
    "mremap03",
    "mremap04",
    "mremap05",
    "mremap06",
    "msync01",
    "msync02",
    "msync03",
    "msync04",
    "mincore02",
    "mincore03",
];

pub const NEXT_TARGET_FOCUS_BATCH6_SPECIAL_FS_MM_TASKS: [&str; 3] =
    ["creat07", "creat09", "userfaultfd01"];

// New 100-test target set (2026-02-24) selected from uncovered tests.
// Ordered for "one batch at a time" progress: signal -> sched -> time -> ipc -> fs io.
pub const NEXT_TARGET_NEXT100_SIGNAL_TASKS: [&str; 26] = [
    "kill02",
    "kill03",
    "kill05",
    "kill06",
    "kill07",
    "kill08",
    "kill09",
    "kill10",
    "kill11",
    "kill12",
    "kill13",
    "pause01",
    "pause02",
    "pause03",
    "rt_sigprocmask01",
    "rt_sigprocmask02",
    "rt_sigqueueinfo01",
    "rt_sigsuspend01",
    "sighold02",
    "signalfd4_01",
    "signalfd4_02",
    "tgkill01",
    "tgkill02",
    "tgkill03",
    "tkill01",
    "tkill02",
];

pub const NEXT_TARGET_NEXT100_SCHED_TASKS: [&str; 7] = [
    "sched_tc0",
    "sched_tc1",
    "sched_tc2",
    "sched_tc3",
    "sched_tc4",
    "sched_tc5",
    "sched_tc6",
];

pub const NEXT_TARGET_NEXT100_TIME_TASKS: [&str; 19] = [
    "adjtimex01",
    "adjtimex02",
    "adjtimex03",
    "clock_adjtime02",
    "futimesat01",
    "settimeofday01",
    "settimeofday02",
    "stime01",
    "stime02",
    "time01",
    "times03",
    "utime02",
    "utime03",
    "utime04",
    "utime05",
    "utime06",
    "utime07",
    "utimes01",
    "leapsec01",
];

pub const NEXT_TARGET_NEXT100_IPC_TASKS: [&str; 25] = [
    "eventfd01",
    "eventfd02",
    "eventfd03",
    "eventfd04",
    "eventfd05",
    "eventfd06",
    "eventfd2_01",
    "eventfd2_02",
    "eventfd2_03",
    "futex_wait01",
    "futex_wait02",
    "futex_wait03",
    "futex_wait04",
    "futex_wait05",
    "futex_wait_bitset01",
    "futex_wake01",
    "futex_wake02",
    "futex_wake03",
    "futex_wake04",
    "futex_cmp_requeue01",
    "futex_cmp_requeue02",
    "futex_waitv01",
    "futex_waitv02",
    "futex_waitv03",
    "timerfd04",
];

pub const NEXT_TARGET_NEXT100_FS_IO_TASKS: [&str; 23] = [
    "close_range01",
    "pread01",
    "pread02",
    "pread01_64",
    "pread02_64",
    "pwrite01",
    "pwrite02",
    "pwrite03",
    "pwrite04",
    "pwrite01_64",
    "pwrite02_64",
    "pwrite03_64",
    "pwrite04_64",
    "preadv01",
    "preadv02",
    "preadv03",
    "preadv01_64",
    "preadv02_64",
    "preadv03_64",
    "pwritev01",
    "pwritev02",
    "pwritev01_64",
    "pwritev02_64",
];

pub const NEXT_TARGET_NEXT100_FS_IO_V2_TASKS: [&str; 8] = [
    "preadv201",
    "preadv202",
    "preadv201_64",
    "preadv202_64",
    "pwritev201",
    "pwritev202",
    "pwritev201_64",
    "pwritev202_64",
];

// Next 100-test target set (2026-02-25), selected via ltp summary priority
// Section 5 (filesystem basic syscalls): fcntl/llseek/getdents/rename/unlink.
pub const NEXT_TARGET_FS100_FCNTL_BATCH1_TASKS: [&str; 20] = [
    "fcntl01",
    "fcntl01_64",
    "fcntl02",
    "fcntl02_64",
    "fcntl03",
    "fcntl03_64",
    "fcntl04",
    "fcntl04_64",
    "fcntl05",
    "fcntl05_64",
    "fcntl07",
    "fcntl07_64",
    "fcntl08",
    "fcntl08_64",
    "fcntl09",
    "fcntl09_64",
    "fcntl10",
    "fcntl10_64",
    "fcntl11",
    "fcntl11_64",
];

pub const NEXT_TARGET_FS100_FCNTL_BATCH2_TASKS: [&str; 20] = [
    "fcntl12",
    "fcntl12_64",
    "fcntl13",
    "fcntl13_64",
    "fcntl14",
    "fcntl14_64",
    "fcntl15",
    "fcntl15_64",
    "fcntl16",
    "fcntl16_64",
    "fcntl17",
    "fcntl17_64",
    "fcntl18",
    "fcntl18_64",
    "fcntl19",
    "fcntl19_64",
    "fcntl20",
    "fcntl20_64",
    "fcntl21",
    "fcntl21_64",
];

pub const NEXT_TARGET_FS100_FCNTL_BATCH3_TASKS: [&str; 20] = [
    "fcntl22",
    "fcntl22_64",
    "fcntl23",
    "fcntl23_64",
    "fcntl24",
    "fcntl24_64",
    "fcntl25",
    "fcntl25_64",
    "fcntl26",
    "fcntl26_64",
    "fcntl27",
    "fcntl27_64",
    "fcntl29",
    "fcntl29_64",
    "fcntl30",
    "fcntl30_64",
    "fcntl31",
    "fcntl31_64",
    "fcntl32",
    "fcntl32_64",
];

pub const NEXT_TARGET_FS100_FCNTL_BATCH4_TASKS: [&str; 20] = [
    "fcntl33",
    "fcntl33_64",
    "fcntl34",
    "fcntl34_64",
    "fcntl35",
    "fcntl35_64",
    "fcntl36",
    "fcntl36_64",
    "fcntl37",
    "fcntl37_64",
    "fcntl38",
    "fcntl38_64",
    "fcntl39",
    "fcntl39_64",
    "llseek01",
    "llseek02",
    "llseek03",
    "getdents01",
    "getdents02",
    "unlink05",
];

pub const NEXT_TARGET_FS100_RENAME_UNLINK_TASKS: [&str; 20] = [
    "rename01",
    "rename03",
    "rename04",
    "rename05",
    "rename06",
    "rename07",
    "rename08",
    "rename09",
    "rename10",
    "rename11",
    "rename12",
    "rename13",
    "rename14",
    "renameat01",
    "renameat201",
    "renameat202",
    "unlink07",
    "unlink08",
    "unlink09",
    "unlinkat01",
];

// Current focus: 2026-02-26 uncovered 100-target plan.
// Priority from ltp summary Section 5/6:
// fs metadata+sync -> stat/statx -> pipe/sendfile/splice/vmsplice.
const NEXT_TARGET_100_BATCH1_FS_META_SYNC_TASKS: [&str; 20] = [
    "fallocate01",
    "fallocate02",
    "fallocate03",
    "fallocate04",
    "fallocate05",
    "fallocate06",
    "fdatasync01",
    "fdatasync02",
    "fdatasync03",
    "fsync01",
    "fsync02",
    "fsync03",
    "fsync04",
    "sync01",
    "sync_file_range01",
    "sync_file_range02",
    "syncfs01",
    "truncate03_64",
    "readdir01",
    "readdir21",
];

const NEXT_TARGET_100_BATCH2_FS_STAT_CORE_TASKS: [&str; 20] = [
    "fstat02_64",
    "fstat03_64",
    "fstatfs01",
    "fstatfs01_64",
    "fstatfs02_64",
    "lstat01",
    "lstat01_64",
    "lstat02",
    "lstat02_64",
    "stat01_64",
    "stat02_64",
    "stat03_64",
    "statfs01",
    "statfs01_64",
    "statfs02_64",
    "statfs03_64",
    "mknod07",
    "mknodat02",
    "statx04",
    "statx05",
];

const NEXT_TARGET_100_BATCHA_STATX_REMAINING_TASKS: [&str; 7] = [
    "statx06", "statx07", "statx08", "statx09", "statx10", "statx11", "statx12",
];

const NEXT_TARGET_100_BATCHB_PIPE_CORE_TASKS: [&str; 13] = [
    "pipe01", "pipe02", "pipe03", "pipe04", "pipe05", "pipe06", "pipe07", "pipe08", "pipe09",
    "pipe10", "pipe11", "pipe12", "pipe13",
];

const NEXT_TARGET_100_BATCH4_PIPE_SENDFILE_SPLICE_TASKS: [&str; 20] = [
    "pipe14",
    "pipe15",
    "pipe2_01",
    "pipe2_02",
    "pipe2_04",
    "sendfile02",
    "sendfile02_64",
    "sendfile03",
    "sendfile03_64",
    "sendfile04",
    "sendfile04_64",
    "sendfile05",
    "sendfile05_64",
    "sendfile06",
    "sendfile06_64",
    "splice01",
    "splice02",
    "splice03",
    "splice04",
    "splice05",
];

const NEXT_TARGET_100_BATCH5_IO_WRAPUP_TASKS: [&str; 20] = [
    "sendfile07",
    "sendfile07_64",
    "sendfile08",
    "sendfile08_64",
    "sendfile09",
    "sendfile09_64",
    "splice06",
    "splice07",
    "splice08",
    "splice09",
    "tee01",
    "tee02",
    "vmsplice01",
    "vmsplice02",
    "vmsplice03",
    "vmsplice04",
    "posix_fadvise01",
    "posix_fadvise02",
    "posix_fadvise03",
    "posix_fadvise04",
];

// Current focus: 2026-02-26 uncovered 100-target plan.
// Priority from ltp summary Section 5/6:
// fs metadata+sync -> stat/statx -> pipe/sendfile/splice/vmsplice.
const BATCH_20260226_FS_META_SYNC_TASKS: [&str; 20] = [
    "fallocate01",
    "fallocate02",
    "fallocate03",
    "fallocate04",
    "fallocate05",
    "fallocate06",
    "fdatasync01",
    "fdatasync02",
    "fdatasync03",
    "fsync01",
    "fsync02",
    "fsync03",
    "fsync04",
    "sync01",
    "sync_file_range01",
    "sync_file_range02",
    "syncfs01",
    "truncate03_64",
    "readdir01",
    "readdir21",
];

const BATCH_20260226_FS_STAT_CORE_TASKS: [&str; 20] = [
    "fstat02_64",
    "fstat03_64",
    "fstatfs01",
    "fstatfs01_64",
    "fstatfs02_64",
    "lstat01",
    "lstat01_64",
    "lstat02",
    "lstat02_64",
    "stat01_64",
    "stat02_64",
    "stat03_64",
    "statfs01",
    "statfs01_64",
    "statfs02_64",
    "statfs03_64",
    "mknod07",
    "mknodat02",
    "statx04",
    "statx05",
];

const BATCH_20250226_STATX_REMAINING_TASKS: [&str; 7] = [
    "statx06", "statx07", "statx08", "statx09", "statx10", "statx11", "statx12",
];

const NBATCH_20250226_PIPE_CORE_TASKS: [&str; 13] = [
    "pipe01", "pipe02", "pipe03", "pipe04", "pipe05", "pipe06", "pipe07", "pipe08", "pipe09",
    "pipe10", "pipe11", "pipe12", "pipe13",
];

const BATCH_20250226_PIPE_SENDFILE_SPLICE_TASKS: [&str; 20] = [
    "pipe14",
    "pipe15",
    "pipe2_01",
    "pipe2_02",
    "pipe2_04",
    "sendfile02",
    "sendfile02_64",
    "sendfile03",
    "sendfile03_64",
    "sendfile04",
    "sendfile04_64",
    "sendfile05",
    "sendfile05_64",
    "sendfile06",
    "sendfile06_64",
    "splice01",
    "splice02",
    "splice03",
    "splice04",
    "splice05",
];

const BATCH_20250226_IO_WRAPUP_TASKS: [&str; 20] = [
    "sendfile07",
    "sendfile07_64",
    "sendfile08",
    "sendfile08_64",
    "sendfile09",
    "sendfile09_64",
    "splice06",
    "splice07",
    "splice08",
    "splice09",
    "tee01",
    "tee02",
    "vmsplice01",
    "vmsplice02",
    "vmsplice03",
    "vmsplice04",
    "posix_fadvise01",
    "posix_fadvise02",
    "posix_fadvise03",
    "posix_fadvise04",
];

// Current focus: 2026-03-01 uncovered 100-target plan.
// Priority from ltp summary Section 2 + Section 8:
// credential *_16 compatibility -> POSIX MQ + SysV IPC core.
const BATCH_20260301_CRED16_CAP_QUERY_TASKS: [&str; 20] = [
    "cap_bounds_r",
    "cap_bounds_rw",
    "cap_bset_inh_bounds",
    "capset03",
    "capset04",
    "check_pe",
    "check_simple_capset",
    "getegid01_16",
    "getegid02_16",
    "geteuid01_16",
    "geteuid02_16",
    "getgid01_16",
    "getgid03_16",
    "getgroups01_16",
    "getgroups03",
    "getgroups03_16",
    "getresgid01_16",
    "getresgid02_16",
    "getresgid03_16",
    "getresuid01_16",
];

const BATCH_20260301_CRED16_MUTATION_A_TASKS: [&str; 20] = [
    "getresuid02_16",
    "getresuid03_16",
    "getuid01_16",
    "getuid03_16",
    "setfsgid01_16",
    "setfsgid02_16",
    "setfsgid03",
    "setfsgid03_16",
    "setfsuid01_16",
    "setfsuid02_16",
    "setfsuid03_16",
    "setfsuid04_16",
    "setgid01_16",
    "setgid02_16",
    "setgid03_16",
    "setgroups01_16",
    "setgroups02_16",
    "setgroups03",
    "setgroups03_16",
    "setgroups04",
];

const BATCH_20260301_CRED16_MUTATION_B_TASKS: [&str; 20] = [
    "setgroups04_16",
    "setregid01_16",
    "setregid02_16",
    "setregid03_16",
    "setregid04_16",
    "setresgid01_16",
    "setresgid02_16",
    "setresgid03_16",
    "setresgid04_16",
    "setresuid01_16",
    "setresuid02_16",
    "setresuid03_16",
    "setresuid04_16",
    "setresuid05_16",
    "setreuid01_16",
    "setreuid02_16",
    "setreuid03_16",
    "setreuid04_16",
    "setreuid05_16",
    "setreuid06_16",
];

const BATCH_20260301_IPC_MQ_MSG_SEM_TASKS: [&str; 20] = [
    "setreuid07_16",
    "setuid01_16",
    "setuid03_16",
    "setuid04_16",
    "mq_notify01",
    "mq_notify02",
    "mq_notify03",
    "mq_open01",
    "mq_timedreceive01",
    "mq_timedsend01",
    "mq_unlink01",
    "msgctl12",
    "msgget05",
    "msgrcv05",
    "msgrcv06",
    "msgrcv07",
    "msgrcv08",
    "msgsnd06",
    "semctl09",
    "semget02",
];

const BATCH_20260301_IPC_SHM_CORE_TASKS: [&str; 19] = [
    "shmat02", "shmat03", "shmat04", "shmat1", "shmctl01", "shmctl02", "shmctl03", "shmctl04",
    "shmctl05", "shmctl06", "shmctl07", "shmctl08", "shmdt01", "shmdt02", "shmget03", "shmget04",
    "shmget05", "shmget06", "shmt02",
];

// 2026-03-02 SHM follow-up cases that passed on musl+glibc.
pub const BATCH_20260302_IPC_SHM_FOLLOWUP_TASKS: [&str; 9] = [
    "shmget02", "shmt03", "shmt04", "shmt05", "shmt06", "shmt07", "shmt08", "shmt09", "shmt10",
];

// 2026-03-02 IPC namespace communication/isolation subset (musl+glibc).
pub const BATCH_20260302_IPC_NS_COMM_TASKS: [&str; 5] = [
    "msg_comm",
    "sem_comm",
    "shm_comm",
    "shmem_2nstest",
    "shmnstest",
];

// 2026-03-02 IPC namespace POSIX MQ + SysV IPC namespace follow-up (musl+glibc).
pub const BATCH_20260302_IPC_NS_MQ_SEM_TASKS: [&str; 7] = [
    "mesgq_nstest",
    "mqns_01",
    "mqns_02",
    "mqns_03",
    "mqns_04",
    "sem_nstest",
    "semtest_2ns",
];

// 2026-03-09 SysV message stress case passed on musl+glibc.
pub const BATCH_20260309_IPC_MSG_STRESS_TASKS: [&str; 1] = ["msgstress01"];

// 2026-03-02 new uncovered 100-target plan from ltp summary (Section 6/7/9).
pub const BATCH_20260302_IO_AIO_DIO_TASKS: [&str; 34] = [
    "aio-stress",
    "aio01",
    "aio02",
    "aiocp",
    "aiodio_append",
    "aiodio_sparse",
    "io_setup01",
    "io_setup02",
    "io_submit01",
    "io_submit02",
    "io_submit03",
    "io_getevents01",
    "io_getevents02",
    "io_destroy01",
    "io_destroy02",
    "io_cancel01",
    "io_cancel02",
    "io_control01",
    "io_pgetevents01",
    "io_pgetevents02",
    "readahead01",
    "readahead02",
    "dio_append",
    "dio_read",
    "dio_sparse",
    "dio_truncate",
    "diotest1",
    "diotest2",
    "diotest3",
    "diotest4",
    "diotest5",
    "diotest6",
    "doio",
    "dma_thread_diotest",
];

// 2026-03-02 new uncovered 100-target plan from ltp summary (Section 7).
pub const BATCH_20260302_MM_TASKS: [&str; 48] = [
    "mmap-corruption01",
    "mmap001",
    "mmap08",
    "mmap1",
    "mmap2",
    "mmap3",
    "mmap10",
    "mmap11",
    "mmap12",
    "mmap13",
    "mmap14",
    "mmap15",
    "mmap16",
    "mmap17",
    "mmap18",
    "mmap19",
    "mmap20",
    "mmapstress01",
    "mmapstress02",
    "mmapstress03",
    "mmapstress04",
    "mmapstress05",
    "mmapstress06 20",
    "mmapstress07 /tmp/mmapstress07_ltp.tmp",
    "mmapstress08",
    "mmapstress09 -p 20 -t 0.2",
    "mmapstress10 -p 20 -t 0.2",
    "mlock04",
    "mlock05",
    "mlock201",
    "mlock202",
    "mlock203",
    "mlockall03",
    "madvise03",
    "madvise05",
    "madvise06",
    "madvise07",
    "madvise08",
    "madvise09",
    "madvise10",
    "madvise11",
    "mincore04",
    "page01",
    "page02",
    "memfd_create01",
    "memfd_create02",
    "memfd_create03",
    "memfd_create04",
];

pub const BATCH_20260308_MM_OOM_TASKS: [&str; 6] = [
    "oom01",
    "oom02",
    "oom03",
    "oom04",
    "oom05",
    "overcommit_memory",
];

// 2026-03-02 new uncovered 100-target plan from ltp summary (Section 9).
pub const BATCH_20260302_THREADING_TASKS: [&str; 17] = [
    "set_thread_area01",
    "nptl01",
    "ptrace01",
    "ptrace02",
    "ptrace03",
    "ptrace04",
    "ptrace05",
    "ptrace06",
    "ptrace07",
    "ptrace08",
    "ptrace09",
    "ptrace10",
    "ptrace11",
    "pth_str01",
    "pth_str02",
    "pth_str03",
    "run_sched_cliserv.sh",
];

// 2026-03-05 new uncovered 100-target plan from ltp summary (Sections 11-13 + core fs/process gaps).
pub const BATCH_20260305_FS_META_CHOWN_XATTR_TASKS: [&str; 30] = [
    "chown01_16",
    "chown02_16",
    "chown03_16",
    "chown04_16",
    "chown05_16",
    "fchown01_16",
    "fchown02_16",
    "fchown03_16",
    "fchown04_16",
    "fchown05_16",
    "lchown01",
    "lchown02",
    "lchown03",
    "lchown01_16",
    "lchown02_16",
    "lchown03_16",
    "fgetxattr01",
    "fgetxattr02",
    "fgetxattr03",
    "fsetxattr01",
    "fsetxattr02",
    "flistxattr01",
    "flistxattr02",
    "flistxattr03",
    "fremovexattr01",
    "fremovexattr02",
    "llistxattr01",
    "llistxattr02",
    "llistxattr03",
    "lremovexattr01",
];

pub const BATCH_20260305_INOTIFY_TASKS: [&str; 14] = [
    "inotify01",
    "inotify02",
    "inotify03",
    "inotify04",
    "inotify05",
    "inotify06",
    "inotify07",
    "inotify08",
    "inotify09",
    "inotify10",
    "inotify11",
    "inotify12",
    "inotify_init1_01",
    "inotify_init1_02",
];

pub const BATCH_20260305_FS_META_INOTIFY_XATTR_TASKS: [&str; 44] = [
    "chown01_16",
    "chown02_16",
    "chown03_16",
    "chown04_16",
    "chown05_16",
    "fchown01_16",
    "fchown02_16",
    "fchown03_16",
    "fchown04_16",
    "fchown05_16",
    "lchown01",
    "lchown02",
    "lchown03",
    "lchown01_16",
    "lchown02_16",
    "lchown03_16",
    "inotify01",
    "inotify02",
    "inotify03",
    "inotify04",
    "inotify05",
    "inotify06",
    "inotify07",
    "inotify08",
    "inotify09",
    "inotify10",
    "inotify11",
    "inotify12",
    "inotify_init1_01",
    "inotify_init1_02",
    "fgetxattr01",
    "fgetxattr02",
    "fgetxattr03",
    "fsetxattr01",
    "fsetxattr02",
    "flistxattr01",
    "flistxattr02",
    "flistxattr03",
    "fremovexattr01",
    "fremovexattr02",
    "llistxattr01",
    "llistxattr02",
    "llistxattr03",
    "lremovexattr01",
];

pub const BATCH_20260305_PIDFD_PRCTL_TASKS: [&str; 19] = [
    "pidfd_getfd01",
    "pidfd_getfd02",
    "pidfd_open01",
    "pidfd_open02",
    "pidfd_open03",
    "pidfd_open04",
    "pidfd_send_signal01",
    "pidfd_send_signal02",
    "pidfd_send_signal03",
    "prctl01",
    "prctl02",
    "prctl03",
    "prctl04",
    "prctl05",
    "prctl06",
    "prctl07",
    "prctl08",
    "prctl09",
    "prctl10",
];

pub const BATCH_20260305_IOCTL_IOURING_OPEN_TASKS: [&str; 26] = [
    "ioctl01",
    "ioctl02 -d /dev/tty",
    "ioctl03",
    "ioctl04",
    "ioctl05",
    "ioctl06",
    "ioctl07",
    "ioctl08",
    "ioctl09",
    "ioctl_loop01",
    "ioctl_loop02",
    "ioctl_loop03",
    "ioctl_loop04",
    "ioctl_loop05",
    "ioctl_loop06",
    "ioctl_loop07",
    "ioctl_sg01",
    "io_uring01",
    "io_uring02",
    "open12",
    "open14",
    "openat02",
    "openat04",
    "openat201",
    "openat202",
    "openat203",
];

pub const BATCH_20260305_NS_TASKS: [&str; 10] = [
    "mountns01",
    "mountns02",
    "mountns03",
    "mountns04",
    "setns01",
    "setns02",
    "unshare01",
    "unshare02",
    "timens01",
    "pidns01",
];

pub const BATCH_20260305_MISC_TASKS: [&str; 1] = ["kcmp01"];

// 2026-03-06 new uncovered 100-target plan from ltp summary +
// tools/find_unimplement_ltp.py (Sections 12/13/17/18/19).
pub const BATCH_20260306_FANOTIFY_CORE_TASKS: [&str; 20] = [
    "fanotify01",
    "fanotify02",
    "fanotify03",
    "fanotify04",
    "fanotify05",
    "fanotify06",
    "fanotify07",
    "fanotify08",
    "fanotify09",
    "fanotify10",
    "fanotify11",
    "fanotify12",
    "fanotify13",
    "fanotify14",
    "fanotify15",
    "fanotify16",
    "fanotify17",
    "fanotify18",
    "fanotify19",
    "fanotify20",
];

pub const BATCH_20260306_MOUNT_API_TASKS: [&str; 20] = [
    "fanotify21",
    "fanotify22",
    "fanotify23",
    "mount01",
    "mount02",
    "mount03",
    "mount04",
    "mount05",
    "mount06",
    "mount07",
    "umount01",
    "umount02",
    "umount03",
    "umount2_01",
    "umount2_02",
    "mount_setattr01",
    "fsconfig01",
    "fsconfig02",
    "fsconfig03",
    "fsopen01",
];

pub const BATCH_20260306_NS_MOUNT_FOLLOWUP_TASKS: [&str; 20] = [
    "fsopen02",
    "fsmount01",
    "fsmount02",
    "fspick01",
    "fspick02",
    "open_tree01",
    "open_tree02",
    "move_mount01",
    "move_mount02",
    "userns01",
    "userns02",
    "userns03",
    "userns04",
    "userns05",
    "userns06",
    "userns07",
    "userns08",
    "pidns02",
    "pidns03",
    "pidns04",
];

pub const BATCH_20260306_PIDNS_MODULE_TASKS: [&str; 20] = [
    "pidns05",
    "pidns06",
    "pidns10",
    "pidns12",
    "pidns13",
    "pidns16",
    "pidns17",
    "pidns20",
    "pidns30",
    "pidns31",
    "pidns32",
    "init_module01",
    "init_module02",
    "finit_module01",
    "finit_module02",
    "delete_module01",
    "delete_module02",
    "delete_module03",
    "getcpu01",
    "membarrier01",
];

// `pathconf02` stays in this batch, but submit_plan currently runs it only
// in glibc lanes because musl's wrapper does not validate path errors here.
pub const BATCH_20260306_MISC_DEVICE_QUERY_TASKS: [&str; 20] = [
    "ioprio_get01",
    "ioprio_set01",
    "ioprio_set02",
    "ioprio_set03",
    "ioperm01",
    "ioperm02",
    "iopl01",
    "iopl02",
    "perf_event_open01",
    "perf_event_open02",
    "perf_event_open03",
    "sysinfo01",
    "sysinfo02",
    "sysinfo03",
    "personality01",
    "personality02",
    "confstr01",
    "pathconf01",
    "pathconf02",
    "fpathconf01",
];

// 2026-03-08 next uncovered 100-target plan from ltp summary +
// tools/find_unimplement_ltp.py, focused on Sections 14/15/19.
pub const BATCH_20260308_CGROUP_CORE_CPU_TASKS: [&str; 20] = [
    "cgroup_core01",
    "cgroup_core02",
    "cgroup_core03",
    "cgroup_xattr",
    "cfs_bandwidth01",
    "cgroup_regression_test.sh",
    "cgroup_fj_function.sh debug",
    "cgroup_fj_function.sh cpuset",
    "cgroup_fj_function.sh cpu",
    "cgroup_fj_function.sh cpuacct",
    "cgroup_fj_function.sh memory",
    "cgroup_fj_function.sh freezer",
    "cgroup_fj_function.sh devices",
    "cgroup_fj_function.sh blkio",
    "cgroup_fj_function.sh net_cls",
    "cgroup_fj_function.sh perf_event",
    "cgroup_fj_function.sh net_prio",
    "cgroup_fj_function.sh hugetlb",
    "cpuacct.sh 1 1",
    "cpuset01",
];

pub const BATCH_20260308_CGROUP_MEM_PID_TASKS: [&str; 28] = [
    "pids.sh 1 5 0",
    "pids.sh 2 5 0",
    "pids.sh 3 5 0",
    "pids.sh 5 5 0",
    "pids.sh 6 5 0",
    "pids.sh 7 6 3",
    "pids.sh 8 5 0",
    "pids.sh 9 5 0",
    "memcg_control_test.sh",
    "memcg_failcnt.sh",
    "memcg_force_empty.sh",
    "memcg_limit_in_bytes.sh",
    "memcg_max_usage_in_bytes_test.sh",
    "memcg_memsw_limit_in_bytes_test.sh",
    "memcg_move_charge_at_immigrate_test.sh",
    "memcg_regression_test.sh",
    "memcg_stat_rss.sh",
    "memcg_stat_test.sh",
    "memcg_stress_test.sh",
    "memcg_subgroup_charge.sh",
    "memcg_usage_in_bytes_test.sh",
    "memcg_use_hierarchy_test.sh",
    "run_memctl_test.sh 1",
    "run_memctl_test.sh 2",
    "memcontrol01",
    "memcontrol02",
    "memcontrol03",
    "memcontrol04",
];

pub const BATCH_20260308_SECURITY_KEYS_CRYPTO_TASKS: [&str; 20] = [
    "keyctl01",
    "keyctl02",
    "keyctl03",
    "keyctl04",
    "keyctl05",
    "keyctl06",
    "keyctl07",
    "keyctl08",
    "keyctl09",
    "request_key01",
    "request_key02",
    "request_key03",
    "request_key04",
    "request_key05",
    "af_alg01",
    "af_alg02",
    "af_alg03",
    "af_alg04",
    "af_alg05",
    "af_alg06",
];

pub const BATCH_20260308_SECURITY_BPF_CVE_TASKS: [&str; 20] = [
    "af_alg07",
    "crypto_user01",
    "crypto_user02",
    "pcrypt_aead01",
    "bpf_map01",
    "bpf_prog01",
    "bpf_prog02",
    "bpf_prog03",
    "bpf_prog04",
    "bpf_prog05",
    "bpf_prog06",
    "bpf_prog07",
    "cve-2014-0196",
    "cve-2015-3290",
    "cve-2016-10044",
    "cve-2016-7042",
    "cve-2016-7117",
    "cve-2017-16939",
    "cve-2017-17052",
    "cve-2017-17053",
];

pub const BATCH_20260308_MISC_FEATURES_TASKS: [&str; 20] = [
    "cve-2017-2618",
    "cve-2017-2671",
    "cve-2022-4378",
    "newuname01",
    "utsname01",
    "utsname02",
    "utsname03",
    "utsname04",
    "sysconf01",
    "getpagesize01",
    "syscall01",
    "sysfs01",
    "sysfs02",
    "sysfs03",
    "sysfs04",
    "sysfs05",
    "sysctl01",
    "sysctl03",
    "sysctl04",
    "aslr01",
];

// 2026-03-09 high-priority core follow-up chosen from sections 1/4/5/8/10
// of ltp_test_summary. Keep the batch on Linux-like semantics that already
// have substantial kernel support, and exclude known stubbed interfaces such
// as timerfd/clone3 and sysctl-driven SysV IPC capacity controls.
pub const BATCH_20260309_CORE100_TASKS: [&str; 100] = [
    "clone05",
    "clone06",
    "clone07",
    "wait403",
    "execve05",
    "execve06",
    "execveat03",
    "sigaction01",
    "sigaction02",
    "rt_sigaction01",
    "rt_sigaction02",
    "rt_sigaction03",
    "sigaltstack01",
    "sigaltstack02",
    "sigpending02",
    "sigprocmask01",
    "sigrelse01",
    "sigsuspend01",
    "sigtimedwait01",
    "sigwait01",
    "sigwaitinfo01",
    "signal01",
    "signal02",
    "signal03",
    "signal04",
    "signal05",
    "chroot01",
    "chroot02",
    "chroot03",
    "chroot04",
    "getgroups01",
    "setgroups01",
    "setgroups02",
    "msgctl03",
    "msgctl04",
    "msgctl05",
    "msgsnd02",
    "msgsnd05",
    "msgrcv02",
    "msgrcv03",
    "semctl03",
    "semctl04",
    "semctl05",
    "semctl07",
    "semctl08",
    "semop03",
    "dup01",
    "dup02",
    "dup03",
    "dup04",
    "dup05",
    "dup06",
    "dup07",
    "dup201",
    "dup202",
    "dup203",
    "dup204",
    "dup205",
    "dup206",
    "dup207",
    "dup3_01",
    "dup3_02",
    "umask01",
    "copy_file_range01",
    "copy_file_range02",
    "copy_file_range03",
    "clock_gettime01",
    "clock_gettime02",
    "clock_gettime03",
    "clock_gettime04",
    "clock_settime01",
    "clock_settime02",
    "clock_settime03",
    "clock_getres01",
    "clock_nanosleep01",
    "clock_nanosleep02",
    "clock_nanosleep03",
    "clock_nanosleep04",
    "nanosleep01",
    "nanosleep02",
    "nanosleep04",
    "gettimeofday02",
    "fchmod01",
    "fchmod02",
    "fchmod03",
    "fchmod04",
    "fchmod05",
    "fchmod06",
    "fchmodat01",
    "fchmodat02",
    "stat01",
    "stat02",
    "stat03",
    "lstat01",
    "lstat02",
    "fstat02",
    "fstat03",
    "fstatat01",
    "statfs02",
    "statfs03",
];
