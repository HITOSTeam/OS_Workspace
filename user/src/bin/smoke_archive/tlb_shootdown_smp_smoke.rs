#![no_std]
#![no_main]

#[macro_use]
extern crate user;

use core::arch::asm;
use core::sync::atomic::{AtomicUsize, Ordering};
use user::syscall::{_yield, exit, syscall};

const PAGE_SIZE: usize = 4096;
const SMALL_RANGE_SIZE: usize = PAGE_SIZE * 16;
const LARGE_RANGE_SIZE: usize = 4 * 1024 * 1024;
const TEST_ADDR: usize = 0x30_0000_0000;
const CHILD_STACK_ADDR: usize = 0x31_0000_0000;
const CHILD_STACK_SIZE: usize = PAGE_SIZE * 16;

const SYSCALL_CLONE: usize = 220;
const SYSCALL_MMAP: usize = 222;
const SYSCALL_MPROTECT: usize = 226;
const SYSCALL_SCHED_SETAFFINITY: usize = 122;

const CLONE_VM: usize = 0x0000_0100;
const CLONE_SIGHAND: usize = 0x0000_0800;
const CLONE_THREAD: usize = 0x0001_0000;

const PROT_READ: usize = 1;
const PROT_WRITE: usize = 2;
const MAP_PRIVATE: usize = 0x02;
const MAP_FIXED: usize = 0x10;
const MAP_ANONYMOUS: usize = 0x20;

static START: AtomicUsize = AtomicUsize::new(0);
static CHILD_READY: AtomicUsize = AtomicUsize::new(0);
static DONE: AtomicUsize = AtomicUsize::new(0);
static FAILED_STEP: AtomicUsize = AtomicUsize::new(0);

fn mmap_fixed(addr: usize, len: usize) -> isize {
    syscall(
        SYSCALL_MMAP,
        [
            addr,
            len,
            PROT_READ | PROT_WRITE,
            MAP_PRIVATE | MAP_FIXED | MAP_ANONYMOUS,
            usize::MAX,
            0,
        ],
    )
}

fn mprotect(addr: usize, len: usize, prot: usize) -> isize {
    syscall(SYSCALL_MPROTECT, [addr, len, prot, 0, 0, 0])
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
    while START.load(Ordering::Acquire) == 0 {
        core::hint::spin_loop();
    }
    println!("tlb_shootdown_smp_smoke: child started");

    let operations = [
        (PAGE_SIZE, PROT_READ),
        (PAGE_SIZE, PROT_READ | PROT_WRITE),
        (SMALL_RANGE_SIZE, PROT_READ),
        (SMALL_RANGE_SIZE, PROT_READ | PROT_WRITE),
        (LARGE_RANGE_SIZE, PROT_READ),
        (LARGE_RANGE_SIZE, PROT_READ | PROT_WRITE),
    ];
    for (index, (len, prot)) in operations.into_iter().enumerate() {
        println!(
            "tlb_shootdown_smp_smoke: step={} len={} prot={}",
            index + 1,
            len,
            prot
        );
        if mprotect(TEST_ADDR, len, prot) != 0 {
            FAILED_STEP.store(index + 1, Ordering::Release);
            break;
        }
    }

    DONE.store(1, Ordering::Release);
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
        println!("tlb_shootdown_smp_smoke: parent affinity failed");
        return 1;
    }
    if mmap_fixed(CHILD_STACK_ADDR, CHILD_STACK_SIZE) != CHILD_STACK_ADDR as isize {
        println!("tlb_shootdown_smp_smoke: stack mmap failed");
        return 1;
    }
    let tid = clone_same_mm_thread(CHILD_STACK_ADDR + CHILD_STACK_SIZE);
    if tid < 0 {
        println!("tlb_shootdown_smp_smoke: clone failed {}", tid);
        return 1;
    }
    if pin_thread_to_cpu(tid as usize, 1) != 0 {
        println!("tlb_shootdown_smp_smoke: affinity failed");
        return 1;
    }
    println!("tlb_shootdown_smp_smoke: child tid={} cpu=1", tid);
    while CHILD_READY.load(Ordering::Acquire) == 0 {
        _yield();
    }

    if mmap_fixed(TEST_ADDR, LARGE_RANGE_SIZE) != TEST_ADDR as isize {
        println!("tlb_shootdown_smp_smoke: mmap failed");
        return 1;
    }
    println!("tlb_shootdown_smp_smoke: mappings ready");

    // Materialize the whole range so all three invalidation classes operate
    // on real PTEs. Keep the first translation hot while the child edits them.
    for offset in (0..LARGE_RANGE_SIZE).step_by(PAGE_SIZE) {
        unsafe {
            (TEST_ADDR as *mut u8).add(offset).write_volatile(0x5a);
        }
    }
    println!("tlb_shootdown_smp_smoke: pages materialized");

    START.store(1, Ordering::Release);

    let mut checksum = 0u8;
    while DONE.load(Ordering::Acquire) == 0 {
        checksum ^= unsafe { (TEST_ADDR as *const u8).read_volatile() };
        core::hint::spin_loop();
    }

    let failed_step = FAILED_STEP.load(Ordering::Acquire);
    let first = unsafe { (TEST_ADDR as *const u8).read_volatile() };
    let last = unsafe {
        (TEST_ADDR as *const u8)
            .add(LARGE_RANGE_SIZE - PAGE_SIZE)
            .read_volatile()
    };

    if failed_step != 0 || first != 0x5a || last != 0x5a {
        println!(
            "tlb_shootdown_smp_smoke failed: step={} first={:#x} last={:#x} checksum={:#x}",
            failed_step, first, last, checksum
        );
        return 1;
    }

    println!("tlb_shootdown_smp_smoke passed");
    0
}
