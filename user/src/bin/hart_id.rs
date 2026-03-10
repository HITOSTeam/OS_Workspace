#![no_std]
#![no_main]

#[macro_use]
extern crate user;

extern crate alloc;
use alloc::vec::Vec;
use user::syscall::{_yield, get_hartid, thread_create, waittid};

fn worker(id: usize) -> ! {
    let hart = get_hartid();
    println!("[worker {}] running on hart {}", id, hart);
    // Keep yielding a few times to show if the task migrates across harts.
    for i in 0..5 {
        _yield();
        let hart_now = get_hartid();
        println!("[worker {}] yield #{} -> hart {}", id, i, hart_now);
    }
    user::syscall::exit(0)
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    println!("[hart_test] main hart {}", get_hartid());
    let mut tids = Vec::new();
    for id in 0..4 {
        let tid = thread_create(worker as usize, id);
        tids.push(tid);
    }
    for tid in tids {
        let code = waittid(tid as usize);
        println!("[hart_test] thread {} exit {}", tid, code);
    }
    0
}
