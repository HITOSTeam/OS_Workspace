#![no_std]
#![no_main]

#[macro_use]
extern crate user;

use user::syscall::{RDONLY, close, open, read};

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let fd = open("/proc/self/maps", RDONLY);
    assert!(fd >= 0);
    let fd = fd as usize;

    let mut buf = [0u8; 4096];
    let len = read(fd, &mut buf);
    assert!(len > 0);
    assert_eq!(close(fd), 0);

    let maps = &buf[..len as usize];
    assert!(contains(maps, b"[stack]"));
    assert!(contains(maps, b" r"));

    println!("proc_maps_stack_smoke passed");
    0
}
