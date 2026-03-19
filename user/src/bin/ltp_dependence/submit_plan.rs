use super::*;

fn run_for_both_libcs(run_group: fn(&str, &[&str]), tasks: &[&str]) {
    run_group("/musl", tasks);
    run_group("/glibc", tasks);
}

const BATCH_20260304_BLOCKED_TASKS: [&str; 1] = ["dma_thread_diotest"];
const FOCUS_BLOCKER_DEBUG: bool = false;
const BLOCKER_FOCUSED_TASKS: [&str; 6] = [
    "write_freezing.sh",
    "freeze_write_freezing.sh",
    "freeze_thaw.sh",
    "freeze_self_thaw.sh",
    "freeze_sleep_thaw.sh",
    "freeze_move_thaw.sh",
];
const FOCUS_CGROUP_MEMCTL_DEBUG: bool = false;
const CGROUP_MEMCTL_FOCUSED_TASKS: [&str; 6] = [
    "run_memctl_test.sh 1",
    "run_memctl_test.sh 2",
    "memcontrol01",
    "memcontrol02",
    "memcontrol03",
    "memcontrol04",
];
const FOCUS_WAIT_REGRESSION: bool = false;
const WAIT_REGRESSION_TASKS: [&str; 5] = ["wait01", "wait02", "wait401", "wait402", "wait403"];
const FOCUS_KILL_CORE_REGRESSION: bool = false;
const KILL_CORE_REGRESSION_TASKS: [&str; 3] = ["kill11", "kill12", "kill13"];
const FOCUS_SIGRELSE_REGRESSION: bool = false;
const SIGRELSE_REGRESSION_TASKS: [&str; 1] = ["sigrelse01"];
const FOCUS_SIGWAIT_DEBUG: bool = false;
const SIGWAIT_DEBUG_TASKS: [&str; 2] = ["sigtimedwait01", "sigwaitinfo01"];
const FOCUS_CLOCK_SETTIME_DEBUG: bool = false;
const CLOCK_SETTIME_DEBUG_TASKS: [&str; 1] = ["clock_settime03"];
const FOCUS_POSIX_TIMER_DEBUG: bool = false;
const FOCUS_MSGSTRESS_DEBUG: bool = false;
const MSGSTRESS_DEBUG_TASKS: [&str; 1] = ["msgstress01"];
const FOCUS_CPUCTL_FJ_DEBUG: bool = false;
const CPUCTL_FJ_DEBUG_TASKS: [&str; 1] = ["run_cpuctl_test_fj.sh"];
const FOCUS_POLL_EPOLL_DEBUG: bool = false;
const POLL_EPOLL_DEBUG_TASKS: [&str; 18] = [
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
    "pselect02_64",
    "pselect03",
    "pselect03_64",
    "epoll_create01",
    "epoll_create02",
    "epoll_create1_01",
    "epoll_create1_02",
    "epoll-ltp",
];
const FOCUS_EVENTFD_DEBUG: bool = false;
const EVENTFD_DEBUG_TASKS: [&str; 9] = [
    "eventfd01",
    "eventfd02",
    "eventfd03",
    "eventfd04",
    "eventfd05",
    "eventfd06",
    "eventfd2_01",
    "eventfd2_02",
    "eventfd2_03",
];
const FOCUS_TIMERFD_DEBUG: bool = false;
const TIMERFD_DEBUG_TASKS: [&str; 6] = [
    "timerfd01",
    "timerfd02",
    "timerfd_create01",
    "timerfd_gettime01",
    "timerfd_settime01",
    "timerfd_settime02",
];
const FOCUS_PROC_MAGIC_DEBUG: bool = false;
const PROC_MAGIC_DEBUG_TASKS: [&str; 4] = [
    "open13",
    "readlink03",
    "readlinkat02",
    "commands/sysctl/sysctl02.sh",
];
const FOCUS_PROC_SYSCTL_DEBUG: bool = false;
const PROC_SYSCTL_DEBUG_TASKS: [&str; 6] = [
    "proc01",
    "sysctl01",
    "sysctl03",
    "sysctl04",
    "commands/sysctl/sysctl01.sh",
    "commands/sysctl/sysctl02.sh",
];
const FOCUS_PIDFD_DEBUG: bool = false;
const PIDFD_DEBUG_TASKS: [&str; 9] = [
    "pidfd_getfd01",
    "pidfd_getfd02",
    "pidfd_open01",
    "pidfd_open02",
    "pidfd_open03",
    "pidfd_open04",
    "pidfd_send_signal01",
    "pidfd_send_signal02",
    "pidfd_send_signal03",
];
const FOCUS_USERFAULTFD_DEBUG: bool = false;
const USERFAULTFD_DEBUG_TASKS: [&str; 1] = ["userfaultfd01"];
const FOCUS_MOUNTNS_DEBUG: bool = false;
const MOUNTNS_DEBUG_TASKS: [&str; 4] = ["mountns01", "mountns02", "mountns03", "mountns04"];
const RUN_20260302_IO: bool = false;
const RUN_20260302_MM: bool = false;
const RUN_20260302_THREADING: bool = false;
const FOCUS_PTRACE_DEBUG: bool = false;
const PTRACE_FOCUSED_TASKS: [&str; 3] = ["ptrace11", "ptrace01", "run_sched_cliserv.sh"];
const RUN_20260305_PHASE1: bool = false;
const RUN_20260305_PHASE1_META_XATTR: bool = false;
const RUN_20260305_PHASE2: bool = false;
const RUN_20260305_PHASE3: bool = false;
const RUN_20260305_PHASE4: bool = false;
const RUN_20260305_PHASE5: bool = false;
const BATCH_20260306_MUSL_BLOCKED_TASKS: [&str; 1] = ["pathconf02"];
const RUN_FORK_SMOKE: bool = false;
const RUN_20260306_PHASE1: bool = false;
const RUN_20260306_PHASE2: bool = false;
const RUN_20260306_PHASE3: bool = false;
const RUN_20260306_PHASE4: bool = false;
const RUN_20260306_PHASE5: bool = false;
const RUN_20260308_PHASE1: bool = false;
const RUN_20260308_PHASE2: bool = false;
const RUN_20260308_PHASE3: bool = false;
const RUN_20260308_PHASE4: bool = false;
const RUN_20260308_PHASE5: bool = false;
const RUN_20260309_CORE100: bool = false;

