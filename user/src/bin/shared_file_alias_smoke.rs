#![no_std]
#![no_main]

#[macro_use]
extern crate user;

use user::syscall::{CREATE, RDWR, TRUNC, close, open, syscall, write};

const PAGE_SIZE: usize = 4096;

const SYSCALL_MUNMAP: usize = 215;
const SYSCALL_MMAP: usize = 222;

const PROT_READ: usize = 1;
const PROT_WRITE: usize = 2;
const MAP_SHARED: usize = 0x01;
const MAP_PRIVATE: usize = 0x02;

fn mmap_with_flags(fd: usize, flags: usize, len: usize) -> isize {
    syscall(SYSCALL_MMAP, [0, len, PROT_READ | PROT_WRITE, flags, fd, 0])
}

fn mmap_shared(fd: usize, len: usize) -> isize {
    mmap_with_flags(fd, MAP_SHARED, len)
}

fn mmap_private(fd: usize, len: usize) -> isize {
    mmap_with_flags(fd, MAP_PRIVATE, len)
}

fn munmap(addr: usize, len: usize) -> isize {
    syscall(SYSCALL_MUNMAP, [addr, len, 0, 0, 0, 0])
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let fd = open("/tmp/shared_file_alias_smoke", RDWR | CREATE | TRUNC);
    assert!(fd >= 0);
    let fd = fd as usize;

    let zeros = [0u8; PAGE_SIZE];
    assert_eq!(write(fd, &zeros), PAGE_SIZE as isize);

    let first = mmap_shared(fd, PAGE_SIZE);
    let second = mmap_shared(fd, PAGE_SIZE);
    assert!(first > 0 && second > 0);
    let first = first as *mut u8;
    let second = second as *mut u8;

    // SAFETY: both mappings cover the same writable shared file page.
    unsafe {
        first.write_volatile(0x51);
        assert_eq!(second.read_volatile(), 0x51);
        second.add(128).write_volatile(0x62);
        assert_eq!(first.add(128).read_volatile(), 0x62);
    }

    let private = mmap_private(fd, PAGE_SIZE);
    assert!(private > 0);
    let private = private as *mut u8;

    // SAFETY: private and shared mappings are valid; private writes must not
    // update the shared per-mm page-cache frame.
    unsafe {
        assert_eq!(first.add(256).read_volatile(), 0);
        private.add(256).write_volatile(0x73);
        assert_eq!(first.add(256).read_volatile(), 0);
    }

    assert_eq!(munmap(first as usize, PAGE_SIZE), 0);
    assert_eq!(munmap(second as usize, PAGE_SIZE), 0);
    assert_eq!(munmap(private as usize, PAGE_SIZE), 0);
    close(fd);

    println!("shared_file_alias_smoke passed");
    0
}
