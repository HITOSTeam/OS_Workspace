#![no_std]
#![no_main]

#[macro_use]
extern crate user;

use user::syscall::{
    EPOLLET, EPOLL_CTL_ADD, EPOLLIN, EpollEvent, close, epoll_create1, epoll_ctl, epoll_wait,
    pipe, read, write,
};

const CHILD_A_DATA: u64 = 0xd1d1_d1d1_d1d1_d1d1;
const CHILD_B_DATA: u64 = 0xe2e2_e2e2_e2e2_e2e2;
const PARENT_A_DATA: u64 = 0xf3f3_f3f3_f3f3_f3f3;
const PARENT_B_DATA: u64 = 0xa4a4_a4a4_a4a4_a4a4;

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let mut pipe_a = [0usize; 2];
    let mut pipe_b = [0usize; 2];
    assert_eq!(pipe(&mut pipe_a), 0);
    assert_eq!(pipe(&mut pipe_b), 0);

    let child_a_epfd = epoll_create1(0);
    assert!(child_a_epfd >= 0);
    let child_a_epfd = child_a_epfd as usize;

    let child_b_epfd = epoll_create1(0);
    assert!(child_b_epfd >= 0);
    let child_b_epfd = child_b_epfd as usize;

    let parent_epfd = epoll_create1(0);
    assert!(parent_epfd >= 0);
    let parent_epfd = parent_epfd as usize;

    let child_a_event = EpollEvent {
        events: EPOLLIN | EPOLLET,
        data: CHILD_A_DATA,
    };
    assert_eq!(
        epoll_ctl(child_a_epfd, EPOLL_CTL_ADD, pipe_a[0], Some(&child_a_event)),
        0
    );

    let child_b_event = EpollEvent {
        events: EPOLLIN | EPOLLET,
        data: CHILD_B_DATA,
    };
    assert_eq!(
        epoll_ctl(child_b_epfd, EPOLL_CTL_ADD, pipe_b[0], Some(&child_b_event)),
        0
    );

    let parent_a_event = EpollEvent {
        events: EPOLLIN | EPOLLET,
        data: PARENT_A_DATA,
    };
    assert_eq!(
        epoll_ctl(parent_epfd, EPOLL_CTL_ADD, child_a_epfd, Some(&parent_a_event)),
        0
    );

    let parent_b_event = EpollEvent {
        events: EPOLLIN | EPOLLET,
        data: PARENT_B_DATA,
    };
    assert_eq!(
        epoll_ctl(parent_epfd, EPOLL_CTL_ADD, child_b_epfd, Some(&parent_b_event)),
        0
    );

    let mut events = [EpollEvent::default(); 1];
    let mut byte = [0u8; 1];

    assert_eq!(write(pipe_b[1], b"1"), 1);
    assert_eq!(epoll_wait(parent_epfd, &mut events, 5000), 1);
    assert_eq!(events[0].data, PARENT_B_DATA);
    assert_ne!(events[0].events & EPOLLIN, 0);
    assert_eq!(epoll_wait(parent_epfd, &mut events, 0), 0);

    assert_eq!(read(pipe_b[0], &mut byte), 1);
    assert_eq!(byte[0], b'1');

    // Child A becomes ready while child B has already transitioned back to
    // not-ready. With maxevents=1, parent epoll still needs to refresh child B's
    // ET snapshot, otherwise the next edge on child B can be lost.
    assert_eq!(write(pipe_a[1], b"a"), 1);
    assert_eq!(epoll_wait(parent_epfd, &mut events, 5000), 1);
    assert_eq!(events[0].data, PARENT_A_DATA);
    assert_ne!(events[0].events & EPOLLIN, 0);
    assert_eq!(epoll_wait(parent_epfd, &mut events, 0), 0);

    assert_eq!(read(pipe_a[0], &mut byte), 1);
    assert_eq!(byte[0], b'a');

    assert_eq!(write(pipe_b[1], b"2"), 1);
    assert_eq!(epoll_wait(parent_epfd, &mut events, 5000), 1);
    assert_eq!(events[0].data, PARENT_B_DATA);
    assert_ne!(events[0].events & EPOLLIN, 0);
    assert_eq!(epoll_wait(parent_epfd, &mut events, 0), 0);

    assert_eq!(read(pipe_b[0], &mut byte), 1);
    assert_eq!(byte[0], b'2');

    close(pipe_a[0]);
    close(pipe_a[1]);
    close(pipe_b[0]);
    close(pipe_b[1]);
    close(child_a_epfd);
    close(child_b_epfd);
    close(parent_epfd);

    println!("nested_epoll_et_maxevents_smoke passed");
    0
}
