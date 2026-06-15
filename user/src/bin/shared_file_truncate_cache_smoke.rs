#![no_std]
#![no_main]

#[macro_use]
extern crate user;

use user::syscall::{CREATE, RDWR, TRUNC, close, fork, open, pipe, read, syscall, waitpid, write};

const PAGE_SIZE: usize = 4096;

const SYSCALL_FTRUNCATE: usize = 46;
const SYSCALL_MUNMAP: usize = 215;
const SYSCALL_MMAP: usize = 222;

const PROT_READ: usize = 1;
const PROT_WRITE: usize = 2;
const MAP_SHARED: usize = 0x01;

fn ftruncate(fd: usize, len: usize) -> isize {
    syscall(SYSCALL_FTRUNCATE, [fd, len, 0, 0, 0, 0])
}

fn mmap_shared(fd: usize, len: usize) -> isize {
    syscall(
        SYSCALL_MMAP,
        [0, len, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0],
    )
}

fn munmap(addr: usize, len: usize) -> isize {
    syscall(SYSCALL_MUNMAP, [addr, len, 0, 0, 0, 0])
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let fd = open(
        "/tmp/shared_file_truncate_cache_smoke",
        RDWR | CREATE | TRUNC,
    );
    assert!(fd >= 0);
    let fd = fd as usize;

    let zeros = [0u8; PAGE_SIZE];
    assert_eq!(write(fd, &zeros), PAGE_SIZE as isize);
    assert_eq!(write(fd, &zeros), PAGE_SIZE as isize);

    let mut child_ready = [0usize; 2];
    let mut parent_done = [0usize; 2];
    assert_eq!(pipe(&mut child_ready), 0);
    assert_eq!(pipe(&mut parent_done), 0);

    let pid = fork();
    assert!(pid >= 0);
    if pid == 0 {
        close(child_ready[0]);
        close(parent_done[1]);

        let mapped = mmap_shared(fd, PAGE_SIZE * 2);
        assert!(mapped > 0);
        let mapped = mapped as *mut u8;

        // SAFETY: the shared mapping covers two writable pages. The write dirties
        // the second cached file page without writing it back to the inode.
        unsafe {
            assert_eq!(mapped.add(PAGE_SIZE + 64).read_volatile(), 0);
            mapped.add(PAGE_SIZE + 64).write_volatile(0x4a);
        }

        assert_eq!(write(child_ready[1], b"r"), 1);
        let mut done = [0u8; 1];
        assert_eq!(read(parent_done[0], &mut done), 1);

        assert_eq!(munmap(mapped as usize, PAGE_SIZE * 2), 0);
        close(child_ready[1]);
        close(parent_done[0]);
        close(fd);
        return 0;
    }

    close(child_ready[1]);
    close(parent_done[0]);

    let mut ready = [0u8; 1];
    assert_eq!(read(child_ready[0], &mut ready), 1);

    assert_eq!(ftruncate(fd, PAGE_SIZE), 0);
    assert_eq!(ftruncate(fd, PAGE_SIZE * 2), 0);

    let mapped = mmap_shared(fd, PAGE_SIZE * 2);
    assert!(mapped > 0);
    let mapped = mapped as *mut u8;

    // SAFETY: after shrink then regrow, the second file page is newly zeroed
    // file contents and must not reuse the child's stale cached dirty page.
    unsafe {
        assert_eq!(mapped.add(PAGE_SIZE + 64).read_volatile(), 0);
    }

    assert_eq!(munmap(mapped as usize, PAGE_SIZE * 2), 0);
    assert_eq!(write(parent_done[1], b"d"), 1);

    let mut exit_code = 0i32;
    assert_eq!(waitpid(pid, &mut exit_code), pid);
    assert_eq!(exit_code, 0);

    close(child_ready[0]);
    close(parent_done[1]);
    close(fd);

    println!("shared_file_truncate_cache_smoke passed");
    0
}
