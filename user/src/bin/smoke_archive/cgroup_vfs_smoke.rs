#![no_std]
#![no_main]

extern crate alloc;

#[macro_use]
extern crate user;

use alloc::string::String;
use user::syscall::{RDONLY, close, getpid, open, pipe, read, syscall, waitpid, write};

const SYSCALL_MKDIRAT: usize = 34;
const SYSCALL_UMOUNT2: usize = 39;
const SYSCALL_MOUNT: usize = 40;
const SYSCALL_OPENAT: usize = 56;
const SYSCALL_CLONE3: usize = 435;

const AT_FDCWD: isize = -100;
const O_DIRECTORY: usize = 0x10000;
const MS_BIND: usize = 0x1000;
const CLONE_INTO_CGROUP: u64 = 1 << 33;
const SIGCHLD: u64 = 17;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct CloneArgs {
    flags: u64,
    pidfd: u64,
    child_tid: u64,
    parent_tid: u64,
    exit_signal: u64,
    stack: u64,
    stack_size: u64,
    tls: u64,
    set_tid: u64,
    set_tid_size: u64,
    cgroup: u64,
}

fn with_c_path<T>(path: &str, f: impl FnOnce(*const u8) -> T) -> T {
    let mut owned = String::from(path);
    owned.push('\0');
    f(owned.as_ptr())
}

fn with_opt_c_path<T>(path: Option<&str>, f: impl FnOnce(*const u8) -> T) -> T {
    match path {
        Some(path) => with_c_path(path, f),
        None => f(core::ptr::null()),
    }
}

fn linux_mkdir(path: &str) -> isize {
    with_c_path(path, |path_ptr| {
        syscall(
            SYSCALL_MKDIRAT,
            [AT_FDCWD as usize, path_ptr as usize, 0o755, 0, 0, 0],
        )
    })
}

fn linux_open_dir(path: &str) -> isize {
    with_c_path(path, |path_ptr| {
        syscall(
            SYSCALL_OPENAT,
            [
                AT_FDCWD as usize,
                path_ptr as usize,
                (RDONLY | O_DIRECTORY) as usize,
                0,
                0,
                0,
            ],
        )
    })
}

fn linux_mount(source: Option<&str>, target: &str, fs_type: Option<&str>, flags: usize) -> isize {
    with_opt_c_path(source, |source_ptr| {
        with_c_path(target, |target_ptr| {
            with_opt_c_path(fs_type, |type_ptr| {
                syscall(
                    SYSCALL_MOUNT,
                    [
                        source_ptr as usize,
                        target_ptr as usize,
                        type_ptr as usize,
                        flags,
                        0,
                        0,
                    ],
                )
            })
        })
    })
}

fn linux_umount(target: &str) -> isize {
    with_c_path(target, |target_ptr| {
        syscall(SYSCALL_UMOUNT2, [target_ptr as usize, 0, 0, 0, 0, 0])
    })
}

fn linux_clone3(args: &CloneArgs) -> isize {
    syscall(
        SYSCALL_CLONE3,
        [
            args as *const CloneArgs as usize,
            core::mem::size_of::<CloneArgs>(),
            0,
            0,
            0,
            0,
        ],
    )
}

fn child_is_in_alpha() -> bool {
    let fd = open("/proc/self/cgroup", RDONLY);
    if fd < 0 {
        return false;
    }
    let mut buf = [0u8; 128];
    let len = read(fd as usize, &mut buf);
    let _ = close(fd as usize);
    if len <= 0 {
        return false;
    }
    buf[..len as usize]
        .windows(b"0::/alpha\n".len())
        .any(|window| window == b"0::/alpha\n")
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let root = alloc::format!("/tmp/cgroup_vfs_smoke_{}", getpid());
    let mountpoint = alloc::format!("{root}/hierarchy");
    let alpha = alloc::format!("{mountpoint}/alpha");
    let bind = alloc::format!("{root}/bind-alpha");
    assert_eq!(linux_mkdir(&root), 0);
    assert_eq!(linux_mkdir(&mountpoint), 0);
    assert_eq!(linux_mkdir(&bind), 0);
    assert_eq!(
        linux_mount(Some("none"), &mountpoint, Some("cgroup2"), 0),
        0
    );
    assert_eq!(linux_mkdir(&alpha), 0);
    assert_eq!(linux_mount(Some(&alpha), &bind, None, MS_BIND), 0);

    // The bind keeps the hierarchy and subroot alive after the presentation
    // path used for the original mount has disappeared.
    assert_eq!(linux_umount(&mountpoint), 0);
    let cgroup_fd = linux_open_dir(&bind);
    assert!(cgroup_fd >= 0);
    let control_fd = open(&alloc::format!("{bind}/cgroup.procs"), RDONLY);
    assert!(control_fd >= 0);
    assert_eq!(close(control_fd as usize), 0);

    let mut pipefd = [0usize; 2];
    assert_eq!(pipe(&mut pipefd), 0);
    let args = CloneArgs {
        flags: CLONE_INTO_CGROUP,
        exit_signal: SIGCHLD,
        cgroup: cgroup_fd as u64,
        ..CloneArgs::default()
    };
    let child = linux_clone3(&args);
    if child == 0 {
        let _ = close(pipefd[0]);
        let status = [u8::from(child_is_in_alpha())];
        let _ = write(pipefd[1], &status);
        let _ = close(pipefd[1]);
        return if status[0] == 1 { 0 } else { 1 };
    }
    assert!(child > 0, "clone3(CLONE_INTO_CGROUP): {child}");
    assert_eq!(close(pipefd[1]), 0);
    let mut status = [0u8; 1];
    assert_eq!(read(pipefd[0], &mut status), 1);
    assert_eq!(status[0], 1);
    assert_eq!(close(pipefd[0]), 0);
    let mut exit_code = -1;
    assert_eq!(waitpid(child, &mut exit_code), child);
    assert_eq!(exit_code, 0);

    assert_eq!(close(cgroup_fd as usize), 0);
    assert_eq!(linux_umount(&bind), 0);
    println!("CGROUP_VFS_BIND_CLONE_PASS");
    0
}
