#![no_std]
#![no_main]

use user::{
    self, println,
    syscall::{RDONLY, close, open},
};
#[unsafe(no_mangle)]
fn main() -> usize {
    println!("05open test start");
    let num = open("123", RDONLY);
    println!("open file 123, got fd = {}", num);
    close(4);
    return 0;
}
