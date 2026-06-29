#![no_std]
#![no_main]

extern crate alloc;

#[macro_use]
extern crate user;

use alloc::string::String;
use user::syscall::{CREATE, RDONLY, RDWR, TRUNC, TimeSpec, close, open, syscall, write};

const SYSCALL_OPENAT: usize = 56;
const SYSCALL_PSELECT6: usize = 72;
const SYSCALL_PPOLL: usize = 73;

const AT_FDCWD: isize = -100;
const O_PATH: usize = 0x200000;

const POLLIN: i16 = 0x0001;
const POLLOUT: i16 = 0x0004;
const POLLNVAL: i16 = 0x0020;

#[repr(C)]
#[derive(Clone, Copy)]
struct PollFd {
    fd: i32,
    events: i16,
    revents: i16,
}

fn set_fd(fdset: &mut [u8], fd: usize) {
    fdset[fd / 8] |= 1u8 << (fd % 8);
}

fn is_fd_set(fdset: &[u8], fd: usize) -> bool {
    (fdset[fd / 8] & (1u8 << (fd % 8))) != 0
}

fn with_c_path<T>(path: &str, f: impl FnOnce(*const u8) -> T) -> T {
    let mut owned = String::from(path);
    owned.push('\0');
    f(owned.as_ptr())
}

fn linux_openat(dirfd: isize, path: &str, flags: usize, mode: usize) -> isize {
    with_c_path(path, |ptr| {
        syscall(
            SYSCALL_OPENAT,
            [dirfd as usize, ptr as usize, flags, mode, 0, 0],
        )
    })
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let path = "/tmp/regular_file_select_smoke";
    let fd = open(path, RDWR | CREATE | TRUNC);
    assert!(fd >= 0);
    let fd = fd as usize;
    assert_eq!(write(fd, b"x"), 1);
    close(fd);

    let fd = open(path, RDONLY);
    assert!(fd >= 0);
    let fd = fd as usize;

    let nfds = fd + 1;
    let mut writefds = [0u8; 32];
    assert!(nfds <= writefds.len() * 8);
    set_fd(&mut writefds, fd);
    let timeout = TimeSpec { sec: 0, nsec: 0 };

    let ready = syscall(
        SYSCALL_PSELECT6,
        [
            nfds,
            0,
            writefds.as_mut_ptr() as usize,
            0,
            &timeout as *const TimeSpec as usize,
            0,
        ],
    );
    assert_eq!(ready, 1);
    assert!(is_fd_set(&writefds, fd));

    close(fd);

    let fd = linux_openat(AT_FDCWD, path, O_PATH, 0);
    assert!(fd >= 0);
    let fd = fd as usize;
    let mut pfd = PollFd {
        fd: fd as i32,
        events: POLLIN | POLLOUT,
        revents: 0,
    };
    let timeout = TimeSpec { sec: 0, nsec: 0 };
    let ready = syscall(
        SYSCALL_PPOLL,
        [
            &mut pfd as *mut PollFd as usize,
            1,
            &timeout as *const TimeSpec as usize,
            0,
            0,
            0,
        ],
    );
    assert_eq!(ready, 1);
    assert_eq!(pfd.revents, POLLNVAL);

    close(fd);
    println!("regular_file_select_smoke passed");
    0
}
