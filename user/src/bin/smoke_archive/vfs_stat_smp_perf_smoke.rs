#![no_std]
#![no_main]

extern crate user;

use core::arch::asm;
use core::sync::atomic::{AtomicUsize, Ordering};
use user::syscall::{_yield, exit, syscall};

const PAGE_SIZE: usize = 4096;
const WORKERS: usize = 12;
const STATS_PER_WORKER: usize = 1024;
const STACK_SIZE: usize = PAGE_SIZE * 16;
const STACKS_ADDR: usize = 0x31_a000_0000;

const SYSCALL_CLONE: usize = 220;
const SYSCALL_CLOCK_GETTIME: usize = 113;
const SYSCALL_EXIT_GROUP: usize = 94;
const SYSCALL_MMAP: usize = 222;
const SYSCALL_NEWFSTATAT: usize = 79;

const CLONE_VM: usize = 0x0000_0100;
const CLONE_FS: usize = 0x0000_0200;
const CLONE_FILES: usize = 0x0000_0400;
const CLONE_SIGHAND: usize = 0x0000_0800;
const CLONE_THREAD: usize = 0x0001_0000;

const PROT_READ: usize = 1;
const PROT_WRITE: usize = 2;
const MAP_PRIVATE: usize = 0x02;
const MAP_FIXED: usize = 0x10;
const MAP_ANONYMOUS: usize = 0x20;
const AT_FDCWD: isize = -100;

static PATH: &[u8] = b"/glibc/buildstorm_testcode.sh\0";
static READY: AtomicUsize = AtomicUsize::new(0);
static START: AtomicUsize = AtomicUsize::new(0);
static DONE: AtomicUsize = AtomicUsize::new(0);
static ERRORS: AtomicUsize = AtomicUsize::new(0);

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct KStat {
    st_dev: u64,
    st_ino: u64,
    st_mode: u32,
    st_nlink: u32,
    st_uid: u32,
    st_gid: u32,
    st_rdev: u64,
    __pad: u64,
    st_size: i64,
    st_blksize: u32,
    __pad2: i32,
    st_blocks: u64,
    st_atime_sec: i64,
    st_atime_nsec: i64,
    st_mtime_sec: i64,
    st_mtime_nsec: i64,
    st_ctime_sec: i64,
    st_ctime_nsec: i64,
    __unused: [u32; 2],
}

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
            [1, &mut now as *mut Timespec as usize, 0, 0, 0, 0],
        ),
        0
    );
    (now.sec as u64)
        .saturating_mul(1_000_000_000)
        .saturating_add(now.nsec as u64)
}

fn stat_buildstorm_script(stat: &mut KStat) -> isize {
    syscall(
        SYSCALL_NEWFSTATAT,
        [
            AT_FDCWD as usize,
            PATH.as_ptr() as usize,
            stat as *mut KStat as usize,
            0,
            0,
            0,
        ],
    )
}

extern "C" fn worker_entry() -> ! {
    READY.fetch_add(1, Ordering::Release);
    while START.load(Ordering::Acquire) == 0 {
        _yield();
    }

    let mut stat = KStat::default();
    for _ in 0..STATS_PER_WORKER {
        if stat_buildstorm_script(&mut stat) != 0 || stat.st_size <= 0 {
            ERRORS.fetch_add(1, Ordering::Relaxed);
            break;
        }
    }
    DONE.fetch_add(1, Ordering::Release);
    exit(0);
}

#[cfg(target_arch = "loongarch64")]
#[inline(never)]
fn clone_worker(child_stack: usize) -> isize {
    let flags = CLONE_VM | CLONE_FS | CLONE_FILES | CLONE_SIGHAND | CLONE_THREAD;
    let ret: isize;
    unsafe {
        asm!(
            "syscall 0",
            "bnez $r4, 2f",
            "b {worker_entry}",
            "2:",
            inlateout("$r4") flags => ret,
            in("$r5") child_stack,
            in("$r6") 0usize,
            in("$r7") 0usize,
            in("$r8") 0usize,
            in("$r9") 0usize,
            in("$r11") SYSCALL_CLONE,
            worker_entry = sym worker_entry,
        );
    }
    ret
}

#[cfg(target_arch = "riscv64")]
#[inline(never)]
fn clone_worker(child_stack: usize) -> isize {
    let flags = CLONE_VM | CLONE_FS | CLONE_FILES | CLONE_SIGHAND | CLONE_THREAD;
    let ret: isize;
    unsafe {
        asm!(
            "ecall",
            "bnez a0, 2f",
            "j {worker_entry}",
            "2:",
            inlateout("a0") flags => ret,
            in("a1") child_stack,
            in("a2") 0usize,
            in("a3") 0usize,
            in("a4") 0usize,
            in("a5") 0usize,
            in("a7") SYSCALL_CLONE,
            worker_entry = sym worker_entry,
        );
    }
    ret
}

fn exit_group(code: usize) -> ! {
    let ret = syscall(SYSCALL_EXIT_GROUP, [code, 0, 0, 0, 0, 0]);
    panic!("exit_group returned {ret}");
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let stacks_len = WORKERS * STACK_SIZE;
    assert_eq!(
        syscall(
            SYSCALL_MMAP,
            [
                STACKS_ADDR,
                stacks_len,
                PROT_READ | PROT_WRITE,
                MAP_PRIVATE | MAP_FIXED | MAP_ANONYMOUS,
                usize::MAX,
                0,
            ],
        ),
        STACKS_ADDR as isize
    );

    for worker in 0..WORKERS {
        let stack_top = STACKS_ADDR + (worker + 1) * STACK_SIZE;
        let tid = clone_worker(stack_top);
        assert!(tid > 0, "clone worker {worker} failed: {tid}");
    }
    while READY.load(Ordering::Acquire) != WORKERS {
        _yield();
    }

    let start_ns = monotonic_ns();
    START.store(1, Ordering::Release);
    while DONE.load(Ordering::Acquire) != WORKERS {
        _yield();
    }
    let elapsed_us = monotonic_ns().saturating_sub(start_ns) / 1_000;
    let errors = ERRORS.load(Ordering::Acquire);
    user::println!(
        "VFS_STAT_SMP_PERF workers={} stats_per_worker={} elapsed_us={} errors={}",
        WORKERS,
        STATS_PER_WORKER,
        elapsed_us,
        errors
    );
    exit_group((errors != 0) as usize);
}
