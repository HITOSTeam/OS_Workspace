#![no_std]
#![no_main]

extern crate user;

use user::println;
use user::syscall::{RDWR, close, dup3, execve, exit, fork, open, syscall, waitpid};

const SYSCALL_CLOCK_GETTIME: usize = 113;
const CLOCK_MONOTONIC: usize = 1;
const WORKERS: usize = 2;
const MEASURED_ROUNDS: usize = 3;

const RUSTC: &str = "/root/.cargo/bin/rustc\0";
const ARG0: &str = "rustc\0";
const ARG1: &str = "-vV\0";
const ENV_HOME: &str = "HOME=/root\0";
const ENV_RUSTUP_HOME: &str = "RUSTUP_HOME=/root/.rustup\0";
const ENV_CARGO_HOME: &str = "CARGO_HOME=/root/.cargo\0";
const ENV_TOOLCHAIN: &str = "RUSTUP_TOOLCHAIN=nightly-2026-05-28\0";
const ENV_PATH: &str = "PATH=/root/.cargo/bin:/usr/local/bin:/usr/bin:/bin:/sbin:/usr/sbin\0";

#[repr(C)]
#[derive(Default)]
struct Timespec {
    sec: i64,
    nsec: i64,
}

fn monotonic_ns() -> u64 {
    let mut now = Timespec::default();
    assert_eq!(
        syscall(
            SYSCALL_CLOCK_GETTIME,
            [
                CLOCK_MONOTONIC,
                &mut now as *mut Timespec as usize,
                0,
                0,
                0,
                0,
            ],
        ),
        0
    );
    (now.sec as u64)
        .saturating_mul(1_000_000_000)
        .saturating_add(now.nsec as u64)
}

fn run_batch() -> usize {
    let mut pids = [0isize; WORKERS];
    for (index, slot) in pids.iter_mut().enumerate() {
        let pid = fork();
        if pid == 0 {
            let null_fd = open("/dev/null", RDWR);
            if null_fd < 0 {
                exit(120);
            }
            let null_fd = null_fd as usize;
            if dup3(null_fd, 1, 0) != 1 || dup3(null_fd, 2, 0) != 2 {
                exit(121);
            }
            if null_fd > 2 {
                let _ = close(null_fd);
            }
            let args = [ARG0.as_ptr(), ARG1.as_ptr(), core::ptr::null()];
            let env = [
                ENV_HOME.as_ptr(),
                ENV_RUSTUP_HOME.as_ptr(),
                ENV_CARGO_HOME.as_ptr(),
                ENV_TOOLCHAIN.as_ptr(),
                ENV_PATH.as_ptr(),
                core::ptr::null(),
            ];
            let _ = execve(RUSTC, &args, &env);
            exit(122);
        }
        assert!(pid > 0, "fork worker {} returned {}", index, pid);
        *slot = pid;
    }

    let mut failures = 0usize;
    for pid in pids {
        let mut status = -1i32;
        if waitpid(pid, &mut status) != pid || status != 0 {
            failures += 1;
        }
    }
    failures
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    println!(
        "EXEC_FILE_PAGE_CACHE_PERF_BEGIN workers={} warmup=1 rounds={}",
        WORKERS, MEASURED_ROUNDS
    );

    let warmup_failures = run_batch();
    if warmup_failures != 0 {
        println!(
            "EXEC_FILE_PAGE_CACHE_PERF_FAIL phase=warmup failures={}",
            warmup_failures
        );
        return 1;
    }

    let mut elapsed_us = [0u64; MEASURED_ROUNDS];
    let mut failures = 0usize;
    for (round, sample) in elapsed_us.iter_mut().enumerate() {
        let start = monotonic_ns();
        let round_failures = run_batch();
        *sample = monotonic_ns().saturating_sub(start) / 1_000;
        failures += round_failures;
        println!(
            "EXEC_FILE_PAGE_CACHE_PERF_ROUND round={} elapsed_us={} failures={}",
            round + 1,
            *sample,
            round_failures
        );
    }

    let mut sorted = elapsed_us;
    sorted.sort_unstable();
    let median_us = sorted[MEASURED_ROUNDS / 2];
    println!(
        "EXEC_FILE_PAGE_CACHE_PERF_RESULT workers={} rounds={} median_us={} failures={}",
        WORKERS, MEASURED_ROUNDS, median_us, failures
    );
    if failures == 0 { 0 } else { 1 }
}
