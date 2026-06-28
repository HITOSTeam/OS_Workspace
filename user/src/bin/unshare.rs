#![no_std]
#![no_main]

extern crate alloc;

#[macro_use]
extern crate user;

use alloc::{format, string::String, vec::Vec};
use user::syscall::{RDONLY, WRONLY, close, execve, open, syscall, write};

const SYSCALL_GETUID: usize = 174;
const SYSCALL_GETGID: usize = 176;
const SYSCALL_MOUNT: usize = 40;
const SYSCALL_UNSHARE: usize = 97;

const CLONE_NEWNS: usize = 0x0002_0000;
const CLONE_NEWUSER: usize = 0x1000_0000;

const MS_REC: usize = 1 << 14;
const MS_PRIVATE: usize = 1 << 18;
const MS_SLAVE: usize = 1 << 19;
const MS_SHARED: usize = 1 << 20;

fn linux_getuid() -> isize {
    syscall(SYSCALL_GETUID, [0, 0, 0, 0, 0, 0])
}

fn linux_getgid() -> isize {
    syscall(SYSCALL_GETGID, [0, 0, 0, 0, 0, 0])
}

fn linux_unshare(flags: usize) -> isize {
    syscall(SYSCALL_UNSHARE, [flags, 0, 0, 0, 0, 0])
}

fn with_c_path<T>(path: &str, f: impl FnOnce(*const u8) -> T) -> T {
    let mut owned = String::from(path);
    owned.push('\0');
    f(owned.as_ptr())
}

fn linux_mount_propagation(flags: usize) -> isize {
    with_c_path("none", |source| {
        with_c_path("/", |target| {
            with_c_path("none", |fstype| {
                syscall(
                    SYSCALL_MOUNT,
                    [
                        source as usize,
                        target as usize,
                        fstype as usize,
                        flags,
                        0,
                        0,
                    ],
                )
            })
        })
    })
}

fn write_all(path: &str, data: &[u8]) -> bool {
    let fd = open(path, WRONLY);
    if fd < 0 {
        return false;
    }
    let fd = fd as usize;
    let mut written = 0;
    while written < data.len() {
        let n = write(fd, &data[written..]);
        if n <= 0 {
            let _ = close(fd);
            return false;
        }
        written += n as usize;
    }
    close(fd) == 0
}

fn path_exists(path: &str) -> bool {
    let fd = open(path, RDONLY);
    if fd < 0 {
        return false;
    }
    let _ = close(fd as usize);
    true
}

fn resolve_command(cmd: &str) -> Option<String> {
    if cmd.contains('/') {
        return path_exists(cmd).then(|| String::from(cmd));
    }
    for prefix in [
        "/extra/bin/",
        "/user/",
        "/bin/",
        "/usr/bin/",
        "/musl/ltp/testcases/bin/",
        "/musl/",
        "/glibc/ltp/testcases/bin/",
        "/glibc/",
    ] {
        let candidate = format!("{prefix}{cmd}");
        if path_exists(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn exec_command(argv: &[&str]) -> i32 {
    if argv.is_empty() {
        println!("unshare: no command specified");
        return 1;
    }
    let Some(mut path) = resolve_command(argv[0]) else {
        println!("unshare: command not found: {}", argv[0]);
        return 127;
    };
    path.push('\0');

    let mut owned_args: Vec<String> = Vec::new();
    for arg in argv {
        let mut s = String::from(*arg);
        s.push('\0');
        owned_args.push(s);
    }
    let mut args: Vec<*const u8> = Vec::with_capacity(owned_args.len() + 1);
    for arg in &owned_args {
        args.push(arg.as_ptr());
    }
    args.push(core::ptr::null());
    let envs = [core::ptr::null()];
    execve(path.as_str(), &args, &envs);
    println!("unshare: exec failed: {}", argv[0]);
    127
}

fn map_root_user(parent_uid: isize, parent_gid: isize) -> bool {
    if parent_uid < 0 || parent_gid < 0 {
        return false;
    }
    let uid_map = format!("0 {} 1\n", parent_uid as usize);
    let gid_map = format!("0 {} 1\n", parent_gid as usize);
    write_all("/proc/self/uid_map", uid_map.as_bytes())
        && write_all("/proc/self/setgroups", b"deny\n")
        && write_all("/proc/self/gid_map", gid_map.as_bytes())
}

#[unsafe(no_mangle)]
pub fn main(argc: usize, argv: &[&str]) -> i32 {
    let mut want_user = false;
    let mut want_map_root = false;
    let mut want_mount = false;
    let mut propagation: Option<&str> = None;

    let mut idx = 1;
    while idx < argc {
        match argv[idx] {
            "--user" | "-U" => want_user = true,
            "--map-root-user" | "-r" => {
                want_user = true;
                want_map_root = true;
            }
            "--mount" | "-m" => want_mount = true,
            "--propagation" => {
                idx += 1;
                if idx >= argc {
                    println!("unshare: option '--propagation' requires an argument");
                    return 1;
                }
                propagation = Some(argv[idx]);
            }
            "--" => {
                idx += 1;
                break;
            }
            opt if opt.starts_with('-') => {
                println!("unshare: unsupported option {}", opt);
                return 1;
            }
            _ => break,
        }
        idx += 1;
    }

    let parent_uid = linux_getuid();
    let parent_gid = linux_getgid();
    if want_user && linux_unshare(CLONE_NEWUSER) < 0 {
        println!("unshare: unshare user namespace failed");
        return 1;
    }
    if want_map_root && !map_root_user(parent_uid, parent_gid) {
        println!("unshare: failed to map root user");
        return 1;
    }
    if want_mount && linux_unshare(CLONE_NEWNS) < 0 {
        println!("unshare: unshare mount namespace failed");
        return 1;
    }
    if want_mount {
        let prop_flag = match propagation.unwrap_or("private") {
            "private" => MS_PRIVATE,
            "shared" if want_user => MS_SLAVE,
            "shared" => MS_SHARED,
            "slave" => MS_SLAVE,
            other => {
                println!("unshare: unsupported propagation {}", other);
                return 1;
            }
        };
        if linux_mount_propagation(MS_REC | prop_flag) < 0 {
            println!("unshare: failed to set mount propagation");
            return 1;
        }
    }

    exec_command(&argv[idx..argc])
}
