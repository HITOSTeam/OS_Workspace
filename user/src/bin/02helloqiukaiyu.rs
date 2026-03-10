#![no_std]
#![no_main]
use user::{println, syscall::_yield};

#[unsafe(no_mangle)]
fn main() -> usize {
    println!("Hello, qiukaiyu !");
    _yield();
    println!("Hello, qiukaiyu !");
    return 0;
}
