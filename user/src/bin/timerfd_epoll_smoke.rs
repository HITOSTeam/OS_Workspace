#![no_std]
#![no_main]

#[macro_use]
extern crate user;

use user::syscall::{
    CLOCK_MONOTONIC, EPOLL_CTL_ADD, EPOLLIN, EpollEvent, ITimerSpec, TimeSpec, close,
    epoll_create1, epoll_ctl, epoll_wait, read, sleep, timerfd_create, timerfd_gettime,
    timerfd_settime,
};

const EPOLL_DATA: u64 = 0x5555_5555_5555_5555;

fn timespec_from_millis(ms: i64) -> TimeSpec {
    TimeSpec {
        sec: ms / 1000,
        nsec: (ms % 1000) * 1_000_000,
    }
}

fn disarmed() -> ITimerSpec {
    ITimerSpec::default()
}

fn oneshot_after(ms: i64) -> ITimerSpec {
    ITimerSpec {
        it_interval: TimeSpec::default(),
        it_value: timespec_from_millis(ms),
    }
}

fn periodic_after(initial_ms: i64, interval_ms: i64) -> ITimerSpec {
    ITimerSpec {
        it_interval: timespec_from_millis(interval_ms),
        it_value: timespec_from_millis(initial_ms),
    }
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let tfd = timerfd_create(CLOCK_MONOTONIC, 0);
    assert!(tfd >= 0);
    let tfd = tfd as usize;

    let epfd = epoll_create1(0);
    assert!(epfd >= 0);
    let epfd = epfd as usize;

    let event = EpollEvent {
        events: EPOLLIN,
        data: EPOLL_DATA,
    };
    assert_eq!(epoll_ctl(epfd, EPOLL_CTL_ADD, tfd, Some(&event)), 0);

    let mut events = [EpollEvent::default(); 1];
    assert_eq!(epoll_wait(epfd, &mut events, 0), 0);

    let oneshot = oneshot_after(50);
    let mut old_value = ITimerSpec::default();
    assert_eq!(timerfd_settime(tfd, 0, &oneshot, Some(&mut old_value)), 0);
    assert_eq!(old_value, disarmed());

    let mut current = ITimerSpec::default();
    assert_eq!(timerfd_gettime(tfd, &mut current), 0);
    assert_eq!(current.it_interval, TimeSpec::default());
    assert!(current.it_value.sec > 0 || current.it_value.nsec > 0);

    assert_eq!(epoll_wait(epfd, &mut events, 5000), 1);
    assert_eq!(events[0].data, EPOLL_DATA);
    assert_ne!(events[0].events & EPOLLIN, 0);

    let mut expirations = [0u8; 8];
    assert_eq!(read(tfd, &mut expirations), expirations.len() as isize);
    assert_eq!(u64::from_ne_bytes(expirations), 1);
    assert_eq!(epoll_wait(epfd, &mut events, 0), 0);

    let periodic = periodic_after(20, 20);
    assert_eq!(timerfd_settime(tfd, 0, &periodic, None), 0);
    assert_eq!(epoll_wait(epfd, &mut events, 5000), 1);
    sleep(70);
    assert_eq!(read(tfd, &mut expirations), expirations.len() as isize);
    assert!(u64::from_ne_bytes(expirations) >= 1);

    let disarmed = disarmed();
    assert_eq!(timerfd_settime(tfd, 0, &disarmed, None), 0);
    sleep(30);
    assert_eq!(timerfd_gettime(tfd, &mut current), 0);
    assert_eq!(current, ITimerSpec::default());
    assert_eq!(epoll_wait(epfd, &mut events, 0), 0);

    close(tfd);
    close(epfd);
    println!("timerfd_epoll_smoke passed");
    0
}