fn filtered_tasks(tasks: &[&'static str], blocked: &[&str]) -> alloc::vec::Vec<&'static str> {
    tasks
        .iter()
        .copied()
        .filter(|t| !blocked.contains(t))
        .collect()
}

fn run_selected_ltp_groups(run_group: fn(&str, &[&str])) {
    if !(RUN_20260306_PHASE1
        || RUN_20260306_PHASE2
        || RUN_20260306_PHASE3
        || RUN_20260306_PHASE4
        || RUN_20260306_PHASE5
        || RUN_20260308_PHASE1
        || RUN_20260308_PHASE2
        || RUN_20260308_PHASE3
        || RUN_20260308_PHASE4
        || RUN_20260308_PHASE5)
    {
        if RUN_FORK_SMOKE {
            run_group("/musl", FORK_TASKS.as_ref());
        }
    }

    if FOCUS_BLOCKER_DEBUG {
        run_for_both_libcs(run_group, BLOCKER_FOCUSED_TASKS.as_ref());
    }
    if FOCUS_CGROUP_MEMCTL_DEBUG {
        run_for_both_libcs(run_group, CGROUP_MEMCTL_FOCUSED_TASKS.as_ref());
    }
    if FOCUS_WAIT_REGRESSION {
        run_for_both_libcs(run_group, WAIT_REGRESSION_TASKS.as_ref());
        run_for_both_libcs(run_group, WAITPID_TASKS.as_ref());
        run_for_both_libcs(run_group, WAITID_TASKS.as_ref());
    }
    if FOCUS_KILL_CORE_REGRESSION {
        run_for_both_libcs(run_group, KILL_CORE_REGRESSION_TASKS.as_ref());
    }
    if FOCUS_SIGRELSE_REGRESSION {
        run_for_both_libcs(run_group, SIGRELSE_REGRESSION_TASKS.as_ref());
    }
    if FOCUS_SIGWAIT_DEBUG {
        run_for_both_libcs(run_group, SIGWAIT_DEBUG_TASKS.as_ref());
    }
    if FOCUS_CLOCK_SETTIME_DEBUG {
        run_for_both_libcs(run_group, CLOCK_SETTIME_DEBUG_TASKS.as_ref());
    }
    if FOCUS_POSIX_TIMER_DEBUG {
        run_for_both_libcs(run_group, POSIX_TIMER_TASKS.as_ref());
    }
    if FOCUS_MSGSTRESS_DEBUG {
        run_for_both_libcs(run_group, MSGSTRESS_DEBUG_TASKS.as_ref());
    }
    if FOCUS_CPUCTL_FJ_DEBUG {
        run_group("/musl", CPUCTL_FJ_DEBUG_TASKS.as_ref());
    }
    if FOCUS_POLL_EPOLL_DEBUG {
        run_for_both_libcs(run_group, POLL_EPOLL_DEBUG_TASKS.as_ref());
    }
    if FOCUS_EVENTFD_DEBUG {
        run_for_both_libcs(run_group, EVENTFD_DEBUG_TASKS.as_ref());
    }
    if FOCUS_TIMERFD_DEBUG {
        run_for_both_libcs(run_group, TIMERFD_DEBUG_TASKS.as_ref());
    }
    if FOCUS_PROC_MAGIC_DEBUG {
        run_for_both_libcs(run_group, PROC_MAGIC_DEBUG_TASKS.as_ref());
    }
    if FOCUS_PROC_SYSCTL_DEBUG {
        run_for_both_libcs(run_group, PROC_SYSCTL_DEBUG_TASKS.as_ref());
    }
    if FOCUS_PIDFD_DEBUG {
        run_for_both_libcs(run_group, PIDFD_DEBUG_TASKS.as_ref());
    }
    if FOCUS_USERFAULTFD_DEBUG {
        run_for_both_libcs(run_group, USERFAULTFD_DEBUG_TASKS.as_ref());
    }
    if FOCUS_MOUNTNS_DEBUG {
        run_for_both_libcs(run_group, MOUNTNS_DEBUG_TASKS.as_ref());
    }

    if RUN_20260302_IO {
        let tasks = filtered_tasks(
            BATCH_20260302_IO_AIO_DIO_TASKS.as_ref(),
            &BATCH_20260304_BLOCKED_TASKS,
        );
        run_for_both_libcs(run_group, tasks.as_slice());
    }
    if RUN_20260302_MM {
        run_for_both_libcs(run_group, BATCH_20260302_MM_TASKS.as_ref());
    }
    if RUN_20260302_THREADING {
        run_for_both_libcs(run_group, BATCH_20260302_THREADING_TASKS.as_ref());
    }
    if FOCUS_PTRACE_DEBUG {
        run_for_both_libcs(run_group, PTRACE_FOCUSED_TASKS.as_ref());
    }

    if RUN_20260305_PHASE1 {
        run_for_both_libcs(
            run_group,
            BATCH_20260305_FS_META_INOTIFY_XATTR_TASKS.as_ref(),
        );
    }
    if RUN_20260305_PHASE1_META_XATTR {
        run_for_both_libcs(run_group, BATCH_20260305_FS_META_CHOWN_XATTR_TASKS.as_ref());
    }
    if RUN_20260305_PHASE2 {
        run_for_both_libcs(run_group, BATCH_20260305_PIDFD_PRCTL_TASKS.as_ref());
    }
    if RUN_20260305_PHASE3 {
        run_for_both_libcs(run_group, BATCH_20260305_IOCTL_IOURING_OPEN_TASKS.as_ref());
    }
    if RUN_20260305_PHASE4 {
        run_for_both_libcs(run_group, BATCH_20260305_NS_TASKS.as_ref());
    }
    if RUN_20260305_PHASE5 {
        run_for_both_libcs(run_group, BATCH_20260305_MISC_TASKS.as_ref());
    }

    if RUN_20260306_PHASE1 {
        run_for_both_libcs(run_group, BATCH_20260306_FANOTIFY_CORE_TASKS.as_ref());
    }
    if RUN_20260306_PHASE2 {
        run_for_both_libcs(run_group, BATCH_20260306_MOUNT_API_TASKS.as_ref());
    }
    if RUN_20260306_PHASE3 {
        run_for_both_libcs(run_group, BATCH_20260306_NS_MOUNT_FOLLOWUP_TASKS.as_ref());
    }
    if RUN_20260306_PHASE4 {
        run_for_both_libcs(run_group, BATCH_20260306_PIDNS_MODULE_TASKS.as_ref());
    }
    if RUN_20260306_PHASE5 {
        let musl_tasks = filtered_tasks(
            BATCH_20260306_MISC_DEVICE_QUERY_TASKS.as_ref(),
            &BATCH_20260306_MUSL_BLOCKED_TASKS,
        );
        run_group("/musl", musl_tasks.as_slice());
        run_group("/glibc", BATCH_20260306_MISC_DEVICE_QUERY_TASKS.as_ref());
    }

    if RUN_20260308_PHASE1 {
        run_for_both_libcs(run_group, BATCH_20260308_CGROUP_CORE_CPU_TASKS.as_ref());
    }
    if RUN_20260308_PHASE2 {
        run_for_both_libcs(run_group, BATCH_20260308_CGROUP_MEM_PID_TASKS.as_ref());
    }
    if RUN_20260308_PHASE3 {
        run_for_both_libcs(
            run_group,
            BATCH_20260308_SECURITY_KEYS_CRYPTO_TASKS.as_ref(),
        );
    }
    if RUN_20260308_PHASE4 {
        run_for_both_libcs(run_group, BATCH_20260308_SECURITY_BPF_CVE_TASKS.as_ref());
    }
    if RUN_20260308_PHASE5 {
        run_for_both_libcs(run_group, BATCH_20260308_MISC_FEATURES_TASKS.as_ref());
    }
    if RUN_20260309_CORE100 {
        run_for_both_libcs(run_group, BATCH_20260309_CORE100_TASKS.as_ref());
    }

    // waitpid part
    // run_group("/musl", WAITPID_TASKS.as_ref());
    // ltp we only run part of ltp test cases here, havent implement all syscalls yet
    // run_for_both_libcs(run_group, LTP_TEST_POINTS.as_ref());
}

pub fn run_riscv_ltp_groups(run_group: fn(&str, &[&str])) {
    run_selected_ltp_groups(run_group);
}

pub fn run_non_riscv_ltp_groups(run_group: fn(&str, &[&str])) {
    run_selected_ltp_groups(run_group);
}
