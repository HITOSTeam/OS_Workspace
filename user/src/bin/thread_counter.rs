#![no_std]
#![no_main]

#[macro_use]
extern crate user;
extern crate alloc;

use alloc::vec;
use user::syscall::{exit, thread_create, waittid};

pub fn thread_a(sum_ptr: *mut usize) -> ! {
    for _ in 0..10000 {
        unsafe {
            *sum_ptr += 1;
        }
    }
    exit(1)
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let mut sum = 0;
    let mut v = vec![];
    for _ in 0..100 {
        v.push(thread_create(
            thread_a as usize,
            &mut sum as *mut _ as usize,
        ));
    }
    for tid in v.iter() {
        println!("waiting for thread#{} to exit...", tid);
        let exit_code = waittid(*tid as usize);
        println!("thread#{} exited with code {}", tid, exit_code);
    }
    println!("main thread exited.");
    println!("sum = {}", sum);
    0
}
