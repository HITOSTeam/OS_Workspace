#![no_std]
#![no_main]

#[macro_use]
extern crate user;

use user::syscall::{SIGSEGV, exit, fork, syscall, waitpid};

const PAGE_SIZE: usize = 4096;
const GROW_START: usize = 0x35_0000_0000;
const USER_STACK_GUARD_PAGES: usize = 256;
const GUARD_MID_PAGE: usize = USER_STACK_GUARD_PAGES / 2;

const SYSCALL_MMAP: usize = 222;
const SYSCALL_MUNMAP: usize = 215;

const PROT_READ: usize = 1;
const PROT_WRITE: usize = 2;
const MAP_PRIVATE: usize = 0x02;
const MAP_FIXED_NOREPLACE: usize = 0x100000;
const MAP_ANONYMOUS: usize = 0x20;
const MAP_GROWSDOWN: usize = 0x0100;

fn mmap_anon(addr: usize, len: usize, flags: usize) -> isize {
    syscall(
        SYSCALL_MMAP,
        [
            addr,
            len,
            PROT_READ | PROT_WRITE,
            MAP_PRIVATE | MAP_ANONYMOUS | flags,
            usize::MAX,
            0,
        ],
    )
}

fn munmap(addr: usize, len: usize) -> isize {
    syscall(SYSCALL_MUNMAP, [addr, len, 0, 0, 0, 0])
}

fn wait_termsig(status: i32) -> i32 {
    status & 0x7f
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    assert_eq!(
        mmap_anon(GROW_START, PAGE_SIZE, MAP_FIXED_NOREPLACE | MAP_GROWSDOWN),
        GROW_START as isize
    );

    let child = fork();
    assert!(child >= 0);
    if child == 0 {
        // SAFETY: this faults the page immediately below the grow-down VMA.
        // With no lower blocker, MAP_GROWSDOWN should expand and resolve it.
        unsafe {
            ((GROW_START - 1) as *mut u8).write_volatile(0x33);
        }
        exit(0);
    }

    let mut status = 0i32;
    assert_eq!(waitpid(child, &mut status), child);
    assert_eq!(status, 0);

    let guard_hint = GROW_START - PAGE_SIZE * GUARD_MID_PAGE;
    let hinted = mmap_anon(guard_hint, PAGE_SIZE, 0);
    assert!(hinted > 0);
    assert_ne!(hinted as usize, guard_hint);
    assert_eq!(munmap(hinted as usize, PAGE_SIZE), 0);

    let blocker = guard_hint;
    assert_eq!(
        mmap_anon(blocker, PAGE_SIZE, MAP_FIXED_NOREPLACE),
        blocker as isize
    );

    let child = fork();
    assert!(child >= 0);
    if child == 0 {
        // SAFETY: this faults the page immediately below the grow-down VMA.
        // With a proper stack guard gap, the lower blocker prevents expansion.
        unsafe {
            ((GROW_START - 1) as *mut u8).write_volatile(0x5a);
        }
        exit(7);
    }

    let mut status = 0i32;
    assert_eq!(waitpid(child, &mut status), child);
    assert_eq!(wait_termsig(status), SIGSEGV);

    assert_eq!(munmap(blocker, PAGE_SIZE), 0);
    assert_eq!(munmap(GROW_START, PAGE_SIZE), 0);

    println!("growsdown_guard_smoke passed");
    0
}
