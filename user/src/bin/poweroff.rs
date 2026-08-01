#![no_std]
#![no_main]

use user::syscall::poweroff;

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    poweroff();
}
