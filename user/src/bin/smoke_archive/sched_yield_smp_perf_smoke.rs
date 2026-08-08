#![no_std]
#![no_main]

extern crate user;

use core::arch::asm;
use core::sync::atomic::{AtomicUsize, Ordering};
use user::syscall::{_yield, exit, syscall};

const PAGE_SIZE: usize = 4096;
const WORKERS: usize = 12;
const YIELDS_PER_WORKER: usize = 4096;
const STACK_SIZE: usize = PAGE_SIZE * 16;
const STACKS_ADDR: usize = 0x31_8000_0000;

const SYSCALL_CLONE: usize = 220;
const SYSCALL_CLOCK_GETTIME: usize = 113;
const SYSCALL_EXIT_GROUP: usize = 94;
const SYSCALL_MMAP: usize = 222;

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

static READY: AtomicUsize = AtomicUsize::new(0);
static START: AtomicUsize = AtomicUsize::new(0);
static DONE: AtomicUsize = AtomicUsize::new(0);

#[repr(C)]
#[derive(Default)]
struct Timespec {
    sec: i64,
    nsec: i64,
}

fn monotonic_ns() -> u64 {
    let mut now = Timespec::default();
    assert_eq!(
        syscall(
            SYSCALL_CLOCK_GETTIME,
            [1, &mut now as *mut Timespec as usize, 0, 0, 0, 0],
        ),
        0
    );
    (now.sec as u64)
        .saturating_mul(1_000_000_000)
        .saturating_add(now.nsec as u64)
}

extern "C" fn worker_entry() -> ! {
    READY.fetch_add(1, Ordering::Release);
    while START.load(Ordering::Acquire) == 0 {
        _yield();
    }
    for _ in 0..YIELDS_PER_WORKER {
        _yield();
    }
    DONE.fetch_add(1, Ordering::Release);
    exit(0);
}

#[cfg(target_arch = "loongarch64")]
#[inline(never)]
fn clone_worker(child_stack: usize) -> isize {
    let flags = CLONE_VM | CLONE_FS | CLONE_FILES | CLONE_SIGHAND | CLONE_THREAD;
    let ret: isize;
    unsafe {
        asm!(
            "syscall 0",
            "bnez $r4, 2f",
            "b {worker_entry}",
            "2:",
            inlateout("$r4") flags => ret,
            in("$r5") child_stack,
            in("$r6") 0usize,
            in("$r7") 0usize,
            in("$r8") 0usize,
            in("$r9") 0usize,
            in("$r11") SYSCALL_CLONE,
            worker_entry = sym worker_entry,
        );
    }
    ret
}

#[cfg(target_arch = "riscv64")]
#[inline(never)]
fn clone_worker(child_stack: usize) -> isize {
    let flags = CLONE_VM | CLONE_FS | CLONE_FILES | CLONE_SIGHAND | CLONE_THREAD;
    let ret: isize;
    unsafe {
        asm!(
            "ecall",
            "bnez a0, 2f",
            "j {worker_entry}",
            "2:",
            inlateout("a0") flags => ret,
            in("a1") child_stack,
            in("a2") 0usize,
            in("a3") 0usize,
            in("a4") 0usize,
            in("a5") 0usize,
            in("a7") SYSCALL_CLONE,
            worker_entry = sym worker_entry,
        );
    }
    ret
}

fn exit_group(code: usize) -> ! {
    let ret = syscall(SYSCALL_EXIT_GROUP, [code, 0, 0, 0, 0, 0]);
    panic!("exit_group returned {ret}");
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let stacks_len = WORKERS * STACK_SIZE;
    assert_eq!(
        syscall(
            SYSCALL_MMAP,
            [
                STACKS_ADDR,
                stacks_len,
                PROT_READ | PROT_WRITE,
                MAP_PRIVATE | MAP_FIXED | MAP_ANONYMOUS,
                usize::MAX,
                0,
            ],
        ),
        STACKS_ADDR as isize
    );

    for worker in 0..WORKERS {
        let stack_top = STACKS_ADDR + (worker + 1) * STACK_SIZE;
        let tid = clone_worker(stack_top);
        assert!(tid > 0, "clone worker {worker} failed: {tid}");
    }
    while READY.load(Ordering::Acquire) != WORKERS {
        _yield();
    }

    let start_ns = monotonic_ns();
    START.store(1, Ordering::Release);
    while DONE.load(Ordering::Acquire) != WORKERS {
        _yield();
    }
    let elapsed_us = monotonic_ns().saturating_sub(start_ns) / 1_000;
    user::println!(
        "SCHED_YIELD_SMP_PERF workers={} yields_per_worker={} elapsed_us={}",
        WORKERS,
        YIELDS_PER_WORKER,
        elapsed_us
    );
    exit_group(0);
}
