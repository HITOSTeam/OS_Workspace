#![no_std]
#![no_main]

#[macro_use]
extern crate user;

extern crate alloc;

use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use user::syscall::{_yield, get_hartid, thread_create, waittid};

// Track which harts ran the workers.
static HART_SEEN: [AtomicBool; 8] = [
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
];

fn worker(id: usize) -> ! {
    let hart = get_hartid() as usize;
    if hart < HART_SEEN.len() {
        HART_SEEN[hart].store(true, Ordering::Relaxed);
    }
    // Busy work with occasional yields so other harts pick up runnable threads.
    let mut acc = 0usize;
    for i in 0..5_000_000 {
        acc = acc.wrapping_add(i);
        if i % 10_000 == 0 {
            _yield();
        }
    }
    println!("[mc worker {}] hart {} done (acc={})", id, hart, acc);
    user::syscall::exit(0)
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    println!("[mc] main hart {}", get_hartid());
    let mut tids = Vec::new();
    for id in 0..8 {
        let tid = thread_create(worker as usize, id);
        tids.push(tid);
    }
    for tid in tids {
        let code = waittid(tid as usize);
        println!("[mc] thread {} exit {}", tid, code);
    }

    // Report which harts executed workers.
    let mut used = Vec::new();
    for (hart, flag) in HART_SEEN.iter().enumerate() {
        if flag.load(Ordering::Relaxed) {
            used.push(hart);
        }
    }
    if used.len() <= 1 {
        println!("[mc] WARN: workers only ran on hart {:?}", used);
    } else {
        println!("[mc] workers observed harts {:?}", used);
    }
    0
}
