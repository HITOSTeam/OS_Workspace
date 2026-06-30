#![no_std]
#![no_main]

extern crate alloc;

#[macro_use]
extern crate user;

use alloc::string::String;
use user::syscall::{CREATE, RDWR, TRUNC, close, open, syscall, write};

const SYSCALL_UNLINKAT: usize = 35;
const SYSCALL_RENAMEAT: usize = 38;
const SYSCALL_NEWFSTATAT: usize = 79;

const AT_FDCWD: isize = -100;
const ENOENT: isize = -2;

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

fn linux_newfstatat(path: &str, st: &mut KStat) -> isize {
    with_c_path(path, |ptr| {
        syscall(
            SYSCALL_NEWFSTATAT,
            [
                AT_FDCWD as usize,
                ptr as usize,
                st as *mut KStat as usize,
                0,
                0,
                0,
            ],
        )
    })
}

fn linux_unlinkat(path: &str) -> isize {
    with_c_path(path, |ptr| {
        syscall(
            SYSCALL_UNLINKAT,
            [AT_FDCWD as usize, ptr as usize, 0, 0, 0, 0],
        )
    })
}

fn linux_renameat(old_path: &str, new_path: &str) -> isize {
    with_c_path(old_path, |old_ptr| {
        with_c_path(new_path, |new_ptr| {
            syscall(
                SYSCALL_RENAMEAT,
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

fn create_file(path: &str, data: &[u8]) {
    let fd = open(path, RDWR | CREATE | TRUNC);
    assert!(fd >= 0);
    let fd = fd as usize;
    assert_eq!(write(fd, data), data.len() as isize);
    assert_eq!(close(fd), 0);
}

fn stat_twice(path: &str) -> KStat {
    let mut st = KStat::default();
    assert_eq!(linux_newfstatat(path, &mut st), 0);
    assert_eq!(linux_newfstatat(path, &mut st), 0);
    st
}

fn assert_missing(path: &str) {
    let mut st = KStat::default();
    assert_eq!(linux_newfstatat(path, &mut st), ENOENT);
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let old_path = "/tmp/path_cache_invalidation_smoke.old";
    let new_path = "/tmp/path_cache_invalidation_smoke.new";
    let unlink_path = "/tmp/path_cache_invalidation_smoke.unlink";

    let _ = linux_unlinkat(old_path);
    let _ = linux_unlinkat(new_path);
    let _ = linux_unlinkat(unlink_path);

    create_file(old_path, b"rename");
    let before = stat_twice(old_path);
    assert_eq!(linux_renameat(old_path, new_path), 0);
    assert_missing(old_path);

    let after = stat_twice(new_path);
    assert_eq!(before.st_dev, after.st_dev);
    assert_eq!(before.st_ino, after.st_ino);
    assert_eq!(linux_unlinkat(new_path), 0);
    assert_missing(new_path);

    create_file(unlink_path, b"unlink");
    let _ = stat_twice(unlink_path);
    assert_eq!(linux_unlinkat(unlink_path), 0);
    assert_missing(unlink_path);

    println!("path_cache_invalidation_smoke passed");
    0
}
