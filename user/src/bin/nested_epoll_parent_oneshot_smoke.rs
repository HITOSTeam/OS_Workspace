#![no_std]
#![no_main]

#[macro_use]
extern crate user;

use user::syscall::{
    EPOLLONESHOT, EPOLL_CTL_ADD, EPOLL_CTL_MOD, EPOLLIN, EpollEvent, close, epoll_create1,
    epoll_ctl, epoll_wait, pipe, read, write,
};

const CHILD_DATA: u64 = 0xd1d1_d1d1_d1d1_d1d1;
const PARENT_DATA: u64 = 0xe2e2_e2e2_e2e2_e2e2;

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let mut pipe_fd = [0usize; 2];
    assert_eq!(pipe(&mut pipe_fd), 0);

    let child_epfd = epoll_create1(0);
    assert!(child_epfd >= 0);
    let child_epfd = child_epfd as usize;

    let parent_epfd = epoll_create1(0);
    assert!(parent_epfd >= 0);
    let parent_epfd = parent_epfd as usize;

    let child_event = EpollEvent {
        events: EPOLLIN,
        data: CHILD_DATA,
    };
    assert_eq!(
        epoll_ctl(child_epfd, EPOLL_CTL_ADD, pipe_fd[0], Some(&child_event)),
        0
    );

    let parent_event = EpollEvent {
        events: EPOLLIN | EPOLLONESHOT,
        data: PARENT_DATA,
    };
    assert_eq!(
        epoll_ctl(parent_epfd, EPOLL_CTL_ADD, child_epfd, Some(&parent_event)),
        0
    );

    let mut events = [EpollEvent::default(); 1];
    assert_eq!(epoll_wait(child_epfd, &mut events, 0), 0);
    assert_eq!(epoll_wait(parent_epfd, &mut events, 0), 0);

    assert_eq!(write(pipe_fd[1], b"a"), 1);

    assert_eq!(epoll_wait(parent_epfd, &mut events, 5000), 1);
    assert_eq!(events[0].data, PARENT_DATA);
    assert_ne!(events[0].events & EPOLLIN, 0);
    assert_eq!(epoll_wait(parent_epfd, &mut events, 0), 0);

    assert_eq!(epoll_wait(child_epfd, &mut events, 0), 1);
    assert_eq!(events[0].data, CHILD_DATA);
    assert_ne!(events[0].events & EPOLLIN, 0);

    let mut byte = [0u8; 1];
    assert_eq!(read(pipe_fd[0], &mut byte), 1);
    assert_eq!(byte[0], b'a');
    assert_eq!(epoll_wait(child_epfd, &mut events, 0), 0);
    assert_eq!(epoll_wait(parent_epfd, &mut events, 0), 0);

    assert_eq!(write(pipe_fd[1], b"b"), 1);

    // Parent ONESHOT must stay disabled even after the child epoll goes
    // through a full not-ready -> ready transition.
    assert_eq!(epoll_wait(child_epfd, &mut events, 0), 1);
    assert_eq!(events[0].data, CHILD_DATA);
    assert_ne!(events[0].events & EPOLLIN, 0);
    assert_eq!(epoll_wait(parent_epfd, &mut events, 0), 0);

    let rearm_event = EpollEvent {
        events: EPOLLIN | EPOLLONESHOT,
        data: PARENT_DATA,
    };
    assert_eq!(
        epoll_ctl(parent_epfd, EPOLL_CTL_MOD, child_epfd, Some(&rearm_event)),
        0
    );

    assert_eq!(epoll_wait(parent_epfd, &mut events, 5000), 1);
    assert_eq!(events[0].data, PARENT_DATA);
    assert_ne!(events[0].events & EPOLLIN, 0);
    assert_eq!(epoll_wait(parent_epfd, &mut events, 0), 0);

    assert_eq!(read(pipe_fd[0], &mut byte), 1);
    assert_eq!(byte[0], b'b');
    assert_eq!(epoll_wait(child_epfd, &mut events, 0), 0);
    assert_eq!(epoll_wait(parent_epfd, &mut events, 0), 0);

    close(pipe_fd[0]);
    close(pipe_fd[1]);
    close(child_epfd);
    close(parent_epfd);

    println!("nested_epoll_parent_oneshot_smoke passed");
    0
}
