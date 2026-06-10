#![no_std]
#![no_main]

#[macro_use]
extern crate user;

use user::syscall::{close, syscall};

const PAGE_SIZE: usize = 4096;

const SYSCALL_FTRUNCATE: usize = 46;
const SYSCALL_MUNMAP: usize = 215;
const SYSCALL_MREMAP: usize = 216;
const SYSCALL_MMAP: usize = 222;
const SYSCALL_MEMFD_CREATE: usize = 279;

const PROT_READ: usize = 1;
const PROT_WRITE: usize = 2;
const MAP_SHARED: usize = 0x01;
const MREMAP_MAYMOVE: usize = 0x01;
const MREMAP_FIXED: usize = 0x02;

fn memfd_create(name: &[u8]) -> isize {
    syscall(
        SYSCALL_MEMFD_CREATE,
        [name.as_ptr() as usize, 0, 0, 0, 0, 0],
    )
}

fn ftruncate(fd: usize, len: usize) -> isize {
    syscall(SYSCALL_FTRUNCATE, [fd, len, 0, 0, 0, 0])
}

fn mmap_shared(fd: usize, len: usize) -> isize {
    syscall(
        SYSCALL_MMAP,
        [0, len, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0],
    )
}

fn mremap_grow(addr: usize, old_len: usize, new_len: usize) -> isize {
    syscall(
        SYSCALL_MREMAP,
        [addr, old_len, new_len, MREMAP_MAYMOVE, 0, 0],
    )
}

fn mremap_fixed(old_addr: usize, len: usize, new_addr: usize) -> isize {
    syscall(
        SYSCALL_MREMAP,
        [
            old_addr,
            len,
            len,
            MREMAP_MAYMOVE | MREMAP_FIXED,
            new_addr,
            0,
        ],
    )
}

fn munmap(addr: usize, len: usize) -> isize {
    syscall(SYSCALL_MUNMAP, [addr, len, 0, 0, 0, 0])
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let fd = memfd_create(b"memfd_mremap_shared_smoke\0");
    assert!(fd >= 0);
    let fd = fd as usize;

    assert_eq!(ftruncate(fd, PAGE_SIZE * 2), 0);
    let mapped = mmap_shared(fd, PAGE_SIZE);
    assert!(mapped > 0);

    let grown = mremap_grow(mapped as usize, PAGE_SIZE, PAGE_SIZE * 2);
    assert!(grown > 0);
    let grown = grown as *mut u8;

    // SAFETY: the mapping is readable/writable for two pages after mremap.
    unsafe {
        grown.write_volatile(0x5a);
        grown.add(PAGE_SIZE).write_volatile(0x7b);
    }

    let mirror = mmap_shared(fd, PAGE_SIZE * 2);
    assert!(mirror > 0);
    let mirror = mirror as *const u8;

    // SAFETY: mirror maps the same memfd for two pages. The second byte check
    // specifically verifies that mremap grow inserted a shared frame, not a
    // private anonymous lazy page.
    unsafe {
        assert_eq!(mirror.read_volatile(), 0x5a);
        assert_eq!(mirror.add(PAGE_SIZE).read_volatile(), 0x7b);
    }

    assert_eq!(munmap(grown as usize, PAGE_SIZE * 2), 0);
    assert_eq!(munmap(mirror as usize, PAGE_SIZE * 2), 0);
    close(fd);

    let src_fd = memfd_create(b"memfd_mremap_fixed_src\0");
    let dst_fd = memfd_create(b"memfd_mremap_fixed_dst\0");
    assert!(src_fd >= 0 && dst_fd >= 0);
    let src_fd = src_fd as usize;
    let dst_fd = dst_fd as usize;
    assert_eq!(ftruncate(src_fd, PAGE_SIZE), 0);
    assert_eq!(ftruncate(dst_fd, PAGE_SIZE), 0);

    let src = mmap_shared(src_fd, PAGE_SIZE);
    let dst = mmap_shared(dst_fd, PAGE_SIZE);
    assert!(src > 0 && dst > 0);
    let src = src as usize;
    let dst = dst as usize;
    unsafe {
        (src as *mut u8).write_volatile(0x31);
        (dst as *mut u8).write_volatile(0x42);
    }

    let moved = mremap_fixed(src, PAGE_SIZE, dst);
    assert_eq!(moved, dst as isize);
    unsafe {
        assert_eq!((dst as *const u8).read_volatile(), 0x31);
    }
    assert_eq!(munmap(dst, PAGE_SIZE), 0);
    close(src_fd);
    close(dst_fd);

    println!("memfd_mremap_shared_smoke passed");
    0
}
