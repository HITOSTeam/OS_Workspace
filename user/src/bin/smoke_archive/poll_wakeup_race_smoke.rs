#![no_std]
#![no_main]

#[macro_use]
extern crate user;

use user::syscall::{
    _yield, EPOLL_CTL_ADD, EPOLLIN, EpollEvent, close, epoll_create1, epoll_ctl, epoll_wait, exit,
    fork, pipe, read, sleep, syscall, waitpid, write,
};

const SYSCALL_PPOLL: usize = 73;
const POLLIN: i16 = 0x001;
const ITERATIONS: usize = 64;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct PollFd {
    fd: i32,
    events: i16,
    revents: i16,
}

fn child_delay(iteration: usize) {
    match iteration % 3 {
        0 => {}
        1 => _yield(),
        _ => sleep(1),
    }
}

fn wait_child(pid: isize) {
    let mut status = -1;
    assert_eq!(waitpid(pid, &mut status), pid);
    assert_eq!(status, 0);
}

fn exercise_ppoll() {
    for iteration in 0..ITERATIONS {
        let mut fds = [0usize; 2];
        assert_eq!(pipe(&mut fds), 0);
        let pid = fork();
        assert!(pid >= 0);
        if pid == 0 {
            close(fds[0]);
            child_delay(iteration);
            assert_eq!(write(fds[1], b"p"), 1);
            close(fds[1]);
            exit(0);
        }

        close(fds[1]);
        let mut pfd = PollFd {
            fd: fds[0] as i32,
            events: POLLIN,
            revents: 0,
        };
        assert_eq!(
            syscall(
                SYSCALL_PPOLL,
                [&mut pfd as *mut PollFd as usize, 1, 0, 0, 0, 0],
            ),
            1,
            "ppoll iteration {iteration}"
        );
        assert_ne!(pfd.revents & POLLIN, 0);
        let mut byte = [0u8; 1];
        assert_eq!(read(fds[0], &mut byte), 1);
        assert_eq!(byte[0], b'p');
        close(fds[0]);
        wait_child(pid);
    }
}

fn exercise_epoll() {
    for iteration in 0..ITERATIONS {
        let mut fds = [0usize; 2];
        assert_eq!(pipe(&mut fds), 0);
        let epfd = epoll_create1(0);
        assert!(epfd >= 0);
        let epfd = epfd as usize;
        let event = EpollEvent {
            events: EPOLLIN,
            data: iteration as u64,
        };
        assert_eq!(epoll_ctl(epfd, EPOLL_CTL_ADD, fds[0], Some(&event)), 0);

        let pid = fork();
        assert!(pid >= 0);
        if pid == 0 {
            close(fds[0]);
            child_delay(iteration);
            assert_eq!(write(fds[1], b"e"), 1);
            close(fds[1]);
            close(epfd);
            exit(0);
        }

        close(fds[1]);
        let mut events = [EpollEvent::default(); 1];
        assert_eq!(
            epoll_wait(epfd, &mut events, -1),
            1,
            "epoll iteration {iteration}"
        );
        assert_eq!(events[0].data, iteration as u64);
        let mut byte = [0u8; 1];
        assert_eq!(read(fds[0], &mut byte), 1);
        assert_eq!(byte[0], b'e');
        close(fds[0]);
        close(epfd);
        wait_child(pid);
    }
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    exercise_ppoll();
    exercise_epoll();
    println!(
        "POLL_WAKEUP_RACE_PASS ppoll={} epoll={}",
        ITERATIONS, ITERATIONS
    );
    0
}
