//! Per-task resource limits and thread/robust-futex registration.
//
// Auto-extracted from the original flat `mod.rs`; test names are
// preserved verbatim so the set consumed by `submit_plan.rs` is unchanged.

#![allow(dead_code)]

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

// --- Thread-id / robust-futex list ------------------------------------
// Thread robust-list and TID setup helpers.
pub const ROBUST_TID_TASKS: [&str; 3] = [
    "set_robust_list01",
    "get_robust_list01",
    "set_tid_address01",
];
