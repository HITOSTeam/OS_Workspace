#![no_std]
#![no_main]

extern crate alloc;

#[macro_use]
extern crate user;

use alloc::string::String;
use user::syscall::syscall;

const SYSCALL_UMOUNT2: usize = 39;

fn with_c_path<T>(path: &str, f: impl FnOnce(*const u8) -> T) -> T {
    let mut owned = String::from(path);
    owned.push('\0');
    f(owned.as_ptr())
}

fn linux_umount2(target: &str, flags: usize) -> isize {
    with_c_path(target, |ptr| {
        syscall(SYSCALL_UMOUNT2, [ptr as usize, flags, 0, 0, 0, 0])
    })
}

#[unsafe(no_mangle)]
pub fn main(argc: usize, argv: &[&str]) -> i32 {
    if argc != 2 {
        println!("usage: umount_once TARGET");
        return 1;
    }

    let rc = linux_umount2(argv[1], 0);
    if rc == 0 {
        return 0;
    }

    println!("umount: can't unmount {}: errno {}", argv[1], -rc);
    1
}
