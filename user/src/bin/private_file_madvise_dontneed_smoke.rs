#![no_std]
#![no_main]

#[macro_use]
extern crate user;

use user::syscall::{CREATE, RDWR, TRUNC, close, open, syscall, write};

const PAGE_SIZE: usize = 4096;
const SYSCALL_MADVISE: usize = 233;
const SYSCALL_MMAP: usize = 222;
const SYSCALL_MUNMAP: usize = 215;

const PROT_READ: usize = 1;
const PROT_WRITE: usize = 2;
const MAP_PRIVATE: usize = 0x02;
const MADV_DONTNEED: usize = 4;

fn mmap_private_file(fd: usize, len: usize) -> isize {
    syscall(
        SYSCALL_MMAP,
        [0, len, PROT_READ | PROT_WRITE, MAP_PRIVATE, fd, 0],
    )
}

fn madvise(addr: usize, len: usize, advice: usize) -> isize {
    syscall(SYSCALL_MADVISE, [addr, len, advice, 0, 0, 0])
}

fn munmap(addr: usize, len: usize) -> isize {
    syscall(SYSCALL_MUNMAP, [addr, len, 0, 0, 0, 0])
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let fd = open(
        "/tmp/private_file_madvise_dontneed_smoke",
        RDWR | CREATE | TRUNC,
    );
    assert!(fd >= 0);
    let fd = fd as usize;
    assert_eq!(write(fd, b"A"), 1);

    let mapped = mmap_private_file(fd, PAGE_SIZE);
    assert!(mapped > 0);
    let ptr = mapped as *mut u8;

    unsafe {
        assert_eq!(ptr.read_volatile(), b'A');
        assert_eq!(ptr.add(1).read_volatile(), 0);
        ptr.write_volatile(b'B');
        assert_eq!(ptr.read_volatile(), b'B');
    }

    assert_eq!(madvise(mapped as usize, PAGE_SIZE, MADV_DONTNEED), 0);
    unsafe {
        assert_eq!(ptr.read_volatile(), b'A');
        assert_eq!(ptr.add(1).read_volatile(), 0);
        ptr.write_volatile(b'C');
        assert_eq!(ptr.read_volatile(), b'C');
    }

    assert_eq!(madvise(mapped as usize, PAGE_SIZE, MADV_DONTNEED), 0);
    unsafe {
        assert_eq!(ptr.read_volatile(), b'A');
    }

    assert_eq!(munmap(mapped as usize, PAGE_SIZE), 0);
    close(fd);

    println!("private_file_madvise_dontneed_smoke passed");
    0
}
