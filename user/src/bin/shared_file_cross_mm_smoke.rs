#![no_std]
#![no_main]

#[macro_use]
extern crate user;

use user::syscall::{CREATE, RDWR, TRUNC, close, fork, open, pipe, read, syscall, waitpid, write};

const PAGE_SIZE: usize = 4096;

const SYSCALL_PWRITE64: usize = 68;
const SYSCALL_MUNMAP: usize = 215;
const SYSCALL_MMAP: usize = 222;

const PROT_READ: usize = 1;
const PROT_WRITE: usize = 2;
const MAP_SHARED: usize = 0x01;

fn pwrite64(fd: usize, buf: &[u8], off: usize) -> isize {
    syscall(
        SYSCALL_PWRITE64,
        [fd, buf.as_ptr() as usize, buf.len(), off, 0, 0],
    )
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
    let fd = open("/tmp/shared_file_cross_mm_smoke", RDWR | CREATE | TRUNC);
    assert!(fd >= 0);
    let fd = fd as usize;

    let zeros = [0u8; PAGE_SIZE];
    assert_eq!(write(fd, &zeros), PAGE_SIZE as isize);

    let mut ready_pipe = [0usize; 2];
    let mut go_pipe = [0usize; 2];
    assert_eq!(pipe(&mut ready_pipe), 0);
    assert_eq!(pipe(&mut go_pipe), 0);

    let pid = fork();
    assert!(pid >= 0);
    if pid == 0 {
        close(ready_pipe[0]);
        close(go_pipe[1]);

        let mapped = mmap_shared(fd, PAGE_SIZE);
        assert!(mapped > 0);
        let mapped = mapped as *mut u8;

        // SAFETY: the mapping covers one shared writable page. This first read
        // faults the page in before the parent writes through the fd.
        unsafe {
            assert_eq!(mapped.add(128).read_volatile(), 0);
        }

        assert_eq!(write(ready_pipe[1], b"r"), 1);
        let mut go = [0u8; 1];
        assert_eq!(read(go_pipe[0], &mut go), 1);

        // SAFETY: the parent wrote the same inode offset after this process
        // already had a resident MAP_SHARED page.
        unsafe {
            assert_eq!(mapped.add(128).read_volatile(), 0x5e);
        }
        assert_eq!(munmap(mapped as usize, PAGE_SIZE), 0);
        close(ready_pipe[1]);
        close(go_pipe[0]);
        close(fd);
        return 0;
    }

    close(ready_pipe[1]);
    close(go_pipe[0]);

    let mut ready = [0u8; 1];
    assert_eq!(read(ready_pipe[0], &mut ready), 1);

    assert_eq!(pwrite64(fd, &[0x5e], 128), 1);
    assert_eq!(write(go_pipe[1], b"g"), 1);

    let mut exit_code = 0i32;
    assert_eq!(waitpid(pid, &mut exit_code), pid);
    assert_eq!(exit_code, 0);

    close(ready_pipe[0]);
    close(go_pipe[1]);
    close(fd);

    println!("shared_file_cross_mm_smoke passed");
    0
}
