#![no_std]
#![no_main]

extern crate alloc;

extern crate user;

use alloc::{format, vec::Vec};
use core::arch::asm;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use user::syscall::{
    _yield, EPOLL_CTL_ADD, EPOLLIN, EpollEvent, RDONLY, close, epoll_create1, epoll_ctl,
    epoll_wait, execve, fork, getpid, open, pipe, read, syscall, waitpid,
};

static WAITER_ENTERED: AtomicBool = AtomicBool::new(false);
static EPOLL_FD: AtomicUsize = AtomicUsize::new(0);

const PAGE_SIZE: usize = 4096;
const CHILD_STACK_ADDR: usize = 0x31_2000_0000;
const CHILD_STACK_SIZE: usize = PAGE_SIZE * 16;

const SYSCALL_CLONE: usize = 220;
const SYSCALL_EXIT_GROUP: usize = 94;
const SYSCALL_MMAP: usize = 222;

const CLONE_VM: usize = 0x0000_0100;
const CLONE_SIGHAND: usize = 0x0000_0800;
const CLONE_THREAD: usize = 0x0001_0000;

const PROT_READ: usize = 1;
const PROT_WRITE: usize = 2;
const MAP_PRIVATE: usize = 0x02;
const MAP_FIXED: usize = 0x10;
const MAP_ANONYMOUS: usize = 0x20;

const TARGET: &str = "/user/basename.bin\0";
const ARG0: &str = "basename\0";
const ARG1: &str = "/exec-prepared-wait/PASS\0";

fn read_file(path: &str) -> Option<Vec<u8>> {
    let fd = open(path, RDONLY);
    if fd < 0 {
        return None;
    }
    let fd = fd as usize;
    let mut out = Vec::new();
    let mut buf = [0u8; 128];
    loop {
        let n = read(fd, &mut buf);
        if n <= 0 {
            break;
        }
        out.extend_from_slice(&buf[..n as usize]);
    }
    close(fd);
    Some(out)
}

fn task_state(pid: isize, tid: isize) -> Option<u8> {
    let stat = read_file(format!("/proc/{pid}/task/{tid}/stat").as_str())?;
    let stat = core::str::from_utf8(&stat).ok()?;
    let rparen = stat.rfind(')')?;
    stat.as_bytes().get(rparen + 2).copied()
}

fn wait_until_blocked(pid: isize, tid: isize) {
    for _ in 0..4096 {
        if WAITER_ENTERED.load(Ordering::Acquire) && task_state(pid, tid) == Some(b'S') {
            return;
        }
        _yield();
    }
    panic!("epoll peer did not block before teardown");
}

fn epoll_waiter(epfd: usize) -> ! {
    WAITER_ENTERED.store(true, Ordering::Release);
    let mut events = [EpollEvent::default(); 1];
    let ret = epoll_wait(epfd, &mut events, -1);
    panic!("epoll peer survived exec with return value {}", ret);
}

extern "C" fn child_entry() -> ! {
    epoll_waiter(EPOLL_FD.load(Ordering::Acquire));
}

fn mmap_child_stack() -> isize {
    syscall(
        SYSCALL_MMAP,
        [
            CHILD_STACK_ADDR,
            CHILD_STACK_SIZE,
            PROT_READ | PROT_WRITE,
            MAP_PRIVATE | MAP_FIXED | MAP_ANONYMOUS,
            usize::MAX,
            0,
        ],
    )
}

#[cfg(target_arch = "loongarch64")]
#[inline(never)]
fn clone_waiter(child_stack: usize) -> isize {
    let flags = CLONE_VM | CLONE_SIGHAND | CLONE_THREAD;
    let ret: isize;
    unsafe {
        asm!(
            "syscall 0",
            "bnez $r4, 2f",
            "b {child_entry}",
            "2:",
            inlateout("$r4") flags => ret,
            in("$r5") child_stack,
            in("$r6") 0usize,
            in("$r7") 0usize,
            in("$r8") 0usize,
            in("$r9") 0usize,
            in("$r11") SYSCALL_CLONE,
            child_entry = sym child_entry,
        );
    }
    ret
}

#[cfg(target_arch = "riscv64")]
#[inline(never)]
fn clone_waiter(child_stack: usize) -> isize {
    let flags = CLONE_VM | CLONE_SIGHAND | CLONE_THREAD;
    let ret: isize;
    unsafe {
        asm!(
            "ecall",
            "bnez a0, 2f",
            "j {child_entry}",
            "2:",
            inlateout("a0") flags => ret,
            in("a1") child_stack,
            in("a2") 0usize,
            in("a3") 0usize,
            in("a4") 0usize,
            in("a5") 0usize,
            in("a7") SYSCALL_CLONE,
            child_entry = sym child_entry,
        );
    }
    ret
}

fn spawn_blocked_waiter() {
    WAITER_ENTERED.store(false, Ordering::Release);
    let mut pipe_fd = [0usize; 2];
    assert_eq!(pipe(&mut pipe_fd), 0);

    let epfd = epoll_create1(0);
    assert!(epfd >= 0);
    let epfd = epfd as usize;
    let event = EpollEvent {
        events: EPOLLIN,
        data: 0x6578_6563,
    };
    assert_eq!(epoll_ctl(epfd, EPOLL_CTL_ADD, pipe_fd[0], Some(&event)), 0);

    assert_eq!(mmap_child_stack(), CHILD_STACK_ADDR as isize);
    EPOLL_FD.store(epfd, Ordering::Release);
    let tid = clone_waiter(CHILD_STACK_ADDR + CHILD_STACK_SIZE);
    assert!(tid > 0, "clone returned {}", tid);
    wait_until_blocked(getpid(), tid);
}

fn exit_group(exit_code: usize) -> ! {
    let ret = syscall(SYSCALL_EXIT_GROUP, [exit_code, 0, 0, 0, 0, 0]);
    panic!("exit_group returned {}", ret);
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    // First exercise the normal fatal-signal path used when a multithreaded
    // rustc returns from main. The parent wait also proves that process-level
    // retirement and notification complete after the epoll peer is killed.
    let child = fork();
    assert!(child >= 0, "fork returned {}", child);
    if child == 0 {
        spawn_blocked_waiter();
        exit_group(0);
    }
    let mut status = -1;
    assert_eq!(waitpid(child, &mut status), child);
    assert_eq!(status, 0);
    user::println!("EXIT_GROUP_PREPARED_WAIT_PASS");

    // Then reproduce rustup's de_thread path: exec must retire the same kind
    // of prepared epoll waiter without broadcasting SIGKILL to the new image.
    spawn_blocked_waiter();

    let args = [ARG0.as_ptr(), ARG1.as_ptr(), core::ptr::null()];
    let env = [core::ptr::null()];
    let ret = execve(TARGET, &args, &env);
    panic!("execve returned {}", ret);
}
