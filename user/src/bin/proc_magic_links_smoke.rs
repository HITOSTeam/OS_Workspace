#![no_std]
#![no_main]

extern crate alloc;

#[macro_use]
extern crate user;

use alloc::string::String;
use core::str;
use user::syscall::{
    CREATE, RDONLY, RDWR, TRUNC, chdir, close, getcwd, getpid, open, openat, read, syscall, write,
};

const SYSCALL_OPENAT: usize = 56;
const SYSCALL_READLINKAT: usize = 78;
const SYSCALL_NEWFSTATAT: usize = 79;
const SYSCALL_UNLINKAT: usize = 35;

const AT_FDCWD: isize = -100;
const AT_EMPTY_PATH: usize = 0x1000;

const O_DIRECTORY: usize = 0x10000;
const O_NOFOLLOW: usize = 0x20000;
const O_PATH: usize = 0x200000;

const ENOENT: isize = -2;
const ELOOP: isize = -40;

const S_IFMT: u32 = 0o170000;
const S_IFLNK: u32 = 0o120000;

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

fn linux_openat(dirfd: isize, path: &str, flags: usize, mode: usize) -> isize {
    with_c_path(path, |ptr| {
        syscall(SYSCALL_OPENAT, [
            dirfd as usize,
            ptr as usize,
            flags,
            mode,
            0,
            0,
        ])
    })
}

fn linux_readlinkat(dirfd: isize, path: &str, buf: &mut [u8]) -> isize {
    with_c_path(path, |ptr| {
        syscall(SYSCALL_READLINKAT, [
            dirfd as usize,
            ptr as usize,
            buf.as_mut_ptr() as usize,
            buf.len(),
            0,
            0,
        ])
    })
}

fn linux_newfstatat(dirfd: isize, path: &str, st: &mut KStat, flags: usize) -> isize {
    with_c_path(path, |ptr| {
        syscall(SYSCALL_NEWFSTATAT, [
            dirfd as usize,
            ptr as usize,
            st as *mut KStat as usize,
            flags,
            0,
            0,
        ])
    })
}

fn linux_unlinkat(dirfd: isize, path: &str, flags: usize) -> isize {
    with_c_path(path, |ptr| {
        syscall(SYSCALL_UNLINKAT, [
            dirfd as usize,
            ptr as usize,
            flags,
            0,
            0,
            0,
        ])
    })
}

fn assert_path_reads(path: &str, expected: &[u8]) {
    let fd = open(path, RDONLY);
    assert!(fd >= 0);
    let fd = fd as usize;

    let mut buf = [0u8; 64];
    let len = read(fd, &mut buf);
    assert_eq!(len, expected.len() as isize);
    assert_eq!(&buf[..expected.len()], expected);
    assert_eq!(close(fd), 0);
}

fn open_symlink_fd(path: &str) -> usize {
    let fd = linux_openat(AT_FDCWD, path, O_PATH | O_NOFOLLOW, 0);
    assert!(fd >= 0);
    fd as usize
}

fn assert_symlink_fd_target(fd: usize, expected_target: &str) {
    let mut buf = [0u8; 256];
    let len = linux_readlinkat(fd as isize, "", &mut buf);
    assert!(len >= 0);
    let target = str::from_utf8(&buf[..len as usize]).unwrap();
    assert_eq!(target, expected_target);
}

fn assert_symlink_fd_mode(fd: usize) {
    let mut st = KStat::default();
    assert_eq!(linux_newfstatat(fd as isize, "", &mut st, AT_EMPTY_PATH), 0);
    assert_eq!(st.st_mode & S_IFMT, S_IFLNK);
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let orig_cwd = getcwd();
    assert_eq!(chdir("/tmp"), 0);

    let cwd = getcwd();
    let name = alloc::format!("proc_magic_links_smoke_{}", getpid());
    let payload = b"proc-magic-links-smoke";

    let fd = openat(AT_FDCWD, &name, RDWR | CREATE | TRUNC, 0o644);
    assert!(fd >= 0);
    let fd = fd as usize;
    assert_eq!(write(fd, payload), payload.len() as isize);
    assert_eq!(close(fd), 0);

    let cwd_path = alloc::format!("/proc/self/cwd/{}", name);
    assert_path_reads(&cwd_path, payload);

    let dirfd = linux_openat(AT_FDCWD, &cwd, O_DIRECTORY, 0);
    assert!(dirfd >= 0);
    let dirfd = dirfd as usize;

    let fd_path = alloc::format!("/proc/self/fd/{}/{}", dirfd, name);
    assert_path_reads(&fd_path, payload);

    assert_eq!(
        linux_openat(AT_FDCWD, "/proc/self/cwd", O_NOFOLLOW, 0),
        ELOOP
    );
    let cwd_link_fd = open_symlink_fd("/proc/self/cwd");
    assert_symlink_fd_target(cwd_link_fd, &cwd);
    assert_symlink_fd_mode(cwd_link_fd);

    assert_eq!(chdir(&orig_cwd), 0);
    assert_symlink_fd_target(cwd_link_fd, &orig_cwd);
    assert_symlink_fd_mode(cwd_link_fd);
    assert_eq!(chdir(&cwd), 0);
    assert_eq!(close(cwd_link_fd), 0);

    let dir_link = alloc::format!("/proc/self/fd/{}", dirfd);
    let dir_link_fd = open_symlink_fd(&dir_link);
    assert_symlink_fd_target(dir_link_fd, &cwd);
    assert_symlink_fd_mode(dir_link_fd);

    assert_eq!(close(dirfd), 0);
    let mut buf = [0u8; 256];
    assert_eq!(linux_readlinkat(dir_link_fd as isize, "", &mut buf), ENOENT);
    assert_symlink_fd_mode(dir_link_fd);
    assert_eq!(close(dir_link_fd), 0);

    assert_eq!(linux_unlinkat(AT_FDCWD, &name, 0), 0);
    assert_eq!(chdir(&orig_cwd), 0);

    println!("proc_magic_links_smoke passed");
    0
}
