#![no_std]
#![no_main]
use user::{
    print, println,
    syscall::{_yield, exec, fork, waitpid},
};

#[unsafe(no_mangle)]
fn main() -> usize {
    let str1 = "all_tests\0";
    let str2 = str1;
    let str3 = str1;
    return 0;
}
