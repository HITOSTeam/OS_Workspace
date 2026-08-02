#![no_std]
#![no_main]

#[macro_use]
extern crate user;

use core::arch::asm;
use core::sync::atomic::{AtomicUsize, Ordering};
use user::syscall::{_yield, CREATE, RDWR, TRUNC, close, exit, open, syscall, write};

const PAGE_SIZE: usize = 4096;
const PAIR_COUNT: usize = 256;
const MAP_LEN: usize = PAIR_COUNT * PAGE_SIZE * 2;
const CHILD_STACK_ADDR: usize = 0x31_1000_0000;
const CHILD_STACK_SIZE: usize = PAGE_SIZE * 16;

const SYSCALL_CLONE: usize = 220;
const SYSCALL_MUNMAP: usize = 215;
const SYSCALL_MMAP: usize = 222;
const SYSCALL_SCHED_SETAFFINITY: usize = 122;

const CLONE_VM: usize = 0x0000_0100;
const CLONE_SIGHAND: usize = 0x0000_0800;
const CLONE_THREAD: usize = 0x0001_0000;

const PROT_READ: usize = 1;
const PROT_WRITE: usize = 2;
const MAP_PRIVATE: usize = 0x02;
const MAP_FIXED: usize = 0x10;
const MAP_ANONYMOUS: usize = 0x20;
const EXPECTED: u8 = 0x5a;

static MAP_BASE: AtomicUsize = AtomicUsize::new(0);
static CHILD_READY: AtomicUsize = AtomicUsize::new(0);
static CHILD_START: AtomicUsize = AtomicUsize::new(0);
static CHILD_DONE: AtomicUsize = AtomicUsize::new(0);
static CHILD_ERROR_PAGE: AtomicUsize = AtomicUsize::new(usize::MAX);

fn mmap_file(fd: usize) -> isize {
    syscall(SYSCALL_MMAP, [0, MAP_LEN, PROT_READ, MAP_PRIVATE, fd, 0])
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

fn munmap(addr: usize, len: usize) -> isize {
    syscall(SYSCALL_MUNMAP, [addr, len, 0, 0, 0, 0])
}

fn pin_thread_to_cpu(tid: usize, cpu: usize) -> isize {
    let mask = 1usize << cpu;
    syscall(
        SYSCALL_SCHED_SETAFFINITY,
        [
            tid,
            core::mem::size_of::<usize>(),
            &mask as *const usize as usize,
            0,
            0,
            0,
        ],
    )
}

fn child_body() -> ! {
    CHILD_READY.store(1, Ordering::Release);
    while CHILD_START.load(Ordering::Acquire) == 0 {
        core::hint::spin_loop();
    }

    let base = MAP_BASE.load(Ordering::Acquire) as *const u8;
    for pair in 0..PAIR_COUNT {
        let page = pair * 2 + 1;
        let value = unsafe { base.add(page * PAGE_SIZE).read_volatile() };
        if value != EXPECTED {
            CHILD_ERROR_PAGE.store(page, Ordering::Release);
            break;
        }
    }
    CHILD_DONE.store(1, Ordering::Release);
    exit(0);
}

extern "C" fn child_entry(_arg: usize) -> ! {
    child_body()
}

#[cfg(target_arch = "loongarch64")]
#[inline(never)]
fn clone_same_mm_thread(child_stack: usize) -> isize {
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
fn clone_same_mm_thread(child_stack: usize) -> isize {
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

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    if pin_thread_to_cpu(0, 0) != 0 {
        println!("lazy_fault_local_tlb_smoke: parent affinity failed");
        return 1;
    }

    let fd = open("/tmp/lazy_fault_local_tlb_smoke", RDWR | CREATE | TRUNC);
    if fd < 0 {
        println!("lazy_fault_local_tlb_smoke: open failed {}", fd);
        return 1;
    }
    let fd = fd as usize;
    let page = [EXPECTED; PAGE_SIZE];
    for page_index in 0..PAIR_COUNT * 2 {
        if write(fd, &page) != PAGE_SIZE as isize {
            println!(
                "lazy_fault_local_tlb_smoke: write failed page={}",
                page_index
            );
            close(fd);
            return 1;
        }
    }

    let mapped = mmap_file(fd);
    close(fd);
    if mapped <= 0 {
        println!("lazy_fault_local_tlb_smoke: mmap failed {}", mapped);
        return 1;
    }
    let base = mapped as usize;
    MAP_BASE.store(base, Ordering::Release);

    if mmap_child_stack() != CHILD_STACK_ADDR as isize {
        println!("lazy_fault_local_tlb_smoke: stack mmap failed");
        return 1;
    }
    let tid = clone_same_mm_thread(CHILD_STACK_ADDR + CHILD_STACK_SIZE);
    if tid < 0 {
        println!("lazy_fault_local_tlb_smoke: clone failed {}", tid);
        return 1;
    }
    if pin_thread_to_cpu(tid as usize, 1) != 0 {
        println!("lazy_fault_local_tlb_smoke: child affinity failed");
        return 1;
    }
    while CHILD_READY.load(Ordering::Acquire) == 0 {
        _yield();
    }

    // Populate only the even half of every LoongArch TLB pair on CPU 0.
    // The cached odd halves are invalid when CPU 1 publishes their PTEs.
    let base_ptr = base as *const u8;
    for pair in 0..PAIR_COUNT {
        let page = pair * 2;
        let value = unsafe { base_ptr.add(page * PAGE_SIZE).read_volatile() };
        if value != EXPECTED {
            println!(
                "lazy_fault_local_tlb_smoke: parent even mismatch page={} value={:#x}",
                page, value
            );
            return 1;
        }
    }

    CHILD_START.store(1, Ordering::Release);
    while CHILD_DONE.load(Ordering::Acquire) == 0 {
        core::hint::spin_loop();
    }
    let child_error = CHILD_ERROR_PAGE.load(Ordering::Acquire);
    if child_error != usize::MAX {
        println!(
            "lazy_fault_local_tlb_smoke: child mismatch page={}",
            child_error
        );
        return 1;
    }

    // CPU 1 did not shoot CPU 0 down when it installed these missing PTEs.
    // CPU 0 must take a recoverable spurious fault, observe the present PTE,
    // refresh only its own pair, and complete the load.
    for pair in 0..PAIR_COUNT {
        let page = pair * 2 + 1;
        let value = unsafe { base_ptr.add(page * PAGE_SIZE).read_volatile() };
        if value != EXPECTED {
            println!(
                "lazy_fault_local_tlb_smoke: parent odd mismatch page={} value={:#x}",
                page, value
            );
            return 1;
        }
    }

    assert_eq!(munmap(base, MAP_LEN), 0);
    assert_eq!(munmap(CHILD_STACK_ADDR, CHILD_STACK_SIZE), 0);
    println!("lazy_fault_local_tlb_smoke passed");
    0
}
