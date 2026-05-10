//! Time / clocks / timers / alarm / rusage.
//
// Auto-extracted from the original flat `mod.rs`; test names are
// preserved verbatim so the set consumed by `submit_plan.rs` is unchanged.

#![allow(dead_code)]

pub const GETITIMER_TASKS: [&str; 2] = ["getitimer01", "getitimer02"];
pub const SETITIMER_TASKS: [&str; 2] = ["setitimer01", "setitimer02"];
pub const GETRUSAGE_TASKS: [&str; 2] = ["getrusage01", "getrusage02"];

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
