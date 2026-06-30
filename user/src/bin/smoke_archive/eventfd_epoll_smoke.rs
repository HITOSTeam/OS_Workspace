#![no_std]
#![no_main]

#[macro_use]
extern crate user;

use user::syscall::{
    EPOLL_CTL_ADD, EPOLL_CTL_MOD, EPOLLIN, EPOLLOUT, EpollEvent, close, epoll_create1, epoll_ctl,
    epoll_wait, eventfd, fork, pipe, read, waitpid, write,
};

const EVENTFD_DATA: u64 = 0x3333_3333_3333_3333;
const EPOLL_DATA: u64 = 0x4444_4444_4444_4444;

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let efd = eventfd(0, 0);
    assert!(efd >= 0);
    let efd = efd as usize;
    let mut start_fd = [0usize; 2];
    assert_eq!(pipe(&mut start_fd), 0);

    let epfd = epoll_create1(0);
    assert!(epfd >= 0);
    let epfd = epfd as usize;

    let event = EpollEvent {
        events: EPOLLIN | EPOLLOUT,
        data: EPOLL_DATA,
    };
    assert_eq!(epoll_ctl(epfd, EPOLL_CTL_ADD, efd, Some(&event)), 0);

    let mut events = [EpollEvent::default(); 1];
    assert_eq!(epoll_wait(epfd, &mut events, 0), 1);
    assert_eq!(events[0].data, EPOLL_DATA);
    assert_eq!(events[0].events & EPOLLIN, 0);
    assert_ne!(events[0].events & EPOLLOUT, 0);

    let read_event = EpollEvent {
        events: EPOLLIN,
        data: EPOLL_DATA,
    };
    assert_eq!(epoll_ctl(epfd, EPOLL_CTL_MOD, efd, Some(&read_event)), 0);

    let pid = fork();
    assert!(pid >= 0);
    if pid == 0 {
        close(start_fd[1]);
        let mut start = [0u8; 1];
        assert_eq!(read(start_fd[0], &mut start), 1);
        assert_eq!(start[0], b'g');
        close(start_fd[0]);

        let payload = EVENTFD_DATA.to_ne_bytes();
        assert_eq!(write(efd, &payload), payload.len() as isize);
        close(efd);
        close(epfd);
        return 0;
    }

    close(start_fd[0]);
    assert_eq!(write(start_fd[1], b"g"), 1);
    close(start_fd[1]);

    assert_eq!(epoll_wait(epfd, &mut events, 5000), 1);
    assert_eq!(events[0].data, EPOLL_DATA);
    assert_ne!(events[0].events & EPOLLIN, 0);

    let mut payload = [0u8; 8];
    assert_eq!(read(efd, &mut payload), payload.len() as isize);
    assert_eq!(u64::from_ne_bytes(payload), EVENTFD_DATA);

    let write_event = EpollEvent {
        events: EPOLLOUT,
        data: EPOLL_DATA,
    };
    assert_eq!(epoll_ctl(epfd, EPOLL_CTL_MOD, efd, Some(&write_event)), 0);
    assert_eq!(epoll_wait(epfd, &mut events, 0), 1);
    assert_eq!(events[0].data, EPOLL_DATA);
    assert_eq!(events[0].events & EPOLLIN, 0);
    assert_ne!(events[0].events & EPOLLOUT, 0);

    close(efd);
    close(epfd);
    let mut exit_code = 0i32;
    assert_eq!(waitpid(pid, &mut exit_code), pid);
    assert_eq!(exit_code, 0);

    println!(
        "eventfd_epoll_smoke passed: event_data={:#x} epoll_data={:#x}",
        EVENTFD_DATA, EPOLL_DATA
    );
    0
}
