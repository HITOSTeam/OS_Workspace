#![no_std]
#![no_main]

use user::{self, println, syscall::*};
const CONDVAR_ID: usize = 0;
const MUTEX_ID: usize = 0;
static mut A: usize = 0;
unsafe fn first() -> ! {
    sleep(10);
    println!("First work, Change A --> 1 and wakeup Second");
    mutex_lock(MUTEX_ID);
    A = 1;
    condvar_signal(CONDVAR_ID);
    mutex_unlock(MUTEX_ID);
    exit(0)
}

unsafe fn second() -> ! {
    println!("Second want to continue,but need to wait A=1");
    mutex_lock(MUTEX_ID);
    while A == 0 {
        let temp = A;
        println!("Second: A is {}", temp);
        condvar_wait(CONDVAR_ID, MUTEX_ID);
    }

    let temp = A;
    println!("A is {}, Second can work now", temp);
    mutex_unlock(MUTEX_ID);
    exit(0)
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    // create condvar & mutex
    assert_eq!(condvar_create() as usize, CONDVAR_ID);
    assert_eq!(mutex_blocking_create() as usize, MUTEX_ID);
    // create two threads
    let tid2 = thread_create(second as usize, 0);

    let tid1 = thread_create(first as usize, 0);
    // wait for two threads to complete
    let exit_code2 = waittid(tid2 as usize);
    println!("thread#{} exited with code {}", tid2, exit_code2);
    let exit_code1 = waittid(tid1 as usize);
    println!("thread#{} exited with code {}", tid1, exit_code1);

    println!("main thread exited.");
    0
}
