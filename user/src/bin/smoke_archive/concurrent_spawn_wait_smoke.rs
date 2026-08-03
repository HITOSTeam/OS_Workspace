#![no_std]
#![no_main]

extern crate user;

use core::arch::asm;
use core::sync::atomic::{AtomicUsize, Ordering};
use user::syscall::{_yield, exit, fork, syscall, waitpid};

const PAGE_SIZE: usize = 4096;
const WORKERS: usize = 8;
const ITERATIONS: usize = 32;
const STACK_SIZE: usize = PAGE_SIZE * 16;
const STACKS_ADDR: usize = 0x31_6000_0000;

const SYSCALL_CLONE: usize = 220;
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

static NEXT_WORKER: AtomicUsize = AtomicUsize::new(0);
static READY: AtomicUsize = AtomicUsize::new(0);
static START: AtomicUsize = AtomicUsize::new(0);
static DONE: AtomicUsize = AtomicUsize::new(0);
static FIRST_ERROR: AtomicUsize = AtomicUsize::new(0);

fn record_error(worker: usize, iteration: usize, stage: usize) {
    let encoded = 1 + worker * ITERATIONS * 4 + iteration * 4 + stage;
    let _ = FIRST_ERROR.compare_exchange(0, encoded, Ordering::AcqRel, Ordering::Acquire);
}

fn worker_body() -> ! {
    let worker = NEXT_WORKER.fetch_add(1, Ordering::AcqRel);
    READY.fetch_add(1, Ordering::Release);
    while START.load(Ordering::Acquire) == 0 {
        _yield();
    }

    for iteration in 0..ITERATIONS {
        let child = fork();
        if child == 0 {
            exit(0);
        }
        if child < 0 {
            record_error(worker, iteration, 0);
            break;
        }
        let mut status = -1;
        if waitpid(child, &mut status) != child {
            record_error(worker, iteration, 1);
            break;
        }
        if status != 0 {
            record_error(worker, iteration, 2);
            break;
        }
    }

    DONE.fetch_add(1, Ordering::Release);
    exit(0);
}

extern "C" fn worker_entry() -> ! {
    worker_body()
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
    START.store(1, Ordering::Release);
    while DONE.load(Ordering::Acquire) != WORKERS {
        _yield();
    }

    let error = FIRST_ERROR.load(Ordering::Acquire);
    if error != 0 {
        user::println!("CONCURRENT_SPAWN_WAIT_FAIL encoded={}", error);
        exit_group(1);
    }
    user::println!(
        "CONCURRENT_SPAWN_WAIT_PASS workers={} iterations={} children={}",
        WORKERS,
        ITERATIONS,
        WORKERS * ITERATIONS
    );
    exit_group(0);
}
