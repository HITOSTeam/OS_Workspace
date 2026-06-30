#![no_std]
#![no_main]

extern crate alloc;

#[macro_use]
extern crate user;

use alloc::string::String;
use user::syscall::{CREATE, RDWR, TRUNC, close, open, syscall, write};

const SYSCALL_UNLINKAT: usize = 35;
const SYSCALL_FTRUNCATE: usize = 46;
const SYSCALL_OPENAT: usize = 56;
const SYSCALL_LSEEK: usize = 62;
const SYSCALL_PWRITE64: usize = 68;
const SYSCALL_FSTAT: usize = 80;
const SYSCALL_MUNMAP: usize = 215;
const SYSCALL_MMAP: usize = 222;
const SYSCALL_NEWFSTATAT: usize = 79;

const AT_FDCWD: isize = -100;
const O_RDWR: usize = 0x2;
const O_CREAT: usize = 0x40;
const O_TRUNC: usize = 0x200;
const O_APPEND: usize = 0x400;
const SEEK_END: usize = 2;
const PAGE_SIZE: usize = 4096;
const PROT_READ: usize = 1;
const PROT_WRITE: usize = 2;
const MAP_SHARED: usize = 0x01;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct KStat {
    st_dev: u64,
    st_ino: u64,
    st_mode: u32,
    st_nlink: u32,
    st_uid: u32,
    st_gid: u32,
    st_rdev: u64,
    __pad: u64,
    st_size: i64,
    st_blksize: u32,
    __pad2: i32,
    st_blocks: u64,
    st_atime_sec: i64,
    st_atime_nsec: i64,
    st_mtime_sec: i64,
    st_mtime_nsec: i64,
    st_ctime_sec: i64,
    st_ctime_nsec: i64,
    __unused: [u32; 2],
}

fn with_c_path<T>(path: &str, f: impl FnOnce(*const u8) -> T) -> T {
    let mut owned = String::from(path);
    owned.push('\0');
    f(owned.as_ptr())
}

fn linux_newfstatat(path: &str, st: &mut KStat) -> isize {
    with_c_path(path, |ptr| {
        syscall(
            SYSCALL_NEWFSTATAT,
            [
                AT_FDCWD as usize,
                ptr as usize,
                st as *mut KStat as usize,
                0,
                0,
                0,
            ],
        )
    })
}

fn linux_unlinkat(path: &str) -> isize {
    with_c_path(path, |ptr| {
        syscall(
            SYSCALL_UNLINKAT,
            [AT_FDCWD as usize, ptr as usize, 0, 0, 0, 0],
        )
    })
}

fn linux_openat(path: &str, flags: usize) -> isize {
    with_c_path(path, |ptr| {
        syscall(
            SYSCALL_OPENAT,
            [AT_FDCWD as usize, ptr as usize, flags, 0o666, 0, 0],
        )
    })
}

fn linux_fstat(fd: usize, st: &mut KStat) -> isize {
    syscall(SYSCALL_FSTAT, [fd, st as *mut KStat as usize, 0, 0, 0, 0])
}

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

fn lseek_end(fd: usize) -> isize {
    syscall(SYSCALL_LSEEK, [fd, 0, SEEK_END, 0, 0, 0])
}

fn ftruncate(fd: usize, len: usize) -> isize {
    syscall(SYSCALL_FTRUNCATE, [fd, len, 0, 0, 0, 0])
}

fn assert_path_size(path: &str, expected_size: usize) {
    let mut st = KStat::default();
    assert_eq!(linux_newfstatat(path, &mut st), 0);
    assert_eq!(st.st_size, expected_size as i64);
}

