#![no_std]
#![no_main]

#[macro_use]
extern crate user;

use user::syscall::{close, exit, fork, syscall, waitpid};

const SYSCALL_NANOSLEEP: usize = 101;
const SYSCALL_SCHED_YIELD: usize = 124;
const SYSCALL_CLONE3: usize = 435;
const SYSCALL_WAITID: usize = 95;

const CLONE_PIDFD: u64 = 0x0000_1000;
const SIGCHLD: u64 = 17;
const P_PIDFD: usize = 3;
const WEXITED: usize = 0x0000_0004;
const ECHILD: isize = -10;
const ITERATIONS: usize = 128;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct CloneArgs {
    flags: u64,
    pidfd: u64,
    child_tid: u64,
    parent_tid: u64,
    exit_signal: u64,
    stack: u64,
    stack_size: u64,
    tls: u64,
    set_tid: u64,
    set_tid_size: u64,
    cgroup: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct TimeSpec {
    sec: usize,
    nsec: usize,
}

fn linux_clone3(args: &CloneArgs) -> isize {
    syscall(
        SYSCALL_CLONE3,
        [
            args as *const CloneArgs as usize,
            core::mem::size_of::<CloneArgs>(),
            0,
            0,
            0,
            0,
        ],
    )
}

fn short_child_delay(iteration: usize) {
    match iteration % 3 {
        0 => {}
        1 => {
            let _ = syscall(SYSCALL_SCHED_YIELD, [0; 6]);
        }
        _ => {
            let delay = TimeSpec {
                sec: 0,
                nsec: 1_000_000,
            };
            let _ = syscall(
                SYSCALL_NANOSLEEP,
                [&delay as *const TimeSpec as usize, 0, 0, 0, 0, 0],
            );
        }
    }
}

fn wait_pidfd(pidfd: usize) -> isize {
    let mut info = [0u8; 128];
    syscall(
        SYSCALL_WAITID,
        [P_PIDFD, pidfd, info.as_mut_ptr() as usize, WEXITED, 0, 0],
    )
}

fn exercise_pidfd_waits() {
    for iteration in 0..ITERATIONS {
        let mut pidfd = -1i32;
        let args = CloneArgs {
            flags: CLONE_PIDFD,
            pidfd: (&mut pidfd as *mut i32) as u64,
            exit_signal: SIGCHLD,
            ..CloneArgs::default()
        };
        let child = linux_clone3(&args);
        if child == 0 {
            short_child_delay(iteration);
            exit(0);
        }
        assert!(child > 0, "clone3(CLONE_PIDFD): {child}");
        assert!(pidfd >= 0, "clone3 did not return a pidfd");
        assert_eq!(wait_pidfd(pidfd as usize), 0, "waitid(P_PIDFD)");
        assert_eq!(close(pidfd as usize), 0, "close(pidfd)");

        // waitid without WNOWAIT consumed the child exactly once.
        let mut status = -1;
        assert_eq!(waitpid(child, &mut status), ECHILD, "double reap");
    }
}

fn exercise_wait4() {
    for iteration in 0..ITERATIONS {
        let child = fork();
        if child == 0 {
            short_child_delay(iteration);
            exit(0);
        }
        assert!(child > 0, "fork: {child}");
        let mut status = -1;
        assert_eq!(waitpid(child, &mut status), child, "wait4");
        assert_eq!(status, 0, "child status");
    }
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    exercise_pidfd_waits();
    exercise_wait4();
    println!("WAIT_WAKEUP_RACE_PASS iterations={}", ITERATIONS * 2);
    0
}
