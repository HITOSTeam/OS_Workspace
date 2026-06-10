//! Planning backlog: curated "next push" lists, not yet wired into submit_plan.
//
// Auto-extracted from the original flat `mod.rs`; test names are
// preserved verbatim so the set consumed by `submit_plan.rs` is unchanged.

#![allow(dead_code)]

// One-shot 100-case batch for the next LTP push, grouped by subsystem.
pub const CLONE_WAIT_EXIT_CORE_TASKS: [&str; 12] = [
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

pub const EXEC_FAMILY_CORE_TASKS: [&str; 12] = [
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

pub const CRED_KEY_CAP_CORE_TASKS: [&str; 12] = [
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

pub const COPY_TRUNCATE_CORE_TASKS: [&str; 12] = [
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

pub const MMAP_MPROTECT_CORE_TASKS: [&str; 12] = [
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

pub const MLOCK_MADVISE_CORE_TASKS: [&str; 12] = [
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

pub const SYSV_IPC_CORE_TASKS: [&str; 12] = [
    "msgctl01", "msgctl02", "msgget01", "msgget02", "msgrcv01", "msgsnd01", "semctl01", "semctl02",
    "semop01", "semop02", "semget01", "shmat01",
];

pub const CLOCK_TIMERFD_SIGNALFD_TASKS: [&str; 12] = [
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

pub const UTIME_UNAME_ARCH_PRCTL_TASKS: [&str; 4] =
    ["utime01", "utimensat01", "uname04", "arch_prctl01"];

// Next 100-target campaign (5 x 20) for network core + xattr core.
// Batch order is arranged for incremental bring-up:
// 1) socket lifecycle/connectivity, 2) send/recv + query basics,
// 3) sockopt + select/poll, 4) epoll core paths, 5) xattr + network wrap-up.
pub const NET_SOCKET_CONN_TASKS: [&str; 20] = [
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

pub const NET_SEND_RECV_TASKS: [&str; 18] = [
    "recv01",
    "recvfrom01",
    "recvmsg01",
    "recvmsg02",
    "recvmsg03",
    "send01",
    "send02",
    "sendmmsg01",
    "sendmmsg02",
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

// These error-path tests intentionally pass invalid user pointers through the
// libc wrapper. The bundled musl wrappers touch those pointers in userspace
// before issuing the syscall, so the kernel cannot return Linux EFAULT there.
// Keep them in the glibc lane, whose wrappers enter the kernel like Linux.
pub const NET_SEND_RECV_GLIBC_ONLY_TASKS: [&str; 2] = ["recvmmsg01", "sendmsg01"];

pub const NET_SOCKOPT_POLL_TASKS: [&str; 20] = [
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

pub const EPOLL_CORE_TASKS: [&str; 19] = [
    "epoll_create01",
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

// Default musl maps epoll_create(size) to epoll_create1(0) without preserving
// invalid size values, so epoll_create02 cannot test the kernel errno path
// there. Glibc validates size like Linux user space expects.
pub const EPOLL_GLIBC_ONLY_TASKS: [&str; 1] = ["epoll_create02"];

pub const XATTR_CORE_TASKS: [&str; 20] = [
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
pub const CLONE_EXEC_CHROOT_GROUPS_TASKS: [&str; 20] = [
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

pub const SIGACTION_SIGNAL_CORE_TASKS: [&str; 17] = [
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
    "sigsuspend01",
    "sigwait01",
    "rt_sigaction01",
    "rt_sigaction02",
    "rt_sigaction03",
];

// Default musl reserves one extra internal real-time signal and its libc
// signal-wait wrappers do not match the shipped LTP expectations here. The
// glibc binaries exercise the Linux kernel ABI successfully; keep the optional
// musl raw-syscall adapter in /extra disabled by default.
pub const SIGNAL_GLIBC_ONLY_TASKS: [&str; 3] = ["sigrelse01", "sigtimedwait01", "sigwaitinfo01"];

pub const READ_WRITE_LSEEK_TASKS: [&str; 20] = [
    "read01", "read02", "read03", "read04", "readv01", "readv02", "write02", "write03", "write04",
    "write05", "write06", "writev01", "writev02", "writev03", "writev05", "writev06", "writev07",
    "lseek01", "lseek02", "lseek07",
];

pub const SYSV_IPC_EXT_TASKS: [&str; 20] = [
    "msgctl03", "msgctl04", "msgctl05", "msgctl06", "msgget03", "msgget04", "msgrcv02", "msgrcv03",
    "msgsnd02", "msgsnd05", "semctl03", "semctl04", "semctl05", "semctl06", "semctl07", "semctl08",
    "semop03", "semop04", "semop05", "semget05",
];

pub const MPROTECT_MREMAP_MSYNC_TASKS: [&str; 20] = [
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

pub const CREAT_USERFAULTFD_TASKS: [&str; 3] = ["creat07", "creat09", "userfaultfd01"];

// New 100-test target set (2026-02-24) selected from uncovered tests.
// Ordered for "one batch at a time" progress: signal -> sched -> time -> ipc -> fs io.
pub const KILL_PAUSE_TGKILL_TASKS: [&str; 26] = [
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

pub const SCHED_TC_TASKS: [&str; 7] = [
    "sched_tc0",
    "sched_tc1",
    "sched_tc2",
    "sched_tc3",
    "sched_tc4",
    "sched_tc5",
    "sched_tc6",
];

pub const ADJTIMEX_SETTIMEOFDAY_UTIME_TASKS: [&str; 19] = [
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

pub const EVENTFD_FUTEX_TIMERFD_TASKS: [&str; 25] = [
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

pub const PREAD_PWRITE_PREADV_TASKS: [&str; 23] = [
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

pub const PREADV2_PWRITEV2_TASKS: [&str; 8] = [
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
pub const FCNTL_BASIC_TASKS: [&str; 20] = [
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

pub const FCNTL_EXTENDED_TASKS: [&str; 20] = [
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

pub const FCNTL_MISC_TASKS: [&str; 20] = [
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

pub const FCNTL_LEASE_TASKS: [&str; 20] = [
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

pub const RENAME_UNLINK_TASKS: [&str; 20] = [
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
