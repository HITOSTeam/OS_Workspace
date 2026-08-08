#![no_std]
#![no_main]

extern crate user;

use user::println;
use user::syscall::{CREATE, RDWR, TRUNC, close, exit, fork, open, syscall, waitpid, write};

const PAGE_SIZE: usize = 4096;
const TEST_ADDR: usize = 0x31_7000_0000;
const ITERATIONS: usize = 1100;

const SYSCALL_MMAP: usize = 222;
const SYSCALL_SCHED_SETAFFINITY: usize = 122;

const PROT_READ: usize = 1;
const MAP_PRIVATE: usize = 0x02;
const MAP_FIXED: usize = 0x10;

const BYTE_A: u8 = 0x35;
const BYTE_B: u8 = 0xca;

fn pin_to_cpu0() -> isize {
    let mask = 1usize;
    syscall(
        SYSCALL_SCHED_SETAFFINITY,
        [
            0,
            core::mem::size_of::<usize>(),
            &mask as *const usize as usize,
            0,
            0,
            0,
        ],
    )
}

fn map_page(fd: usize) -> isize {
    syscall(
        SYSCALL_MMAP,
        [
            TEST_ADDR,
            PAGE_SIZE,
            PROT_READ,
            MAP_PRIVATE | MAP_FIXED,
            fd,
            0,
        ],
    )
}

fn create_page(path: &str, byte: u8) -> isize {
    let fd = open(path, RDWR | CREATE | TRUNC);
    if fd < 0 {
        return fd;
    }
    let page = [byte; PAGE_SIZE];
    if write(fd as usize, &page) != PAGE_SIZE as isize {
        let _ = close(fd as usize);
        return -1;
    }
    fd
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    if pin_to_cpu0() != 0 {
        println!("LOONGARCH_ASID_WRAP_FAIL stage=parent-affinity");
        return 1;
    }

    let fd_a = create_page("/tmp/loongarch_asid_wrap_a", BYTE_A);
    let fd_b = create_page("/tmp/loongarch_asid_wrap_b", BYTE_B);
    if fd_a < 0 || fd_b < 0 {
        println!(
            "LOONGARCH_ASID_WRAP_FAIL stage=create fd_a={} fd_b={}",
            fd_a, fd_b
        );
        return 1;
    }

    for iteration in 0..ITERATIONS {
        let child = fork();
        if child == 0 {
            if pin_to_cpu0() != 0 {
                exit(10);
            }
            let (fd, expected) = if iteration & 1 == 0 {
                (fd_a as usize, BYTE_A)
            } else {
                (fd_b as usize, BYTE_B)
            };
            if map_page(fd) != TEST_ADDR as isize {
                exit(11);
            }
            let observed = unsafe { (TEST_ADDR as *const u8).read_volatile() };
            if observed != expected {
                println!(
                    "LOONGARCH_ASID_WRAP_MISMATCH iteration={} expected={:#x} observed={:#x}",
                    iteration, expected, observed
                );
                exit(12);
            }
            exit(0);
        }
        if child < 0 {
            println!(
                "LOONGARCH_ASID_WRAP_FAIL stage=fork iteration={} rc={}",
                iteration, child
            );
            return 1;
        }
        let mut status = -1;
        if waitpid(child, &mut status) != child || status != 0 {
            println!(
                "LOONGARCH_ASID_WRAP_FAIL stage=wait iteration={} status={}",
                iteration, status
            );
            return 1;
        }
        if (iteration + 1) % 128 == 0 {
            println!("LOONGARCH_ASID_WRAP_PROGRESS iterations={}", iteration + 1);
        }
    }

    let _ = close(fd_a as usize);
    let _ = close(fd_b as usize);
    println!("LOONGARCH_ASID_WRAP_PASS iterations={}", ITERATIONS);
    0
}
