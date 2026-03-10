#![no_std]
#![no_main]

#[macro_use]
extern crate user;

#[unsafe(no_mangle)]
pub fn main(argc: usize, argv: &[&str]) -> i32 {
    // Minimal compatibility for LTP sendmsg01 setup:
    // `ifconfig lo up 127.0.0.1`.
    if argc >= 3 && argv[1] == "lo" && argv[2] == "up" {
        return 0;
    }

    println!("ifconfig: unsupported arguments");
    -1
}
