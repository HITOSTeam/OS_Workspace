#![no_std]
#![no_main]

extern crate alloc;
#[macro_use]
extern crate user;

use alloc::vec;
use core::sync::atomic::{AtomicUsize, Ordering};
use user::syscall::{
    _yield, SIGPIPE, SignalAction, close, exit, fork, pipe, read, sigaction, sigreturn, syscall,
    waitpid, write,
};

const TOTAL: usize = 96 * 1024 + 123;
const READ_CHUNK: usize = 8191;
const EPIPE: isize = -32;
const SYSCALL_WRITE: usize = 64;
const SYSCALL_FCNTL: usize = 25;
const F_GETFL: usize = 3;
const F_SETFL: usize = 4;
const O_NONBLOCK: usize = 0x800;

static SIGPIPE_COUNT: AtomicUsize = AtomicUsize::new(0);

fn pattern(pos: usize) -> u8 {
    pos.wrapping_mul(131).wrapping_add(17) as u8
}

fn sigpipe_handler() {
    SIGPIPE_COUNT.fetch_add(1, Ordering::SeqCst);
    sigreturn();
}

fn install_sigpipe_handler() {
    let mut action = SignalAction::default();
    action.handler = sigpipe_handler as usize;
    assert_eq!(sigaction(SIGPIPE, Some(&action), None), 0);
}

fn fcntl(fd: usize, cmd: usize, arg: usize) -> isize {
    syscall(SYSCALL_FCNTL, [fd, cmd, arg, 0, 0, 0])
}

fn wait_for_sigpipe_count(target: usize) {
    for _ in 0..8 {
        if SIGPIPE_COUNT.load(Ordering::SeqCst) >= target {
            return;
        }
        _yield();
    }
}

fn check_large_blocking_write() {
    let mut fds = [0usize; 2];
    assert_eq!(pipe(&mut fds), 0);

    let pid = fork();
    assert!(pid >= 0);
    if pid == 0 {
        close(fds[1]);
        let mut buf = vec![0u8; READ_CHUNK];
        let mut seen = 0usize;
        while seen < TOTAL {
            let want = core::cmp::min(READ_CHUNK, TOTAL - seen);
            let n = read(fds[0], &mut buf[..want]);
            assert!(n > 0);
            let n = n as usize;
            for i in 0..n {
                assert_eq!(buf[i], pattern(seen + i));
            }
            seen += n;
        }

        let mut eof = [0u8; 1];
        assert_eq!(read(fds[0], &mut eof), 0);
        close(fds[0]);
        exit(0);
    }

    close(fds[0]);
    let mut data = vec![0u8; TOTAL];
    for (i, byte) in data.iter_mut().enumerate() {
        *byte = pattern(i);
    }
    assert_eq!(write(fds[1], data.as_slice()), TOTAL as isize);
    close(fds[1]);

    let mut exit_code = 0i32;
    assert_eq!(waitpid(pid, &mut exit_code), pid);
    assert_eq!(exit_code, 0);
}

fn check_nonblock_closed_reader_sigpipe() {
    SIGPIPE_COUNT.store(0, Ordering::SeqCst);

    let mut fds = [0usize; 2];
    assert_eq!(pipe(&mut fds), 0);
    let flags = fcntl(fds[1], F_GETFL, 0);
    assert!(flags >= 0);
    assert_eq!(fcntl(fds[1], F_SETFL, flags as usize | O_NONBLOCK), 0);
    close(fds[0]);

    assert_eq!(write(fds[1], &[0x5a]), EPIPE);
    wait_for_sigpipe_count(1);
    assert_eq!(SIGPIPE_COUNT.load(Ordering::SeqCst), 1);

    assert_eq!(
        syscall(SYSCALL_WRITE, [fds[1], usize::MAX - 4095, 1, 0, 0, 0]),
        EPIPE
    );
    wait_for_sigpipe_count(2);
    assert_eq!(SIGPIPE_COUNT.load(Ordering::SeqCst), 2);
    close(fds[1]);
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    install_sigpipe_handler();
    check_large_blocking_write();
    check_nonblock_closed_reader_sigpipe();

    println!("pipe_large_write_smoke passed");
    0
}
