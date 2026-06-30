#![no_std]
#![no_main]

extern crate alloc;
#[macro_use]
extern crate user;

use alloc::format;
use user::syscall::{
    EPOLL_CTL_ADD, EPOLLIN, EpollEvent, MQ_O_CREAT, MQ_O_EXCL, MqAttr, RDONLY, WRONLY, close,
    epoll_create1, epoll_ctl, epoll_wait, getpid, mq_open, mq_timedreceive, mq_timedsend,
    mq_unlink,
};

const EPOLL_DATA: u64 = 0x7777_7777_7777_7777;
const ENOENT: isize = -2;
const MQ_MSG: &[u8] = b"unlink-still-live";

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let queue_name = format!("/mq_unlink_epoll_{}", getpid());
    let attr = MqAttr {
        mq_maxmsg: 4,
        mq_msgsize: 64,
        ..MqAttr::default()
    };

    let reader = mq_open(
        &queue_name,
        RDONLY | MQ_O_CREAT | MQ_O_EXCL,
        0o600,
        Some(&attr),
    );
    assert!(reader >= 0);
    let reader = reader as usize;

    let writer = mq_open(&queue_name, WRONLY, 0, None);
    assert!(writer >= 0);
    let writer = writer as usize;

    let epfd = epoll_create1(0);
    assert!(epfd >= 0);
    let epfd = epfd as usize;

    let event = EpollEvent {
        events: EPOLLIN,
        data: EPOLL_DATA,
    };
    assert_eq!(epoll_ctl(epfd, EPOLL_CTL_ADD, reader, Some(&event)), 0);
    let mut events = [EpollEvent::default(); 1];
    assert_eq!(epoll_wait(epfd, &mut events, 0), 0);

    assert_eq!(mq_unlink(&queue_name), 0);
    assert_eq!(mq_open(&queue_name, RDONLY, 0, None), ENOENT);

    assert_eq!(mq_timedsend(writer, MQ_MSG, 1, None), 0);
    assert_eq!(epoll_wait(epfd, &mut events, 5000), 1);
    assert_eq!(events[0].data, EPOLL_DATA);
    assert_ne!(events[0].events & EPOLLIN, 0);

    let mut recv_buf = [0u8; 64];
    let recv_len = mq_timedreceive(reader, &mut recv_buf, None, None);
    assert_eq!(recv_len, MQ_MSG.len() as isize);
    assert_eq!(&recv_buf[..recv_len as usize], MQ_MSG);
    assert_eq!(epoll_wait(epfd, &mut events, 0), 0);

    close(writer);
    close(reader);
    close(epfd);
    println!("mq_unlink_epoll_smoke passed");
    0
}