fn assert_fd_size(fd: usize, expected_size: usize) {
    let mut st = KStat::default();
    assert_eq!(linux_fstat(fd, &mut st), 0);
    assert_eq!(st.st_size, expected_size as i64);
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let pending_path = "/tmp/pending_write_stat_smoke.pending";
    let multi_fd_path = "/tmp/pending_write_stat_smoke.multi";
    let append_path = "/tmp/pending_write_stat_smoke.append";
    let zero_path = "/tmp/pending_write_stat_smoke.zero";
    let mmap_path = "/tmp/pending_write_stat_smoke.mmap";
    let trunc_path = "/tmp/pending_write_stat_smoke.trunc";
    let otrunc_path = "/tmp/pending_write_stat_smoke.otrunc";
    let pending_off = 64 * 1024 + 17;
    let multi_fd_off = 96 * 1024 + 7;
    let append_off = 112 * 1024 + 5;
    let zero_off = 128 * 1024 + 33;
    let mmap_off = PAGE_SIZE + 123;

    let _ = linux_unlinkat(pending_path);
    let _ = linux_unlinkat(multi_fd_path);
    let _ = linux_unlinkat(append_path);
    let _ = linux_unlinkat(zero_path);
    let _ = linux_unlinkat(mmap_path);
    let _ = linux_unlinkat(trunc_path);
    let _ = linux_unlinkat(otrunc_path);

    let fd = open(pending_path, RDWR | CREATE | TRUNC);
    assert!(fd >= 0);
    let fd = fd as usize;
    assert_eq!(pwrite64(fd, b"x", pending_off), 1);
    assert_path_size(pending_path, pending_off + 1);
    assert_eq!(lseek_end(fd), (pending_off + 1) as isize);
    assert_eq!(close(fd), 0);
    assert_path_size(pending_path, pending_off + 1);

    let fd1 = open(multi_fd_path, RDWR | CREATE | TRUNC);
    assert!(fd1 >= 0);
    let fd1 = fd1 as usize;
    let fd2 = open(multi_fd_path, RDWR);
    assert!(fd2 >= 0);
    let fd2 = fd2 as usize;
    assert_eq!(pwrite64(fd1, b"f", multi_fd_off), 1);
    assert_fd_size(fd2, multi_fd_off + 1);
    assert_eq!(lseek_end(fd2), (multi_fd_off + 1) as isize);
    assert_eq!(close(fd1), 0);
    assert_eq!(close(fd2), 0);
    assert_path_size(multi_fd_path, multi_fd_off + 1);

    let fd1 = linux_openat(append_path, O_RDWR | O_CREAT | O_TRUNC);
    assert!(fd1 >= 0);
    let fd1 = fd1 as usize;
    let fd2 = linux_openat(append_path, O_RDWR | O_APPEND);
    assert!(fd2 >= 0);
    let fd2 = fd2 as usize;
    assert_eq!(pwrite64(fd1, b"x", append_off), 1);
    let append_map_len = append_off + PAGE_SIZE;
    let append_map = mmap_shared(fd1, append_map_len);
    assert!(append_map > 0);
    let _ = unsafe { core::ptr::read_volatile((append_map as usize + append_off) as *const u8) };
    assert_eq!(write(fd2, b"a"), 1);
    let appended =
        unsafe { core::ptr::read_volatile((append_map as usize + append_off + 1) as *const u8) };
    assert_eq!(appended, b'a');
    assert_eq!(munmap(append_map as usize, append_map_len), 0);
    assert_fd_size(fd2, append_off + 2);
    assert_eq!(lseek_end(fd2), (append_off + 2) as isize);
    assert_eq!(close(fd1), 0);
    assert_eq!(close(fd2), 0);
    assert_path_size(append_path, append_off + 2);

    let fd = open(zero_path, RDWR | CREATE | TRUNC);
    assert!(fd >= 0);
    let fd = fd as usize;
    let empty: [u8; 0] = [];
    assert_eq!(pwrite64(fd, &empty, zero_off), 0);
    assert_path_size(zero_path, 0);
    assert_eq!(lseek_end(fd), 0);
    assert_eq!(close(fd), 0);
    assert_path_size(zero_path, 0);

    let fd = open(mmap_path, RDWR | CREATE | TRUNC);
    assert!(fd >= 0);
    let fd = fd as usize;
    assert_eq!(pwrite64(fd, b"m", 0), 1);
    let mapped = mmap_shared(fd, PAGE_SIZE * 2);
    assert!(mapped > 0);
    assert_eq!(pwrite64(fd, b"z", mmap_off), 1);
    assert_eq!(close(fd), 0);
    assert_path_size(mmap_path, mmap_off + 1);
    let trunc_fd = linux_openat(mmap_path, O_RDWR);
    assert!(trunc_fd >= 0);
    let trunc_fd = trunc_fd as usize;
    assert_eq!(ftruncate(trunc_fd, 0), 0);
    assert_eq!(close(trunc_fd), 0);
    assert_path_size(mmap_path, 0);
    assert_eq!(munmap(mapped as usize, PAGE_SIZE * 2), 0);
    assert_path_size(mmap_path, 0);

    let fd1 = open(trunc_path, RDWR | CREATE | TRUNC);
    assert!(fd1 >= 0);
    let fd1 = fd1 as usize;
    let fd2 = open(trunc_path, RDWR);
    assert!(fd2 >= 0);
    let fd2 = fd2 as usize;
    assert_eq!(pwrite64(fd1, b"t", pending_off), 1);
    assert_path_size(trunc_path, pending_off + 1);
    assert_eq!(ftruncate(fd2, 0), 0);
    assert_fd_size(fd1, 0);
    assert_eq!(lseek_end(fd1), 0);
    assert_eq!(close(fd2), 0);
    assert_eq!(close(fd1), 0);
    assert_path_size(trunc_path, 0);

    let fd1 = open(otrunc_path, RDWR | CREATE | TRUNC);
    assert!(fd1 >= 0);
    let fd1 = fd1 as usize;
    assert_eq!(pwrite64(fd1, b"o", pending_off), 1);
    assert_path_size(otrunc_path, pending_off + 1);
    let fd2 = linux_openat(otrunc_path, O_RDWR | O_TRUNC);
    assert!(fd2 >= 0);
    let fd2 = fd2 as usize;
    assert_fd_size(fd1, 0);
    assert_path_size(otrunc_path, 0);
    assert_eq!(close(fd2), 0);
    assert_eq!(close(fd1), 0);
    assert_path_size(otrunc_path, 0);

    let _ = linux_unlinkat(pending_path);
    let _ = linux_unlinkat(multi_fd_path);
    let _ = linux_unlinkat(append_path);
    let _ = linux_unlinkat(zero_path);
    let _ = linux_unlinkat(mmap_path);
    let _ = linux_unlinkat(trunc_path);
    let _ = linux_unlinkat(otrunc_path);

    println!("pending_write_stat_smoke passed");
    0
}
