#![no_std]
#![no_main]

use user;
#[unsafe(no_mangle)]
fn main() -> usize {
    user::syscall::syscall_fortest(0, 0);
    return 0;
}
