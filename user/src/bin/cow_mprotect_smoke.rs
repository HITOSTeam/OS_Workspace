#![no_std]
#![no_main]

#[macro_use]
extern crate user;

use user::syscall::{fork, syscall, waitpid};

const PAGE_SIZE: usize = 4096;
const SYSCALL_MUNMAP: usize = 215;
const SYSCALL_MMAP: usize = 222;
const SYSCALL_MPROTECT: usize = 226;

const PROT_READ: usize = 1;
const PROT_WRITE: usize = 2;
const MAP_PRIVATE: usize = 0x02;
const MAP_ANONYMOUS: usize = 0x20;

fn mmap_private_anon(len: usize) -> isize {
    syscall(
        SYSCALL_MMAP,
        [
            0,
            len,
            PROT_READ | PROT_WRITE,
            MAP_PRIVATE | MAP_ANONYMOUS,
            usize::MAX,
            0,
        ],
    )
}

fn mprotect(addr: usize, len: usize, prot: usize) -> isize {
    syscall(SYSCALL_MPROTECT, [addr, len, prot, 0, 0, 0])
}

fn munmap(addr: usize, len: usize) -> isize {
    syscall(SYSCALL_MUNMAP, [addr, len, 0, 0, 0, 0])
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let mapped = mmap_private_anon(PAGE_SIZE);
    assert!(mapped > 0);
    let ptr = mapped as *mut u8;

    unsafe {
        ptr.write_volatile(0x41);
    }

    let pid = fork();
    assert!(pid >= 0);
    if pid == 0 {
        assert_eq!(mprotect(ptr as usize, PAGE_SIZE, PROT_READ), 0);
        unsafe {
            ptr.write_volatile(0x42);
        }
        return 7;
    }

    let mut exit_code = 0i32;
    assert_eq!(waitpid(pid, &mut exit_code), pid);
    assert_eq!(exit_code, 139);

    unsafe {
        assert_eq!(ptr.read_volatile(), 0x41);
        ptr.write_volatile(0x43);
        assert_eq!(ptr.read_volatile(), 0x43);
    }

    assert_eq!(munmap(ptr as usize, PAGE_SIZE), 0);
    println!("cow_mprotect_smoke passed");
    0
}
