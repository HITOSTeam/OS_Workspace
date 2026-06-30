#![no_std]
#![no_main]

extern crate alloc;
#[macro_use]
extern crate user;

use alloc::format;
use user::syscall::{
    EPOLL_CTL_ADD, EPOLLIN, EpollEvent, MQ_O_CREAT, MQ_O_EXCL, MqAttr, RDONLY, WRONLY, close,
    epoll_create1, epoll_ctl, epoll_wait, fork, getpid, mq_getattr, mq_open, mq_timedreceive,
    mq_timedsend, mq_unlink, pipe, read, waitpid, write,
};

const EPOLL_DATA: u64 = 0x6666_6666_6666_6666;
const MQ_PRIO: u32 = 7;
const MQ_MODE: usize = 0o600;
const MQ_MSG: &[u8] = b"mq-epoll";

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let queue_name = format!("/mq_epoll_smoke_{}", getpid());
    let attr = MqAttr {
        mq_maxmsg: 4,
        mq_msgsize: 32,
        ..MqAttr::default()
    };

    let mqd = mq_open(
        &queue_name,
        RDONLY | MQ_O_CREAT | MQ_O_EXCL,
        MQ_MODE,
        Some(&attr),
    );
    assert!(mqd >= 0);
    let mqd = mqd as usize;

    let epfd = epoll_create1(0);
    assert!(epfd >= 0);
    let epfd = epfd as usize;

    let event = EpollEvent {
        events: EPOLLIN,
        data: EPOLL_DATA,
    };
    assert_eq!(epoll_ctl(epfd, EPOLL_CTL_ADD, mqd, Some(&event)), 0);

    let mut events = [EpollEvent::default(); 1];
    assert_eq!(epoll_wait(epfd, &mut events, 0), 0);

    let mut start_fd = [0usize; 2];
    assert_eq!(pipe(&mut start_fd), 0);

    let pid = fork();
    assert!(pid >= 0);
    if pid == 0 {
        close(start_fd[1]);
        let mut start = [0u8; 1];
        assert_eq!(read(start_fd[0], &mut start), 1);
        assert_eq!(start[0], b'g');
        close(start_fd[0]);

        let writer = mq_open(&queue_name, WRONLY, 0, None);
        assert!(writer >= 0);
        let writer = writer as usize;
        assert_eq!(mq_timedsend(writer, MQ_MSG, MQ_PRIO, None), 0);
        close(writer);
        return 0;
    }

    close(start_fd[0]);
    assert_eq!(write(start_fd[1], b"g"), 1);
    close(start_fd[1]);

    assert_eq!(epoll_wait(epfd, &mut events, 5000), 1);
    assert_eq!(events[0].data, EPOLL_DATA);
    assert_ne!(events[0].events & EPOLLIN, 0);

    let mut attr_after = MqAttr::default();
    assert_eq!(mq_getattr(mqd, &mut attr_after), 0);
    assert_eq!(attr_after.mq_curmsgs, 1);
    assert_eq!(attr_after.mq_msgsize, attr.mq_msgsize);

    let mut recv_buf = [0u8; 32];
    let mut recv_prio = 0u32;
    let recv_len = mq_timedreceive(mqd, &mut recv_buf, Some(&mut recv_prio), None);
    assert_eq!(recv_len, MQ_MSG.len() as isize);
    assert_eq!(&recv_buf[..recv_len as usize], MQ_MSG);
    assert_eq!(recv_prio, MQ_PRIO);
    assert_eq!(epoll_wait(epfd, &mut events, 0), 0);

    close(mqd);
    assert_eq!(mq_unlink(&queue_name), 0);
    close(epfd);

    let mut exit_code = 0i32;
    assert_eq!(waitpid(pid, &mut exit_code), pid);
    assert_eq!(exit_code, 0);

    println!("mq_epoll_smoke passed");
    0
}
