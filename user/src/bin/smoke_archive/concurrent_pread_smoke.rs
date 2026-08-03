#![no_std]
#![no_main]

#[macro_use]
extern crate user;

use core::arch::asm;
use core::sync::atomic::{AtomicUsize, Ordering};
use user::syscall::{_yield, CREATE, RDONLY, RDWR, TRUNC, close, exit, open, syscall, write};

const PAGE_SIZE: usize = 4096;
const FILE_PAGES: usize = 256;
const WORKERS: usize = 8;
const READS_PER_WORKER: usize = 128;
const STACK_SIZE: usize = PAGE_SIZE * 16;
const STACKS_ADDR: usize = 0x31_5000_0000;
const PATH: &str = "/tmp/concurrent_pread_smoke";

const SYSCALL_CLONE: usize = 220;
const SYSCALL_MMAP: usize = 222;
const SYSCALL_PREAD64: usize = 67;

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

static SHARED_FD: AtomicUsize = AtomicUsize::new(usize::MAX);
static NEXT_WORKER: AtomicUsize = AtomicUsize::new(0);
static READY: AtomicUsize = AtomicUsize::new(0);
static START: AtomicUsize = AtomicUsize::new(0);
static DONE: AtomicUsize = AtomicUsize::new(0);
static FIRST_ERROR: AtomicUsize = AtomicUsize::new(0);

fn expected_byte(page: usize, byte: usize) -> u8 {
    (page as u8).wrapping_mul(17) ^ (byte as u8).wrapping_mul(31)
}

fn pread64(fd: usize, buf: &mut [u8], offset: usize) -> isize {
    syscall(
        SYSCALL_PREAD64,
        [fd, buf.as_mut_ptr() as usize, buf.len(), offset, 0, 0],
    )
}

fn worker_body() -> ! {
    let worker = NEXT_WORKER.fetch_add(1, Ordering::AcqRel);
    READY.fetch_add(1, Ordering::Release);
    while START.load(Ordering::Acquire) == 0 {
        _yield();
    }

    let fd = SHARED_FD.load(Ordering::Acquire);
    let mut page_buf = [0u8; PAGE_SIZE];
    'reads: for iteration in 0..READS_PER_WORKER {
        // A stride coprime to FILE_PAGES makes each worker traverse the whole
        // file while starting in a different readahead window.
        let page = (worker * 29 + iteration * 37) % FILE_PAGES;
        let offset = page * PAGE_SIZE;
        if pread64(fd, &mut page_buf, offset) != PAGE_SIZE as isize {
            FIRST_ERROR
                .compare_exchange(0, offset + 1, Ordering::AcqRel, Ordering::Acquire)
                .ok();
            break;
        }
        for (byte, value) in page_buf.iter().copied().enumerate() {
            if value != expected_byte(page, byte) {
                FIRST_ERROR
                    .compare_exchange(0, offset + byte + 1, Ordering::AcqRel, Ordering::Acquire)
                    .ok();
                break 'reads;
            }
        }
    }

    DONE.fetch_add(1, Ordering::Release);
    exit(0);
}

extern "C" fn worker_entry() -> ! {
    worker_body()
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

fn map_stacks() -> isize {
    syscall(
        SYSCALL_MMAP,
        [
            STACKS_ADDR,
            WORKERS * STACK_SIZE,
            PROT_READ | PROT_WRITE,
            MAP_PRIVATE | MAP_FIXED | MAP_ANONYMOUS,
            usize::MAX,
            0,
        ],
    )
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let write_fd = open(PATH, RDWR | CREATE | TRUNC);
    assert!(write_fd >= 0, "create failed: {write_fd}");
    let write_fd = write_fd as usize;
    let mut page_buf = [0u8; PAGE_SIZE];
    for page in 0..FILE_PAGES {
        for (byte, value) in page_buf.iter_mut().enumerate() {
            *value = expected_byte(page, byte);
        }
        assert_eq!(write(write_fd, &page_buf), PAGE_SIZE as isize);
    }
    close(write_fd);

    let read_fd = open(PATH, RDONLY);
    assert!(read_fd >= 0, "read-only open failed: {read_fd}");
    let read_fd = read_fd as usize;
    SHARED_FD.store(read_fd, Ordering::Release);

    assert_eq!(map_stacks(), STACKS_ADDR as isize);
    for worker in 0..WORKERS {
        let stack_top = STACKS_ADDR + (worker + 1) * STACK_SIZE;
        let tid = clone_worker(stack_top);
        assert!(tid > 0, "clone worker {worker} failed: {tid}");
    }
    while READY.load(Ordering::Acquire) != WORKERS {
        _yield();
    }
    START.store(1, Ordering::Release);
    while DONE.load(Ordering::Acquire) != WORKERS {
        _yield();
    }

    let error = FIRST_ERROR.load(Ordering::Acquire);
    close(read_fd);
    assert_eq!(
        error, 0,
        "concurrent pread mismatch at encoded offset {error}"
    );
    println!(
        "CONCURRENT_PREAD_PASS workers={} reads={} bytes={}",
        WORKERS,
        READS_PER_WORKER,
        WORKERS * READS_PER_WORKER * PAGE_SIZE
    );
    0
}
