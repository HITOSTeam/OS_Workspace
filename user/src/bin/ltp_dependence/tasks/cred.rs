//! Credential (uid/gid/euid/egid/fsuid) query and mutation tests.
//
// Auto-extracted from the original flat `mod.rs`; test names are
// preserved verbatim so the set consumed by `submit_plan.rs` is unchanged.

#![allow(dead_code)]

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
