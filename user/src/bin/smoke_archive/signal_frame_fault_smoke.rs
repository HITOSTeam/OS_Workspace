#![no_std]
#![no_main]

#[macro_use]
extern crate user;

use core::sync::atomic::{AtomicUsize, Ordering};
use user::syscall::{fork, getpid, kill, syscall, waitpid};

const PAGE_SIZE: usize = 4096;
const ALTSTACK_SIZE: usize = PAGE_SIZE * 16;
const ITERATIONS: usize = 64;

const SYSCALL_RT_SIGACTION: usize = 134;
const SYSCALL_SIGALTSTACK: usize = 132;
const SYSCALL_MUNMAP: usize = 215;
const SYSCALL_MMAP: usize = 222;

const SIGUSR1: i32 = 10;
const SA_SIGINFO: usize = 0x0000_0004;
const SA_ONSTACK: usize = 0x0800_0000;

const PROT_READ: usize = 1;
const PROT_WRITE: usize = 2;
const MAP_PRIVATE: usize = 0x02;
const MAP_ANONYMOUS: usize = 0x20;

static SIGNAL_COUNT: AtomicUsize = AtomicUsize::new(0);

#[repr(C)]
#[derive(Clone, Copy)]
struct RtSigAction {
    handler: usize,
    flags: usize,
    restorer: usize,
    mask: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct SigStack {
    ss_sp: usize,
    ss_flags: i32,
    _pad: i32,
    ss_size: usize,
}

fn signal_handler(_signum: usize, _info: usize, _ucontext: usize) {
    SIGNAL_COUNT.fetch_add(1, Ordering::SeqCst);
}

fn mmap_private_anon(len: usize) -> isize {
    syscall(
        SYSCALL_MMAP,
        [
            0,
            len,
            PROT_READ | PROT_WRITE,
            MAP_PRIVATE | MAP_ANONYMOUS,
            usize::MAX,
            0,
        ],
    )
}

fn install_handler() -> isize {
    let action = RtSigAction {
        handler: signal_handler as *const () as usize,
        flags: SA_SIGINFO | SA_ONSTACK,
        restorer: 0,
        mask: 0,
    };
    syscall(
        SYSCALL_RT_SIGACTION,
        [
            SIGUSR1 as usize,
            &action as *const RtSigAction as usize,
            0,
            core::mem::size_of::<u64>(),
            0,
            0,
        ],
    )
}

fn set_altstack(base: usize) -> isize {
    let stack = SigStack {
        ss_sp: base,
        ss_flags: 0,
        _pad: 0,
        ss_size: ALTSTACK_SIZE,
    };
    syscall(
        SYSCALL_SIGALTSTACK,
        [&stack as *const SigStack as usize, 0, 0, 0, 0, 0],
    )
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let mapped = mmap_private_anon(ALTSTACK_SIZE);
    assert!(mapped > 0);
    let altstack = mapped as usize;
    assert_eq!(install_handler(), 0);

    for iteration in 0..ITERATIONS {
        let child = fork();
        assert!(child >= 0);
        if child == 0 {
            SIGNAL_COUNT.store(0, Ordering::SeqCst);
            assert_eq!(set_altstack(altstack), 0);

            // The mmap is intentionally untouched before this signal.  With
            // SA_ONSTACK | SA_SIGINFO, kernel frame construction must resolve
            // lazy/COW pages without holding the task-state spin lock.
            assert_eq!(kill(getpid() as usize, SIGUSR1), 0);
            return if SIGNAL_COUNT.load(Ordering::SeqCst) == 1 {
                0
            } else {
                10
            };
        }

        let mut status = -1;
        assert_eq!(waitpid(child, &mut status), child);
        if status != 0 {
            println!(
                "signal_frame_fault_smoke failed iteration={} status={}",
                iteration, status
            );
            return 1;
        }
    }

    assert_eq!(
        syscall(SYSCALL_MUNMAP, [altstack, ALTSTACK_SIZE, 0, 0, 0, 0]),
        0
    );
    println!("SIGNAL_FRAME_FAULT_PASS iterations={}", ITERATIONS);
    0
}
