#![no_std]
#![no_main]

extern crate alloc;
extern crate user;

use alloc::{format, vec::Vec};
use core::arch::asm;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use user::syscall::{
    _yield, RDONLY, close, execve, exit, fork, getpid, open, read, syscall, waitpid,
};

const PAGE_SIZE: usize = 4096;
const CHILD_STACK_ADDR: usize = 0x31_8000_0000;
const CHILD_STACK_SIZE: usize = PAGE_SIZE * 16;
const ITERATIONS: usize = 16;

const SYSCALL_EXIT_GROUP: usize = 94;
const SYSCALL_SOCKETPAIR: usize = 199;
const SYSCALL_RECVFROM: usize = 207;
const SYSCALL_CLONE: usize = 220;
const SYSCALL_MMAP: usize = 222;

const AF_UNIX: usize = 1;
const SOCK_STREAM: usize = 1;
const SOCK_SEQPACKET: usize = 5;
const SOCK_CLOEXEC: usize = 0x80000;

const EXEC_TARGET: &str = "/user/basename.bin\0";
const EXEC_ARG0: &str = "basename.bin\0";
const EXEC_ARG1: &str = "/tmp/socketpair-cloexec\0";

const CLONE_VM: usize = 0x0000_0100;
const CLONE_FS: usize = 0x0000_0200;
const CLONE_FILES: usize = 0x0000_0400;
const CLONE_SIGHAND: usize = 0x0000_0800;
const CLONE_THREAD: usize = 0x0001_0000;

const PROT_READ: usize = 1;
const PROT_WRITE: usize = 2;
const MAP_PRIVATE: usize = 0x02;
const MAP_FIXED: usize = 0x10;
const MAP_ANONYMOUS: usize = 0x20;

static RECV_FD: AtomicUsize = AtomicUsize::new(usize::MAX);
static RECV_ENTERED: AtomicBool = AtomicBool::new(false);

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

fn proc_state(path: &str) -> Option<u8> {
    let stat = read_file(path)?;
    let stat = core::str::from_utf8(&stat).ok()?;
    let rparen = stat.rfind(')')?;
    stat.as_bytes().get(rparen + 2).copied()
}

fn wait_until_task_blocked(pid: isize, tid: isize) {
    let path = format!("/proc/{pid}/task/{tid}/stat");
    for _ in 0..16_384 {
        if RECV_ENTERED.load(Ordering::Acquire) && proc_state(path.as_str()) == Some(b'S') {
            return;
        }
        _yield();
    }
    panic!("socketpair recv thread did not block");
}

fn wait_until_process_zombie(pid: isize) {
    let path = format!("/proc/{pid}/stat");
    for _ in 0..65_536 {
        if proc_state(path.as_str()) == Some(b'Z') {
            return;
        }
        _yield();
    }
    panic!("socketpair child did not become a zombie");
}

extern "C" fn recv_worker_entry() -> ! {
    let fd = RECV_FD.load(Ordering::Acquire);
    RECV_ENTERED.store(true, Ordering::Release);
    let mut byte = [0u8; 1];
    let ret = syscall(
        SYSCALL_RECVFROM,
        [fd, byte.as_mut_ptr() as usize, byte.len(), 0, 0, 0],
    );
    panic!("recv worker survived exit_group with ret={ret}");
}

#[cfg(target_arch = "loongarch64")]
#[inline(never)]
fn clone_recv_worker(child_stack: usize) -> isize {
    let flags = CLONE_VM | CLONE_FS | CLONE_FILES | CLONE_SIGHAND | CLONE_THREAD;
    let ret: isize;
    unsafe {
        asm!(
            "syscall 0",
            "bnez $r4, 2f",
            "b {recv_worker_entry}",
            "2:",
            inlateout("$r4") flags => ret,
            in("$r5") child_stack,
            in("$r6") 0usize,
            in("$r7") 0usize,
            in("$r8") 0usize,
            in("$r9") 0usize,
            in("$r11") SYSCALL_CLONE,
            recv_worker_entry = sym recv_worker_entry,
        );
    }
    ret
}

