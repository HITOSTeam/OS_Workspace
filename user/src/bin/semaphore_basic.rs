#![no_std]
#![no_main]

#[macro_use]
extern crate user;
extern crate alloc;

use alloc::vec;
use user::syscall::*;
const SEM_MUTEX: usize = 0;
const SEM_EMPTY: usize = 1;
const SEM_AVAIL: usize = 2;
const BUFFER_SIZE: usize = 8;
static mut BUFFER: [usize; BUFFER_SIZE] = [0; BUFFER_SIZE];
static mut FRONT: usize = 0;
static mut TAIL: usize = 0;
const PRODUCER_COUNT: usize = 4;
const NUMBER_PER_PRODUCER: usize = 100;

const SEM_SYNC: usize = 0;

unsafe fn first() -> ! {
    sleep(10);
    println!("First work and wakeup Second");
    semaphore_up(SEM_SYNC);
    exit(0)
}

unsafe fn second() -> ! {
    println!("Second want to continue,but need to wait first");
    semaphore_down(SEM_SYNC);
    println!("Second can work now");
    exit(0)
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    // create semaphores
    assert_eq!(semaphore_create(0) as usize, SEM_SYNC);
    // create threads
    let threads = vec![
        thread_create(first as usize, 0),
        thread_create(second as usize, 0),
    ];
    // wait for all threads to complete
    for thread in threads.iter() {
        waittid(*thread as usize);
    }
    println!("sync_sem passed!");
    0
}
