#![no_std]
#![no_main]

extern crate user;

use user::println;
use user::syscall::{CREATE, RDWR, TRUNC, close, open, read, syscall, write};

const PAGE_SIZE: usize = 4096;
const ITERATIONS: usize = 32;

const SYSCALL_RENAMEAT: usize = 38;
const SYSCALL_PREAD64: usize = 67;
const SYSCALL_MUNMAP: usize = 215;
const SYSCALL_MMAP: usize = 222;

const AT_FDCWD: isize = -100;
const PROT_READ: usize = 1;
const MAP_PRIVATE: usize = 0x02;

const OLD_PATH: &str = "/tmp/rename_over_lifetime_target";
const NEW_PATH: &str = "/tmp/rename_over_lifetime_source";
const OLD_C_PATH: &[u8] = b"/tmp/rename_over_lifetime_target\0";
const NEW_C_PATH: &[u8] = b"/tmp/rename_over_lifetime_source\0";

fn create_page(path: &str, byte: u8) -> bool {
    let fd = open(path, RDWR | CREATE | TRUNC);
    if fd < 0 {
        return false;
    }
    let fd = fd as usize;
    let page = [byte; PAGE_SIZE];
    let wrote_page = write(fd, &page) == PAGE_SIZE as isize;
    let closed = close(fd) == 0;
    wrote_page && closed
}

fn mmap_private(fd: usize) -> isize {
    syscall(SYSCALL_MMAP, [0, PAGE_SIZE, PROT_READ, MAP_PRIVATE, fd, 0])
}

fn munmap(addr: usize) -> isize {
    syscall(SYSCALL_MUNMAP, [addr, PAGE_SIZE, 0, 0, 0, 0])
}

fn rename_source_over_target() -> isize {
    syscall(
        SYSCALL_RENAMEAT,
        [
            AT_FDCWD as usize,
            NEW_C_PATH.as_ptr() as usize,
            AT_FDCWD as usize,
            OLD_C_PATH.as_ptr() as usize,
            0,
            0,
        ],
    )
}

fn pread_byte(fd: usize) -> Option<u8> {
    let mut byte = [0u8; 1];
    (syscall(
        SYSCALL_PREAD64,
        [fd, byte.as_mut_ptr() as usize, byte.len(), 0, 0, 0],
    ) == 1)
        .then_some(byte[0])
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    for iteration in 0..ITERATIONS {
        let old_byte = 0x31u8.wrapping_add(iteration as u8);
        let new_byte = 0xc1u8.wrapping_sub(iteration as u8);
        if !create_page(OLD_PATH, old_byte) || !create_page(NEW_PATH, new_byte) {
            println!(
                "RENAME_OVER_MMAP_LIFETIME_FAIL stage=create iteration={}",
                iteration
            );
            return 1;
        }

        let old_fd = open(OLD_PATH, 0);
        if old_fd < 0 {
            println!(
                "RENAME_OVER_MMAP_LIFETIME_FAIL stage=open-old iteration={} rc={}",
                iteration, old_fd
            );
            return 1;
        }
        let old_fd = old_fd as usize;
        let mapped = mmap_private(old_fd);
        if mapped <= 0 {
            println!(
                "RENAME_OVER_MMAP_LIFETIME_FAIL stage=mmap iteration={} rc={}",
                iteration, mapped
            );
            let _ = close(old_fd);
            return 1;
        }

        // Exercise both Linux lifetime owners. Odd iterations close the fd
        // before rename, leaving only the VMA's file reference to pin the old
        // inode; even iterations keep the descriptor and validate both views.
        let keep_fd = iteration % 2 == 0;
        if !keep_fd && close(old_fd) != 0 {
            println!(
                "RENAME_OVER_MMAP_LIFETIME_FAIL stage=close-before-rename iteration={}",
                iteration
            );
            let _ = munmap(mapped as usize);
            return 1;
        }

        if rename_source_over_target() != 0 {
            println!(
                "RENAME_OVER_MMAP_LIFETIME_FAIL stage=rename iteration={}",
                iteration
            );
            let _ = munmap(mapped as usize);
            if keep_fd {
                let _ = close(old_fd);
            }
            return 1;
        }

        // Neither view was touched before rename. Linux keeps the replaced
        // inode alive through both struct file and the VMA, so these late
        // reads must still observe the old object rather than the replacement
        // or a freed, zeroed inode.
        let mapped_byte = unsafe { (mapped as *const u8).read_volatile() };
        let fd_byte = keep_fd.then(|| pread_byte(old_fd)).flatten();

        let replacement_fd = open(OLD_PATH, 0);
        let replacement_byte = if replacement_fd >= 0 {
            let mut byte = [0u8; 1];
            let got = read(replacement_fd as usize, &mut byte);
            let _ = close(replacement_fd as usize);
            (got == 1).then_some(byte[0])
        } else {
            None
        };

        let _ = munmap(mapped as usize);
        if keep_fd {
            let _ = close(old_fd);
        }

        if mapped_byte != old_byte
            || (keep_fd && fd_byte != Some(old_byte))
            || replacement_byte != Some(new_byte)
        {
            println!(
                "RENAME_OVER_MMAP_LIFETIME_FAIL stage=verify iteration={} expected_old={:#x} mapped={:#x} fd={:?} expected_new={:#x} replacement={:?}",
                iteration, old_byte, mapped_byte, fd_byte, new_byte, replacement_byte
            );
            return 1;
        }
    }

    println!(
        "RENAME_OVER_MMAP_LIFETIME_PASS iterations={} vma_only={} fd_and_vma={}",
        ITERATIONS,
        ITERATIONS / 2,
        ITERATIONS / 2
    );
    0
}
