#![no_std]
#![no_main]

extern crate alloc;

#[macro_use]
extern crate user;

use user::syscall::{CREATE, RDONLY, RDWR, TRUNC, close, execve, exit, fork, open, openat, read};
use user::syscall::{syscall, waitpid, write};

const SYSCALL_UNLINKAT: usize = 35;
const AT_FDCWD: isize = -100;
const ETXTBSY: isize = -26;

const SRC: &str = "/user/basename.bin";
const TARGET: &str = "/tmp/exec_write_count_smoke.bin";
const TARGET_C: &str = "/tmp/exec_write_count_smoke.bin\0";
const ARG0: &str = "exec_write_count_smoke.bin\0";
const ARG1: &str = "ok\0";
const ARG2: &str = "extra\0";
const ARG3: &str = "fail\0";

fn unlinkat(path: &str) -> isize {
    let mut owned = alloc::string::String::from(path);
    owned.push('\0');
    syscall(
        SYSCALL_UNLINKAT,
        [AT_FDCWD as usize, owned.as_ptr() as usize, 0, 0, 0, 0],
    )
}

fn copy_exec_target() {
    let _ = unlinkat(TARGET);
    let src = open(SRC, RDONLY);
    assert!(src >= 0);
    let dst = openat(AT_FDCWD, TARGET, RDWR | CREATE | TRUNC, 0o755);
    assert!(dst >= 0);
    let src = src as usize;
    let dst = dst as usize;
    let mut buf = [0u8; 4096];
    loop {
        let n = read(src, &mut buf);
        assert!(n >= 0);
        if n == 0 {
            break;
        }
        let n = n as usize;
        assert_eq!(write(dst, &buf[..n]), n as isize);
    }
    assert_eq!(close(src), 0);
    assert_eq!(close(dst), 0);
}

fn wait_exit_zero(pid: isize) {
    let mut status = 0i32;
    assert_eq!(waitpid(pid, &mut status), pid);
    assert_eq!(status, 0);
}

fn expect_exec_etxtbsy(write_fd: usize) {
    let pid = fork();
    assert!(pid >= 0);
    if pid == 0 {
        assert_eq!(close(write_fd), 0);
        let args = [
            ARG0.as_ptr(),
            ARG1.as_ptr(),
            ARG2.as_ptr(),
            ARG3.as_ptr(),
            core::ptr::null(),
        ];
        let env = [core::ptr::null()];
        let ret = execve(TARGET_C, &args, &env);
        if ret == ETXTBSY {
            exit(0);
        }
        println!("exec while write-open returned {}", ret);
        exit(1);
    }
    wait_exit_zero(pid);
}

fn expect_exec_success() {
    let pid = fork();
    assert!(pid >= 0);
    if pid == 0 {
        let args = [ARG0.as_ptr(), ARG1.as_ptr(), core::ptr::null()];
        let env = [core::ptr::null()];
        let ret = execve(TARGET_C, &args, &env);
        println!("exec after close returned {}", ret);
        exit(1);
    }
    wait_exit_zero(pid);
}

fn expect_two_independent_writers() {
    let fd1 = open(TARGET, RDWR);
    assert!(fd1 >= 0);
    let fd2 = open(TARGET, RDWR);
    assert!(fd2 >= 0);

    assert_eq!(close(fd1 as usize), 0);
    expect_exec_etxtbsy(fd2 as usize);
    assert_eq!(close(fd2 as usize), 0);
    expect_exec_success();
}

fn expect_exec_after_writer_exit() {
    let pid = fork();
    assert!(pid >= 0);
    if pid == 0 {
        let fd = open(TARGET, RDWR);
        assert!(fd >= 0);
        exit(0);
    }
    wait_exit_zero(pid);
    expect_exec_success();
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    copy_exec_target();

    let write_fd = open(TARGET, RDWR);
    assert!(write_fd >= 0);
    expect_exec_etxtbsy(write_fd as usize);
    expect_exec_etxtbsy(write_fd as usize);
    assert_eq!(close(write_fd as usize), 0);

    expect_two_independent_writers();
    expect_exec_after_writer_exit();
    expect_exec_success();
    let _ = unlinkat(TARGET);
    println!("exec_write_count_smoke passed");
    0
}
