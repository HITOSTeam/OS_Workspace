#![no_std]
#![no_main]

#[macro_use]
extern crate user;

use user::syscall::{
    EPOLL_CTL_ADD, EPOLL_CTL_DEL, EPOLLIN, EpollEvent, close, epoll_create1, epoll_ctl,
    epoll_wait, pipe, read, write,
};

const CHILD_DATA: u64 = 0x9191_9191_9191_9191;
const PARENT_DATA: u64 = 0xa2a2_a2a2_a2a2_a2a2;

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
        events: EPOLLIN,
        data: PARENT_DATA,
    };
    assert_eq!(
        epoll_ctl(parent_epfd, EPOLL_CTL_ADD, child_epfd, Some(&parent_event)),
        0
    );

    assert_eq!(write(pipe_fd[1], b"q"), 1);

    let mut events = [EpollEvent::default(); 1];
    assert_eq!(epoll_wait(child_epfd, &mut events, 0), 1);
    assert_eq!(events[0].data, CHILD_DATA);
    assert_ne!(events[0].events & EPOLLIN, 0);

    assert_eq!(epoll_wait(parent_epfd, &mut events, 0), 1);
    assert_eq!(events[0].data, PARENT_DATA);
    assert_ne!(events[0].events & EPOLLIN, 0);

    assert_eq!(epoll_ctl(child_epfd, EPOLL_CTL_DEL, pipe_fd[0], None), 0);
    assert_eq!(epoll_wait(child_epfd, &mut events, 0), 0);
    assert_eq!(epoll_wait(parent_epfd, &mut events, 0), 0);

    assert_eq!(
        epoll_ctl(child_epfd, EPOLL_CTL_ADD, pipe_fd[0], Some(&child_event)),
        0
    );
    assert_eq!(epoll_wait(child_epfd, &mut events, 0), 1);
    assert_eq!(events[0].data, CHILD_DATA);
    assert_eq!(epoll_wait(parent_epfd, &mut events, 0), 1);
    assert_eq!(events[0].data, PARENT_DATA);

    let mut byte = [0u8; 1];
    assert_eq!(read(pipe_fd[0], &mut byte), 1);
    assert_eq!(byte[0], b'q');

    close(pipe_fd[0]);
    close(pipe_fd[1]);
    close(child_epfd);
    close(parent_epfd);

    println!("nested_epoll_ctl_del_smoke passed");
    0
}
