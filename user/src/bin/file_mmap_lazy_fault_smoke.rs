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

fn assert_payload(mapped: *mut u8, payload: &[u8]) {
    for (idx, expected) in payload.iter().copied().enumerate() {
        // SAFETY: callers pass a successful shared mmap covering at least two
        // pages; the second page is validated through volatile reads so the
        // lazy fault path is exercised.
        let actual = unsafe { mapped.add(PAGE_SIZE + idx).read_volatile() };
        assert_eq!(actual, expected);
    }
    // SAFETY: Same mapping as above; the file-backed partial EOF page must
    // keep the byte after the pwrite payload zero-filled.
    assert_eq!(
        unsafe { mapped.add(PAGE_SIZE + payload.len()).read_volatile() },
        0
    );
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let fd = open("/tmp/file_mmap_lazy_fault_smoke", RDWR | CREATE | TRUNC);
    assert!(fd >= 0);
    let fd = fd as usize;

    assert_eq!(write(fd, b"A"), 1);
    let mapped = mmap_shared(fd, PAGE_SIZE * 3);
    assert!(mapped > 0);
    let mapped = mapped as *mut u8;

    let payload = b"lazy-file-fault";
    let mut start_pipe = [0usize; 2];
    assert_eq!(pipe(&mut start_pipe), 0);

    let pid = fork();
    assert!(pid >= 0);
    if pid == 0 {
        close(start_pipe[1]);
        let mut start = [0u8; 1];
        assert_eq!(read(start_pipe[0], &mut start), 1);
        assert_payload(mapped, payload);
        close(start_pipe[0]);
        return 0;
    }

    close(start_pipe[0]);
    assert_eq!(pwrite64(fd, payload, PAGE_SIZE), payload.len() as isize);
    assert_eq!(write(start_pipe[1], b"x"), 1);
    close(start_pipe[1]);

    let mut exit_code = 0i32;
    assert_eq!(waitpid(pid, &mut exit_code), pid);
    assert_eq!(exit_code, 0);

    assert_payload(mapped, payload);

    assert_eq!(munmap(mapped as usize, PAGE_SIZE * 3), 0);
    close(fd);

    println!("file_mmap_lazy_fault_smoke passed");
    0
}
