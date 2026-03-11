#![no_std]
#![no_main]

extern crate alloc;
#[macro_use]
extern crate user;

use alloc::{format, vec::Vec};
use user::syscall::{
    EPOLL_CTL_ADD, EPOLLIN, EpollEvent, RDONLY, close, epoll_create1, epoll_ctl, epoll_wait,
    fork, open, pipe, read, waitpid, write, _yield,
};

const EVENT_DATA: u64 = 0x3333_3333_3333_3333;

fn read_file(path: &str) -> Option<Vec<u8>> {
    let fd = open(path, RDONLY);
    if fd < 0 {
        return None;
    }
    let fd = fd as usize;
    let mut out = Vec::new();
    let mut buf = [0u8; 128];
    loop {
        let n = read(fd, &mut buf);
        if n <= 0 {
            break;
        }
        out.extend_from_slice(&buf[..n as usize]);
    }
    close(fd);
    Some(out)
}

fn proc_state(pid: isize) -> Option<u8> {
    let stat = read_file(format!("/proc/{pid}/stat").as_str())?;
    let stat = core::str::from_utf8(&stat).ok()?;
    let rparen = stat.rfind(')')?;
    stat.as_bytes().get(rparen + 2).copied()
}

fn wait_for_blocked_state(pid: isize) {
    for _ in 0..512 {
        if proc_state(pid) == Some(b'S') {
            return;
        }
        _yield();
    }
    panic!("child did not enter blocked epoll_wait state");
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let mut pipe_fd = [0usize; 2];
    assert_eq!(pipe(&mut pipe_fd), 0);
    let mut sync_fd = [0usize; 2];
    assert_eq!(pipe(&mut sync_fd), 0);

    let epfd = epoll_create1(0);
    assert!(epfd >= 0);
    let epfd = epfd as usize;

    let pid = fork();
    assert!(pid >= 0);
    if pid == 0 {
        close(pipe_fd[1]);
        close(sync_fd[0]);

        assert_eq!(write(sync_fd[1], b"r"), 1);
        close(sync_fd[1]);

        let mut events = [EpollEvent::default(); 1];
        assert_eq!(epoll_wait(epfd, &mut events, -1), 1);
        assert_eq!(events[0].data, EVENT_DATA);
        assert_ne!(events[0].events & EPOLLIN, 0);

        let mut byte = [0u8; 1];
        assert_eq!(read(pipe_fd[0], &mut byte), 1);
        assert_eq!(byte[0], b'y');

        close(pipe_fd[0]);
        close(epfd);
        return 0;
    }

    close(sync_fd[1]);
    let mut ready = [0u8; 1];
    assert_eq!(read(sync_fd[0], &mut ready), 1);
    assert_eq!(ready[0], b'r');
    close(sync_fd[0]);

    wait_for_blocked_state(pid);
    assert_eq!(write(pipe_fd[1], b"y"), 1);

    let event = EpollEvent {
        events: EPOLLIN,
        data: EVENT_DATA,
    };
    assert_eq!(epoll_ctl(epfd, EPOLL_CTL_ADD, pipe_fd[0], Some(&event)), 0);

    close(pipe_fd[1]);

    let mut exit_code = 0i32;
    assert_eq!(waitpid(pid, &mut exit_code), pid);
    assert_eq!(exit_code, 0);

    close(pipe_fd[0]);
    close(epfd);

    println!("epoll_ctl_wakeup_smoke passed");
    0
}
