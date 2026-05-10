//! Basic FS I/O: cwd, access, open/close/close_range/umask, fd duplication.
//
// Auto-extracted from the original flat `mod.rs`; test names are
// preserved verbatim so the set consumed by `submit_plan.rs` is unchanged.

#![allow(dead_code)]

pub const CWD_DIR_TASKS: [&str; 6] = [
    "getcwd01", "getcwd02", "getcwd03", "getcwd04", "chdir01", "chdir04",
];
pub const ACCESS_TASKS: [&str; 4] = ["access01", "access02", "access03", "access04"];
pub const FACCESSAT_TASKS: [&str; 4] =
    ["faccessat01", "faccessat02", "faccessat201", "faccessat202"];
// --- open / close / close_range / umask -------------------------------
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

// File descriptor duplication tests.
pub const DUP_CORE_TASKS: [&str; 9] = [
    "dup01", "dup02", "dup03", "dup04", "dup05", "dup06", "dup07", "dup3_01", "dup3_02",
];
pub const DUP_FCNTL_TASKS: [&str; 7] = [
    "dup201", "dup202", "dup203", "dup204", "dup205", "dup206", "dup207",
];
