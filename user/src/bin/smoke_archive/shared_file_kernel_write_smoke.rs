#![no_std]
#![no_main]

#[macro_use]
extern crate user;

use user::syscall::{CREATE, RDWR, TRUNC, close, fork, open, pipe, read, syscall, waitpid, write};

const PAGE_SIZE: usize = 4096;

const SYSCALL_COPY_FILE_RANGE: usize = 285;
const SYSCALL_MUNMAP: usize = 215;
const SYSCALL_MMAP: usize = 222;

const PROT_READ: usize = 1;
const PROT_WRITE: usize = 2;
const MAP_SHARED: usize = 0x01;

fn copy_file_range(
    in_fd: usize,
    in_off: &mut isize,
    out_fd: usize,
    out_off: &mut isize,
    len: usize,
) -> isize {
    syscall(
        SYSCALL_COPY_FILE_RANGE,
        [
            in_fd,
            in_off as *mut isize as usize,
            out_fd,
            out_off as *mut isize as usize,
            len,
            0,
        ],
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
    let src = open("/tmp/shared_file_kernel_write_src", RDWR | CREATE | TRUNC);
    let dst = open("/tmp/shared_file_kernel_write_dst", RDWR | CREATE | TRUNC);
    assert!(src >= 0 && dst >= 0);
    let src = src as usize;
    let dst = dst as usize;

    assert_eq!(write(src, &[0x6d]), 1);
    let zeros = [0u8; PAGE_SIZE];
    assert_eq!(write(dst, &zeros), PAGE_SIZE as isize);

    let mut ready_pipe = [0usize; 2];
    let mut go_pipe = [0usize; 2];
    assert_eq!(pipe(&mut ready_pipe), 0);
    assert_eq!(pipe(&mut go_pipe), 0);

    let pid = fork();
    assert!(pid >= 0);
    if pid == 0 {
        close(ready_pipe[0]);
        close(go_pipe[1]);

        let mapped = mmap_shared(dst, PAGE_SIZE);
        assert!(mapped > 0);
        let mapped = mapped as *mut u8;

        // SAFETY: the mapping covers one shared writable page. This read
        // materializes the target page before the parent writes via kernel I/O.
        unsafe {
            assert_eq!(mapped.add(192).read_volatile(), 0);
        }

        assert_eq!(write(ready_pipe[1], b"r"), 1);
        let mut go = [0u8; 1];
        assert_eq!(read(go_pipe[0], &mut go), 1);

        // SAFETY: parent copy_file_range wrote into the same inode offset.
        unsafe {
            assert_eq!(mapped.add(192).read_volatile(), 0x6d);
        }
        assert_eq!(munmap(mapped as usize, PAGE_SIZE), 0);
        close(ready_pipe[1]);
        close(go_pipe[0]);
        close(src);
        close(dst);
        return 0;
    }

    close(ready_pipe[1]);
    close(go_pipe[0]);

    let mut ready = [0u8; 1];
    assert_eq!(read(ready_pipe[0], &mut ready), 1);

    let mut in_off = 0isize;
    let mut out_off = 192isize;
    assert_eq!(copy_file_range(src, &mut in_off, dst, &mut out_off, 1), 1);
    assert_eq!(write(go_pipe[1], b"g"), 1);

    let mut exit_code = 0i32;
    assert_eq!(waitpid(pid, &mut exit_code), pid);
    assert_eq!(exit_code, 0);

    close(ready_pipe[0]);
    close(go_pipe[1]);
    close(src);
    close(dst);

    println!("shared_file_kernel_write_smoke passed");
    0
}
