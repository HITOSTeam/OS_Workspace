#![no_std]
#![no_main]

extern crate alloc;

#[macro_use]
extern crate user;

use alloc::string::String;
use user::syscall::{chdir, fork, getpid, syscall, waitpid};

const SYSCALL_MKDIRAT: usize = 34;
const SYSCALL_UMOUNT2: usize = 39;
const SYSCALL_MOUNT: usize = 40;
const SYSCALL_READLINKAT: usize = 78;
const SYSCALL_WAITID: usize = 95;

const AT_FDCWD: isize = -100;
const P_PID: usize = 1;
const WEXITED: usize = 0x0000_0004;
const WNOWAIT: usize = 0x0100_0000;
const ENOENT: isize = -2;

fn with_c_path<T>(path: &str, f: impl FnOnce(*const u8) -> T) -> T {
    let mut owned = String::from(path);
    owned.push('\0');
    f(owned.as_ptr())
}

fn linux_mkdir(path: &str) -> isize {
    with_c_path(path, |path_ptr| {
        syscall(
            SYSCALL_MKDIRAT,
            [AT_FDCWD as usize, path_ptr as usize, 0o755, 0, 0, 0],
        )
    })
}

fn linux_mount_tmpfs(target: &str) -> isize {
    with_c_path("tmpfs", |source_ptr| {
        with_c_path(target, |target_ptr| {
            with_c_path("tmpfs", |type_ptr| {
                with_c_path("size=1m,mode=755", |data_ptr| {
                    syscall(
                        SYSCALL_MOUNT,
                        [
                            source_ptr as usize,
                            target_ptr as usize,
                            type_ptr as usize,
                            0,
                            data_ptr as usize,
                            0,
                        ],
                    )
                })
            })
        })
    })
}

fn linux_umount(target: &str) -> isize {
    with_c_path(target, |target_ptr| {
        syscall(SYSCALL_UMOUNT2, [target_ptr as usize, 0, 0, 0, 0, 0])
    })
}

fn waitid_zombie_without_reaping(pid: isize) -> isize {
    let mut info = [0u8; 128];
    syscall(
        SYSCALL_WAITID,
        [
            P_PID,
            pid as usize,
            info.as_mut_ptr() as usize,
            WEXITED | WNOWAIT,
            0,
            0,
        ],
    )
}

fn readlink(path: &str, output: &mut [u8]) -> isize {
    with_c_path(path, |path_ptr| {
        syscall(
            SYSCALL_READLINKAT,
            [
                AT_FDCWD as usize,
                path_ptr as usize,
                output.as_mut_ptr() as usize,
                output.len(),
                0,
                0,
            ],
        )
    })
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let root = alloc::format!("/tmp/vfs_exit_fs_smoke_{}", getpid());
    let mountpoint = alloc::format!("{root}/mnt");
    assert_eq!(linux_mkdir(&root), 0);
    assert_eq!(linux_mkdir(&mountpoint), 0);
    assert_eq!(linux_mount_tmpfs(&mountpoint), 0);

    let child = fork();
    if child == 0 {
        assert_eq!(chdir(&mountpoint), 0);
        return 0;
    }
    assert!(child > 0, "fork: {child}");

    // WNOWAIT guarantees that the child is already a visible zombie while
    // preserving its PCB. Linux has run exit_fs() by this point, so cwd must
    // no longer pin the tmpfs mount and /proc/<pid>/cwd must have no target.
    assert_eq!(waitid_zombie_without_reaping(child), 0);
    let proc_cwd = alloc::format!("/proc/{child}/cwd");
    let mut link = [0u8; 128];
    assert_eq!(readlink(&proc_cwd, &mut link), ENOENT);
    assert_eq!(linux_umount(&mountpoint), 0);

    let mut status = -1;
    assert_eq!(waitpid(child, &mut status), child);
    assert_eq!(status, 0);
    println!("VFS_EXIT_FS_ZOMBIE_PASS");
    0
}
