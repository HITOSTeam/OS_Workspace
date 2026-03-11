#![no_std]
#![no_main]

#[macro_use]
extern crate user;

use user::syscall::{
    EPOLL_CTL_ADD, EPOLLIN, EpollEvent, close, epoll_create1, epoll_ctl, epoll_wait, fork, pipe,
    read, sleep, waitpid, write,
};

const EVENT_DATA: u64 = 0x3333_3333_3333_3333;

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let mut pipe_fd = [0usize; 2];
    assert_eq!(pipe(&mut pipe_fd), 0);

    let epfd = epoll_create1(0);
    assert!(epfd >= 0);
    let epfd = epfd as usize;

    let pid = fork();
    assert!(pid >= 0);
    if pid == 0 {
        close(pipe_fd[1]);

        let mut events = [EpollEvent::default(); 1];
        assert_eq!(epoll_wait(epfd, &mut events, 5000), 1);
        assert_eq!(events[0].data, EVENT_DATA);
        assert_ne!(events[0].events & EPOLLIN, 0);

        let mut byte = [0u8; 1];
        assert_eq!(read(pipe_fd[0], &mut byte), 1);
        assert_eq!(byte[0], b'y');

        close(pipe_fd[0]);
        close(epfd);
        return 0;
    }

    sleep(50);
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
