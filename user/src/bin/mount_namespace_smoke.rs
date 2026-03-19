#![no_std]
#![no_main]

extern crate alloc;

#[macro_use]
extern crate user;

use alloc::string::{String, ToString};
use core::str;
use user::syscall::{RDONLY, close, getpid, open, read, syscall};

const SYSCALL_OPENAT: usize = 56;
const SYSCALL_READLINKAT: usize = 78;
const SYSCALL_MKDIRAT: usize = 34;
const SYSCALL_UNLINKAT: usize = 35;
const SYSCALL_MOUNT: usize = 40;
const SYSCALL_UNSHARE: usize = 97;
const SYSCALL_SETNS: usize = 268;

const AT_FDCWD: isize = -100;
const AT_REMOVEDIR: usize = 0x200;

const O_NOFOLLOW: usize = 0x20000;
const O_PATH: usize = 0x200000;

const CLONE_NEWNS: usize = 0x0002_0000;
const MS_BIND: usize = 0x1000;

const EEXIST: isize = -17;

fn with_c_path<T>(path: &str, f: impl FnOnce(*const u8) -> T) -> T {
    let mut owned = String::from(path);
    owned.push('\0');
    f(owned.as_ptr())
}

fn with_opt_c_path<T>(path: Option<&str>, f: impl FnOnce(*const u8) -> T) -> T {
    if let Some(path) = path {
        with_c_path(path, f)
    } else {
        f(core::ptr::null())
    }
}

fn linux_openat(dirfd: isize, path: &str, flags: usize, mode: usize) -> isize {
    with_c_path(path, |ptr| {
        syscall(
            SYSCALL_OPENAT,
            [dirfd as usize, ptr as usize, flags, mode, 0, 0],
        )
    })
}

fn linux_readlinkat(dirfd: isize, path: &str, buf: &mut [u8]) -> isize {
    with_c_path(path, |ptr| {
        syscall(
            SYSCALL_READLINKAT,
            [
                dirfd as usize,
                ptr as usize,
                buf.as_mut_ptr() as usize,
                buf.len(),
                0,
                0,
            ],
        )
    })
}

fn linux_mkdirat(dirfd: isize, path: &str, mode: usize) -> isize {
    with_c_path(path, |ptr| {
        syscall(
            SYSCALL_MKDIRAT,
            [dirfd as usize, ptr as usize, mode, 0, 0, 0],
        )
    })
}

fn linux_unlinkat(dirfd: isize, path: &str, flags: usize) -> isize {
    with_c_path(path, |ptr| {
        syscall(
            SYSCALL_UNLINKAT,
            [dirfd as usize, ptr as usize, flags, 0, 0, 0],
        )
    })
}

fn linux_mount(
    source: Option<&str>,
    target: &str,
    fs_type: Option<&str>,
    flags: usize,
    data: Option<&str>,
) -> isize {
    with_opt_c_path(source, |source_ptr| {
        with_c_path(target, |target_ptr| {
            with_opt_c_path(fs_type, |type_ptr| {
                with_opt_c_path(data, |data_ptr| {
                    syscall(
                        SYSCALL_MOUNT,
                        [
                            source_ptr as usize,
                            target_ptr as usize,
                            type_ptr as usize,
                            flags,
                            data_ptr as usize,
                            0,
                        ],
                    )
                })
            })
        })
    })
}

fn linux_unshare(flags: usize) -> isize {
    syscall(SYSCALL_UNSHARE, [flags, 0, 0, 0, 0, 0])
}

fn linux_setns(fd: isize, nstype: usize) -> isize {
    syscall(SYSCALL_SETNS, [fd as usize, nstype, 0, 0, 0, 0])
}

fn readlink(path: &str) -> String {
    let mut buf = [0u8; 256];
    let len = linux_readlinkat(AT_FDCWD, path, &mut buf);
    assert!(len >= 0);
    str::from_utf8(&buf[..len as usize]).unwrap().to_string()
}

fn read_all(path: &str) -> String {
    let fd = open(path, RDONLY);
    assert!(fd >= 0);
    let fd = fd as usize;

    let mut out = [0u8; 2048];
    let len = read(fd, &mut out);
    assert!(len >= 0);
    assert_eq!(close(fd), 0);
    str::from_utf8(&out[..len as usize]).unwrap().to_string()
}

fn assert_symlink_fd_target(fd: usize, expected_target: &str) {
    let mut buf = [0u8; 256];
    let len = linux_readlinkat(fd as isize, "", &mut buf);
    assert!(len >= 0);
    let target = str::from_utf8(&buf[..len as usize]).unwrap();
    assert_eq!(target, expected_target);
}

fn proc_mounts_contains_target(mounts: &str, target: &str) -> bool {
    mounts
        .lines()
        .filter_map(|line| line.split_whitespace().nth(1))
        .any(|entry| entry == target)
}

fn mkdir_unique(path: &str) {
    let rc = linux_mkdirat(AT_FDCWD, path, 0o755);
    assert!(rc == 0 || rc == EEXIST);
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let pid = getpid();
    let src = alloc::format!("/tmp/mount_ns_smoke_src_{}", pid);
    let dst = alloc::format!("/tmp/mount_ns_smoke_dst_{}", pid);

    mkdir_unique(&src);
    mkdir_unique(&dst);

    let old_target = readlink("/proc/self/ns/mnt");
    let old_ns_fd = open("/proc/self/ns/mnt", RDONLY);
    assert!(old_ns_fd >= 0);
    let old_ns_fd = old_ns_fd as usize;

    let mnt_link_fd = linux_openat(AT_FDCWD, "/proc/self/ns/mnt", O_PATH | O_NOFOLLOW, 0);
    assert!(mnt_link_fd >= 0);
    let mnt_link_fd = mnt_link_fd as usize;
    assert_symlink_fd_target(mnt_link_fd, &old_target);

    assert_eq!(linux_unshare(CLONE_NEWNS), 0);
    let new_target = readlink("/proc/self/ns/mnt");
    assert_ne!(new_target, old_target);
    assert_symlink_fd_target(mnt_link_fd, &new_target);

    assert_eq!(linux_mount(Some(&src), &dst, None, MS_BIND, None), 0);
    let mounts = read_all("/proc/self/mounts");
    assert!(proc_mounts_contains_target(&mounts, &dst));

    assert_eq!(linux_setns(old_ns_fd as isize, CLONE_NEWNS), 0);
    let restored_target = readlink("/proc/self/ns/mnt");
    assert_eq!(restored_target, old_target);
    assert_symlink_fd_target(mnt_link_fd, &restored_target);

    let restored_mounts = read_all("/proc/self/mounts");
    assert!(!proc_mounts_contains_target(&restored_mounts, &dst));

    assert_eq!(close(old_ns_fd), 0);
    assert_eq!(close(mnt_link_fd), 0);
    assert_eq!(linux_unlinkat(AT_FDCWD, &dst, AT_REMOVEDIR), 0);
    assert_eq!(linux_unlinkat(AT_FDCWD, &src, AT_REMOVEDIR), 0);

    println!("mount_namespace_smoke passed");
    0
}
