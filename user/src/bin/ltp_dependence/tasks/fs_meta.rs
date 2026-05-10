//! FS metadata: chown/chmod/creat, stat family, tree-mutation (link/mkdir/...).
//
// Auto-extracted from the original flat `mod.rs`; test names are
// preserved verbatim so the set consumed by `submit_plan.rs` is unchanged.

#![allow(dead_code)]

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


// Stat-family descriptor tests.
pub const FSTAT_TASKS: [&str; 3] = ["fstat02", "fstat03", "fstatat01"];
// fstatfs01 requires free mount devices from LTP harness and currently breaks
// in this environment with TBROK before reaching syscall checks.
pub const FSTATFS_TASKS: [&str; 1] = ["fstatfs02"];
// statfs01 depends on mount-device allocation from LTP harness.
pub const STATFS_TASKS: [&str; 2] = ["statfs02", "statfs03"];
pub const STATX_BASIC_TASKS: [&str; 3] = ["statx01", "statx02", "statx03"];
pub const STAT_TASKS: [&str; 3] = ["stat01", "stat02", "stat03"];
// --- Tree mutation: mknod / mkdir / rmdir / link / symlink / readlink -
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
