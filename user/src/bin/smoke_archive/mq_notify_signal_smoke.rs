#![no_std]
#![no_main]

extern crate alloc;
#[macro_use]
extern crate user;

use alloc::format;
use core::sync::atomic::{AtomicUsize, Ordering};
use user::syscall::{
    _yield, MQ_O_CREAT, MQ_O_EXCL, MqAttr, RDONLY, SIGEV_SIGNAL, SIGUSR1, Sigevent, SignalAction,
    WRONLY, close, getpid, mq_notify, mq_open, mq_timedreceive, mq_timedsend, mq_unlink, sigaction,
    sigreturn, sleep,
};

const EBUSY: isize = -16;
const MQ_MODE: usize = 0o600;
const MQ_MSG_1: &[u8] = b"notify-1";
const MQ_MSG_2: &[u8] = b"notify-2";
const MQ_MSG_3: &[u8] = b"notify-3";
const WAIT_ROUNDS: usize = 100;
const WAIT_SLICE_MS: usize = 10;

static SIGNAL_COUNT: AtomicUsize = AtomicUsize::new(0);

fn notify_handler() {
    SIGNAL_COUNT.fetch_add(1, Ordering::SeqCst);
    sigreturn();
}

fn install_sigusr1_handler() {
    let mut new_action = SignalAction::default();
    let mut old_action = SignalAction::default();
    new_action.handler = notify_handler as usize;
    assert_eq!(
        sigaction(SIGUSR1, Some(&new_action), Some(&mut old_action)),
        0
    );
}

fn mq_signal_event() -> Sigevent {
    Sigevent {
        sigev_value: 0x1234_5678,
        sigev_signo: SIGUSR1,
        sigev_notify: SIGEV_SIGNAL,
        sigev_data: [0; 2],
    }
}

fn wait_for_signal_count(target: usize) -> bool {
    for _ in 0..WAIT_ROUNDS {
        if SIGNAL_COUNT.load(Ordering::SeqCst) >= target {
            return true;
        }
        _yield();
        sleep(WAIT_SLICE_MS);
    }
    false
}

fn assert_no_new_signal(expected: usize) {
    for _ in 0..WAIT_ROUNDS {
        _yield();
        sleep(WAIT_SLICE_MS);
        assert_eq!(SIGNAL_COUNT.load(Ordering::SeqCst), expected);
    }
}

fn recv_and_check(mqd: usize, expected: &[u8]) {
    let mut recv_buf = [0u8; 64];
    let recv_len = mq_timedreceive(mqd, &mut recv_buf, None, None);
    assert_eq!(recv_len, expected.len() as isize);
    assert_eq!(&recv_buf[..recv_len as usize], expected);
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    install_sigusr1_handler();
    SIGNAL_COUNT.store(0, Ordering::SeqCst);

    let queue_name = format!("/mq_notify_signal_{}", getpid());
    let attr = MqAttr {
        mq_maxmsg: 4,
        mq_msgsize: 64,
        ..MqAttr::default()
    };

    let reader = mq_open(
        &queue_name,
        RDONLY | MQ_O_CREAT | MQ_O_EXCL,
        MQ_MODE,
        Some(&attr),
    );
    assert!(reader >= 0);
    let reader = reader as usize;

    let writer = mq_open(&queue_name, WRONLY, 0, None);
    assert!(writer >= 0);
    let writer = writer as usize;

    let event = mq_signal_event();
    assert_eq!(mq_notify(reader, Some(&event)), 0);
    assert_eq!(mq_notify(reader, Some(&event)), EBUSY);

    assert_eq!(mq_timedsend(writer, MQ_MSG_1, 1, None), 0);
    assert!(wait_for_signal_count(1));
    recv_and_check(reader, MQ_MSG_1);

    assert_eq!(mq_timedsend(writer, MQ_MSG_2, 1, None), 0);
    assert_no_new_signal(1);
    recv_and_check(reader, MQ_MSG_2);

    assert_eq!(mq_notify(reader, Some(&event)), 0);
    assert_eq!(mq_notify(reader, None), 0);
    assert_eq!(mq_timedsend(writer, MQ_MSG_3, 1, None), 0);
    assert_no_new_signal(1);
    recv_and_check(reader, MQ_MSG_3);

    close(writer);
    close(reader);
    assert_eq!(mq_unlink(&queue_name), 0);

    println!("mq_notify_signal_smoke passed");
    0
}
