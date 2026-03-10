#![no_std]
#![no_main]

use user;
#[unsafe(no_mangle)]
fn main() -> usize {
    user::syscall::_yield();
    return 0;
}
