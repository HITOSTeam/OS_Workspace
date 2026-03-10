#![no_std]
#![no_main]

use user::{
    self, println,
    syscall::{RDONLY, close, fork, open},
};
#[unsafe(no_mangle)]
fn main() -> usize {
    fork();

    return 0;
}
