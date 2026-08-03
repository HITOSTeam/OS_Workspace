#![no_std]
#![no_main]

#[macro_use]
extern crate user;

use user::syscall::{CREATE, RDWR, TRUNC, close, open, syscall, write};

const PAGE_SIZE: usize = 4096;

const SYSCALL_PREAD64: usize = 67;
const SYSCALL_PWRITE64: usize = 68;
const SYSCALL_MUNMAP: usize = 215;
const SYSCALL_MMAP: usize = 222;

const PROT_READ: usize = 1;
const PROT_WRITE: usize = 2;
const MAP_PRIVATE: usize = 0x02;

fn pread64(fd: usize, buf: &mut [u8], off: usize) -> isize {
    syscall(
        SYSCALL_PREAD64,
        [fd, buf.as_mut_ptr() as usize, buf.len(), off, 0, 0],
    )
}

fn pwrite64(fd: usize, buf: &[u8], off: usize) -> isize {
    syscall(
        SYSCALL_PWRITE64,
        [fd, buf.as_ptr() as usize, buf.len(), off, 0, 0],
    )
}

fn mmap_private(fd: usize) -> isize {
    syscall(
        SYSCALL_MMAP,
        [0, PAGE_SIZE, PROT_READ | PROT_WRITE, MAP_PRIVATE, fd, 0],
    )
}

fn munmap(addr: usize) -> isize {
    syscall(SYSCALL_MUNMAP, [addr, PAGE_SIZE, 0, 0, 0, 0])
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let fd = open("/tmp/private_file_page_cache_smoke", RDWR | CREATE | TRUNC);
    assert!(fd >= 0);
    let fd = fd as usize;

    let mut initial = [0u8; PAGE_SIZE];
    initial[0] = b'A';
    initial[127] = b'X';
    assert_eq!(write(fd, &initial), PAGE_SIZE as isize);

    let first = mmap_private(fd);
    let second = mmap_private(fd);
    assert!(first > 0 && second > 0 && first != second);
    let first = first as *mut u8;
    let second = second as *mut u8;

    // Both clean private mappings initially reference the inode page-cache
    // frame.  A write to one mapping must split only that mapping with COW.
    unsafe {
        assert_eq!(first.read_volatile(), b'A');
        assert_eq!(second.read_volatile(), b'A');
        first.write_volatile(b'B');
        assert_eq!(first.read_volatile(), b'B');
        assert_eq!(second.read_volatile(), b'A');
    }

    // Linux keeps an unmodified MAP_PRIVATE mapping coherent with page-cache
    // updates.  The mapping that already took a COW fault remains private.
    assert_eq!(pwrite64(fd, b"C", 0), 1);
    unsafe {
        assert_eq!(first.read_volatile(), b'B');
        assert_eq!(second.read_volatile(), b'C');
        assert_eq!(first.add(127).read_volatile(), b'X');
        assert_eq!(second.add(127).read_volatile(), b'X');
    }

    // A private write must not leak back into the file or the other mapping.
    unsafe {
        second.add(127).write_volatile(b'Y');
        assert_eq!(first.add(127).read_volatile(), b'X');
    }
    let mut disk = [0u8; 128];
    assert_eq!(pread64(fd, &mut disk, 0), disk.len() as isize);
    assert_eq!(disk[0], b'C');
    assert_eq!(disk[127], b'X');

    assert_eq!(munmap(first as usize), 0);
    assert_eq!(munmap(second as usize), 0);
    close(fd);

    println!("private_file_page_cache_smoke passed");
    0
}
