#![no_std]
#![no_main]

extern crate alloc;
#[macro_use]
extern crate user;

use alloc::{format, vec::Vec};
use user::syscall::{
    _yield, EPOLL_CTL_ADD, EPOLLIN, EpollEvent, RDONLY, close, epoll_create1, epoll_ctl,
    epoll_wait, fork, getpid, open, pipe, read, waitpid, write,
};

const CHILD_DATA: u64 = 0x5151_5151_5151_5151;
const PARENT_DATA: u64 = 0x6262_6262_6262_6262;

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
    panic!("parent did not enter blocked nested epoll_wait state");
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let parent_pid = getpid();

    let mut pipe_fd = [0usize; 2];
    assert_eq!(pipe(&mut pipe_fd), 0);
    let mut start_fd = [0usize; 2];
    assert_eq!(pipe(&mut start_fd), 0);

    let child_epfd = epoll_create1(0);
    assert!(child_epfd >= 0);
    let child_epfd = child_epfd as usize;

    let parent_epfd = epoll_create1(0);
    assert!(parent_epfd >= 0);
    let parent_epfd = parent_epfd as usize;

    let parent_event = EpollEvent {
        events: EPOLLIN,
        data: PARENT_DATA,
    };
    assert_eq!(
        epoll_ctl(parent_epfd, EPOLL_CTL_ADD, child_epfd, Some(&parent_event)),
        0
    );

    let pid = fork();
    assert!(pid >= 0);
    if pid == 0 {
        close(start_fd[1]);

        let mut start = [0u8; 1];
        assert_eq!(read(start_fd[0], &mut start), 1);
        assert_eq!(start[0], b'g');
        close(start_fd[0]);

        wait_for_blocked_state(parent_pid);
        assert_eq!(write(pipe_fd[1], b"z"), 1);

        let child_event = EpollEvent {
            events: EPOLLIN,
            data: CHILD_DATA,
        };
        assert_eq!(
            epoll_ctl(child_epfd, EPOLL_CTL_ADD, pipe_fd[0], Some(&child_event)),
            0
        );

        close(pipe_fd[1]);
        close(pipe_fd[0]);
        close(child_epfd);
        close(parent_epfd);
        return 0;
    }

    close(pipe_fd[1]);
    close(start_fd[0]);
    assert_eq!(write(start_fd[1], b"g"), 1);
    close(start_fd[1]);

    let mut events = [EpollEvent::default(); 2];
    assert_eq!(epoll_wait(parent_epfd, &mut events[..1], -1), 1);
    assert_eq!(events[0].data, PARENT_DATA);
    assert_ne!(events[0].events & EPOLLIN, 0);

    assert_eq!(epoll_wait(child_epfd, &mut events[..1], 0), 1);
    assert_eq!(events[0].data, CHILD_DATA);
    assert_ne!(events[0].events & EPOLLIN, 0);

    let mut byte = [0u8; 1];
    assert_eq!(read(pipe_fd[0], &mut byte), 1);
    assert_eq!(byte[0], b'z');

    close(pipe_fd[0]);
    close(child_epfd);
    close(parent_epfd);

    let mut exit_code = 0i32;
    assert_eq!(waitpid(pid, &mut exit_code), pid);
    assert_eq!(exit_code, 0);

    println!("nested_epoll_ctl_wakeup_smoke passed");
    0
}
