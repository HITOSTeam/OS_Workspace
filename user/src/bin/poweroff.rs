#![no_std]
#![no_main]

#[macro_use]
extern crate user;

use user::syscall::poweroff;

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    poweroff();
}