#[cfg(target_arch = "riscv64")]
#[inline(never)]
fn clone_recv_worker(child_stack: usize) -> isize {
    let flags = CLONE_VM | CLONE_FS | CLONE_FILES | CLONE_SIGHAND | CLONE_THREAD;
    let ret: isize;
    unsafe {
        asm!(
            "ecall",
            "bnez a0, 2f",
            "j {recv_worker_entry}",
            "2:",
            inlateout("a0") flags => ret,
            in("a1") child_stack,
            in("a2") 0usize,
            in("a3") 0usize,
            in("a4") 0usize,
            in("a5") 0usize,
            in("a7") SYSCALL_CLONE,
            recv_worker_entry = sym recv_worker_entry,
        );
    }
    ret
}

fn mmap_child_stack() {
    assert_eq!(
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
        ),
        CHILD_STACK_ADDR as isize
    );
}

fn exit_group(code: usize) -> ! {
    let ret = syscall(SYSCALL_EXIT_GROUP, [code, 0, 0, 0, 0, 0]);
    panic!("exit_group returned {ret}");
}

fn socketpair(socket_type: usize) -> [usize; 2] {
    let mut fds = [-1i32; 2];
    assert_eq!(
        syscall(
            SYSCALL_SOCKETPAIR,
            [AF_UNIX, socket_type, 0, fds.as_mut_ptr() as usize, 0, 0,],
        ),
        0
    );
    assert!(fds[0] >= 0 && fds[1] >= 0);
    [fds[0] as usize, fds[1] as usize]
}

fn exercise_one(iteration: usize) {
    let fds = socketpair(SOCK_STREAM);
    let child = fork();
    assert!(child >= 0, "fork {iteration} returned {child}");
    if child == 0 {
        close(fds[0]);
        mmap_child_stack();
        RECV_FD.store(fds[1], Ordering::Release);
        RECV_ENTERED.store(false, Ordering::Release);
        let tid = clone_recv_worker(CHILD_STACK_ADDR + CHILD_STACK_SIZE);
        assert!(tid > 0, "clone recv worker returned {tid}");
        wait_until_task_blocked(getpid(), tid);
        exit_group(0);
    }

    close(fds[1]);
    wait_until_process_zombie(child);

    // Linux reports EOF as soon as the last descriptor owning the peer is
    // closed during exit_files(), even if zombie task objects are not reaped.
    let mut byte = [0u8; 1];
    assert_eq!(
        syscall(
            SYSCALL_RECVFROM,
            [fds[0], byte.as_mut_ptr() as usize, byte.len(), 0, 0, 0,],
        ),
        0,
        "recvfrom did not observe peer EOF at iteration {iteration}"
    );
    close(fds[0]);

    let mut status = -1;
    assert_eq!(waitpid(child, &mut status), child);
    assert_eq!(status, 0);
}

fn exercise_seqpacket_cloexec(iteration: usize) {
    let fds = socketpair(SOCK_SEQPACKET | SOCK_CLOEXEC);
    let child = fork();
    assert!(child >= 0, "seqpacket fork {iteration} returned {child}");
    if child == 0 {
        close(fds[0]);
        let args = [EXEC_ARG0.as_ptr(), EXEC_ARG1.as_ptr(), core::ptr::null()];
        let env = [core::ptr::null()];
        let ret = execve(EXEC_TARGET, &args, &env);
        panic!("seqpacket execve returned {ret} at iteration {iteration}");
    }

    // This is the channel used by glibc's clone/exec fallback and therefore by
    // Cargo when it spawns rustc: the parent owns the receive endpoint, while
    // successful exec closes the child's SOCK_CLOEXEC endpoint. Linux then
    // returns EOF from recvfrom before the child is reaped.
    close(fds[1]);
    let mut byte = [0u8; 1];
    assert_eq!(
        syscall(
            SYSCALL_RECVFROM,
            [fds[0], byte.as_mut_ptr() as usize, byte.len(), 0, 0, 0,],
        ),
        0,
        "seqpacket recvfrom did not observe CLOEXEC EOF at iteration {iteration}"
    );
    close(fds[0]);

    let mut status = -1;
    assert_eq!(waitpid(child, &mut status), child);
    assert_eq!(status, 0);
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    for iteration in 0..ITERATIONS {
        exercise_one(iteration);
    }
    for iteration in 0..ITERATIONS {
        exercise_seqpacket_cloexec(iteration);
    }
    user::println!(
        "SOCKETPAIR_EXIT_EOF_PASS stream_iterations={} seqpacket_cloexec_iterations={}",
        ITERATIONS,
        ITERATIONS
    );
    exit(0);
}
