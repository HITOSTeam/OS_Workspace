#![no_std]
#![no_main]

use user::{print, syscall::*};

fn printa() -> ! {
    for _ in 0..10000 {
        print!("a");
    }
    exit(1)
}
fn printb() -> ! {
    for _ in 0..10000 {
        print!("b");
    }
    exit(2)
}

fn printc() -> ! {
    for _ in 0..10000 {
        print!("c");
    }
    exit(3)
}
#[unsafe(no_mangle)]
unsafe fn main() -> usize {
    thread_create(printa as usize, 0);
    thread_create(printb as usize, 0);
    thread_create(printc as usize, 0);
    sleep(1000);
    0
}
