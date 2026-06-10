#![no_std]
#![no_main]

#[macro_use]
extern crate user;

use user::syscall::syscall;

const PAGE_SIZE: usize = 4096;
const SYSCALL_MADVISE: usize = 233;
const MADV_DONTNEED: usize = 4;

fn madvise(addr: usize, len: usize, advice: usize) -> isize {
    syscall(SYSCALL_MADVISE, [addr, len, advice, 0, 0, 0])
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let marker = 0usize;
    let marker_page = (&marker as *const usize as usize) & !(PAGE_SIZE - 1);
    let target = marker_page
        .checked_sub(PAGE_SIZE * 64)
        .expect("stack probe page must stay within the initial user stack");
    let ptr = target as *mut u8;

    unsafe {
        ptr.write_volatile(0x5a);
        assert_eq!(ptr.read_volatile(), 0x5a);
    }

    assert_eq!(madvise(target, PAGE_SIZE, MADV_DONTNEED), 0);
    unsafe {
        assert_eq!(ptr.read_volatile(), 0);
        ptr.write_volatile(0xa5);
        assert_eq!(ptr.read_volatile(), 0xa5);
    }

    assert_eq!(madvise(target, PAGE_SIZE, MADV_DONTNEED), 0);
    unsafe {
        assert_eq!(ptr.read_volatile(), 0);
    }

    println!("stack_madvise_dontneed_smoke passed");
    0
}
