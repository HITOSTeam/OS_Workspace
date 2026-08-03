#![no_std]
#![no_main]

extern crate alloc;

#[macro_use]
extern crate user;

use alloc::string::String;
use user::syscall::{
    CREATE, RDWR, TRUNC, close, exit, fork, getpid, open, syscall, waitpid, write,
};

const SYSCALL_UNLINKAT: usize = 35;
const SYSCALL_MKDIRAT: usize = 34;
const SYSCALL_LINKAT: usize = 37;
const SYSCALL_PREAD64: usize = 67;
const SYSCALL_FSTAT: usize = 80;
const SYSCALL_NEWFSTATAT: usize = 79;

const AT_FDCWD: isize = -100;
const AT_REMOVEDIR: usize = 0x200;
const ENOENT: isize = -2;
const WORKERS: usize = 6;
const ITERATIONS: usize = 32;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct KStat {
    st_dev: u64,
    st_ino: u64,
    st_mode: u32,
    st_nlink: u32,
    st_uid: u32,
    st_gid: u32,
    st_rdev: u64,
    __pad: u64,
    st_size: i64,
    st_blksize: u32,
    __pad2: i32,
    st_blocks: u64,
    st_atime_sec: i64,
    st_atime_nsec: i64,
    st_mtime_sec: i64,
    st_mtime_nsec: i64,
    st_ctime_sec: i64,
    st_ctime_nsec: i64,
    __unused: [u32; 2],
}

fn with_c_path<T>(path: &str, f: impl FnOnce(*const u8) -> T) -> T {
    let mut owned = String::from(path);
    owned.push('\0');
    f(owned.as_ptr())
}

fn unlink_with_flags(path: &str, flags: usize) -> isize {
    with_c_path(path, |ptr| {
        syscall(
            SYSCALL_UNLINKAT,
            [AT_FDCWD as usize, ptr as usize, flags, 0, 0, 0],
        )
    })
}

fn unlink(path: &str) -> isize {
    unlink_with_flags(path, 0)
}

fn mkdir(path: &str) -> isize {
    with_c_path(path, |ptr| {
        syscall(
            SYSCALL_MKDIRAT,
            [AT_FDCWD as usize, ptr as usize, 0o755, 0, 0, 0],
        )
    })
}

fn link(old: &str, new: &str) -> isize {
    with_c_path(old, |old_ptr| {
        with_c_path(new, |new_ptr| {
            syscall(
                SYSCALL_LINKAT,
                [
                    AT_FDCWD as usize,
                    old_ptr as usize,
                    AT_FDCWD as usize,
                    new_ptr as usize,
                    0,
                    0,
                ],
            )
        })
    })
}

fn stat_path(path: &str, stat: &mut KStat) -> isize {
    with_c_path(path, |ptr| {
        syscall(
            SYSCALL_NEWFSTATAT,
            [
                AT_FDCWD as usize,
                ptr as usize,
                stat as *mut KStat as usize,
                0,
                0,
                0,
            ],
        )
    })
}

fn fstat(fd: usize, stat: &mut KStat) -> isize {
    syscall(SYSCALL_FSTAT, [fd, stat as *mut KStat as usize, 0, 0, 0, 0])
}

fn pread(fd: usize, buf: &mut [u8], offset: usize) -> isize {
    syscall(
        SYSCALL_PREAD64,
        [fd, buf.as_mut_ptr() as usize, buf.len(), offset, 0, 0],
    )
}

fn exercise_worker(worker: usize) {
    let pid = getpid();
    for iteration in 0..ITERATIONS {
        let path = alloc::format!("/tmp/open_unlink_{}_{}_{}", pid, worker, iteration);
        let old_data = [worker as u8, iteration as u8, 0xa5, 0x5a];
        let new_data = [0x11, 0x22, worker as u8, iteration as u8];
        let _ = unlink(&path);

        let old_fd = open(&path, RDWR | CREATE | TRUNC);
        assert!(old_fd >= 0, "open old: {old_fd}");
        let old_fd = old_fd as usize;
        assert_eq!(write(old_fd, &old_data), old_data.len() as isize);
        assert_eq!(unlink(&path), 0, "unlink open file");

        let mut stat = KStat::default();
        assert_eq!(stat_path(&path, &mut stat), ENOENT, "name survived unlink");
        assert_eq!(fstat(old_fd, &mut stat), 0, "fstat old description");
        assert_eq!(stat.st_size, old_data.len() as i64);
        let mut readback = [0u8; 4];
        assert_eq!(pread(old_fd, &mut readback, 0), readback.len() as isize);
        assert_eq!(readback, old_data, "old description lost contents");

        // Reusing the pathname must create a distinct inode while the old
        // open description remains fully usable.
        let new_fd = open(&path, RDWR | CREATE | TRUNC);
        assert!(new_fd >= 0, "open replacement: {new_fd}");
        let new_fd = new_fd as usize;
        assert_eq!(write(new_fd, &new_data), new_data.len() as isize);
        let mut replacement = KStat::default();
        assert_eq!(fstat(new_fd, &mut replacement), 0);
        assert_ne!(replacement.st_ino, stat.st_ino, "pathname reused old inode");
        assert_eq!(close(new_fd), 0);
        assert_eq!(unlink(&path), 0);

        readback.fill(0);
        assert_eq!(pread(old_fd, &mut readback, 0), readback.len() as isize);
        assert_eq!(readback, old_data, "replacement changed old description");
        assert_eq!(close(old_fd), 0);
    }

    // An inode can have several dentries.  Unlinking two hard-link names
    // while the description remains open must retain both cleanup records;
    // otherwise one hidden compatibility name leaks and keeps the directory
    // permanently non-empty after the final close.
    let directory = alloc::format!("/tmp/open_unlink_links_{}_{}", pid, worker);
    let first = alloc::format!("{directory}/first");
    let second = alloc::format!("{directory}/second");
    assert_eq!(mkdir(&directory), 0, "mkdir hard-link case");
    let fd = open(&first, RDWR | CREATE | TRUNC);
    assert!(fd >= 0, "open hard-link case: {fd}");
    let fd = fd as usize;
    let data = [0x5a, worker as u8, 0xa5, 0x3c];
    assert_eq!(write(fd, &data), data.len() as isize);
    assert_eq!(link(&first, &second), 0, "link second name");
    assert_eq!(unlink(&first), 0, "unlink first hard link");
    assert_eq!(unlink(&second), 0, "unlink second hard link");
    let mut readback = [0u8; 4];
    assert_eq!(pread(fd, &mut readback, 0), readback.len() as isize);
    assert_eq!(readback, data, "hard-linked open description lost contents");
    assert_eq!(close(fd), 0);
    assert_eq!(
        unlink_with_flags(&directory, AT_REMOVEDIR),
        0,
        "deferred hard-link cleanup leaked a hidden name"
    );
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let mut children = [0isize; WORKERS];
    for worker in 0..WORKERS {
        let child = fork();
        if child == 0 {
            exercise_worker(worker);
            exit(0);
        }
        assert!(child > 0, "fork: {child}");
        children[worker] = child;
    }

    for child in children {
        let mut status = -1;
        assert_eq!(waitpid(child, &mut status), child);
        assert_eq!(status, 0, "worker failed");
    }
    println!(
        "OPEN_UNLINK_LIFETIME_PASS workers={} iterations={}",
        WORKERS, ITERATIONS
    );
    0
}
