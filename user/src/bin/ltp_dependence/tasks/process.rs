//! Process lifecycle, identity, session and scheduler/priority tests.
//
// Auto-extracted from the original flat `mod.rs`; test names are
// preserved verbatim so the set consumed by `submit_plan.rs` is unchanged.

#![allow(dead_code)]

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
pub const SCHED_NICE_CORE_TASKS: [&str; 31] = [
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
