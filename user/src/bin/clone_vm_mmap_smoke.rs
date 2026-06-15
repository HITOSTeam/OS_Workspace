#![no_std]
#![no_main]

#[macro_use]
extern crate user;

use core::arch::asm;
use user::syscall::{exit, syscall, waitpid};

const PAGE_SIZE: usize = 4096;
const TEST_ADDR: usize = 0x32_0000_0000;
const CHILD_STACK_ADDR: usize = 0x33_0000_0000;
const CHILD_STACK_SIZE: usize = PAGE_SIZE * 2;
const TEST_BYTE: u8 = 0x5a;

const SYSCALL_CLONE: usize = 220;
const SYSCALL_MMAP: usize = 222;
const SYSCALL_MUNMAP: usize = 215;

const SIGCHLD: usize = 17;
const CLONE_VM: usize = 0x0000_0100;
const CLONE_VFORK: usize = 0x0000_4000;

const PROT_READ: usize = 1;
const PROT_WRITE: usize = 2;
const MAP_PRIVATE: usize = 0x02;
const MAP_FIXED: usize = 0x10;
const MAP_ANONYMOUS: usize = 0x20;

static mut CHILD_STEP: usize = 0;
static mut CHILD_MMAP_RET: isize = 0;

#[cfg(target_arch = "riscv64")]
#[inline(never)]
fn clone_vm_vfork_with_stack(child_stack: usize) -> isize {
    let flags = CLONE_VM | CLONE_VFORK | SIGCHLD;
    let ret: isize;
    unsafe {
        asm!(
            "ecall",
            "bnez a0, 2f",
            "call {child_entry}",
            "2:",
            inlateout("a0") flags => ret,
            in("a1") child_stack,
            in("a2") 0usize,
            in("a3") 0usize,
            in("a4") 0usize,
            in("a5") 0usize,
            in("a7") SYSCALL_CLONE,
            child_entry = sym clone_vm_child_entry,
            clobber_abi("C"),
        );
    }
    ret
}

#[cfg(target_arch = "loongarch64")]
#[inline(never)]
fn clone_vm_vfork_with_stack(_child_stack: usize) -> isize {
    let flags = CLONE_VM | CLONE_VFORK | SIGCHLD;
    let ret: isize;
    unsafe {
        asm!(
            "move $r5, $r3",
            "syscall 0",
            inlateout("$r4") flags => ret,
            lateout("$r5") _,
            in("$r6") 0usize,
            in("$r7") 0usize,
            in("$r8") 0usize,
            in("$r9") 0usize,
            in("$r11") SYSCALL_CLONE,
        );
    }
    ret
}

fn mmap_fixed(addr: usize, len: usize) -> isize {
    syscall(
        SYSCALL_MMAP,
        [
            addr,
            len,
            PROT_READ | PROT_WRITE,
            MAP_PRIVATE | MAP_FIXED | MAP_ANONYMOUS,
            usize::MAX,
            0,
        ],
    )
}

fn munmap_fixed(addr: usize, len: usize) -> isize {
    syscall(SYSCALL_MUNMAP, [addr, len, 0, 0, 0, 0])
}

fn check_isize(label: &str, actual: isize, expected: isize) -> bool {
    if actual != expected {
        println!(
            "{} mismatch: actual={} expected={}",
            label, actual, expected
        );
        return false;
    }
    true
}

fn check_u8(label: &str, actual: u8, expected: u8) -> bool {
    if actual != expected {
        println!(
            "{} mismatch: actual={} expected={}",
            label, actual, expected
        );
        return false;
    }
    true
}

fn clone_vm_child_body() -> ! {
    unsafe {
        core::ptr::write_volatile(&raw mut CHILD_STEP, 1);
    }
    let mmap_ret = mmap_fixed(TEST_ADDR, PAGE_SIZE);
    unsafe {
        core::ptr::write_volatile(&raw mut CHILD_MMAP_RET, mmap_ret);
        core::ptr::write_volatile(&raw mut CHILD_STEP, 2);
    }
    if mmap_ret != TEST_ADDR as isize {
        exit(2);
    }
    unsafe {
        (TEST_ADDR as *mut u8).write_volatile(TEST_BYTE);
        core::ptr::write_volatile(&raw mut CHILD_STEP, 3);
    }
    exit(0);
}

#[cfg(target_arch = "riscv64")]
extern "C" fn clone_vm_child_entry() -> ! {
    clone_vm_child_body()
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    unsafe {
        core::ptr::write_volatile(&raw mut CHILD_STEP, 0);
        core::ptr::write_volatile(&raw mut CHILD_MMAP_RET, 0);
    }
    if !check_isize(
        "mmap child stack",
        mmap_fixed(CHILD_STACK_ADDR, CHILD_STACK_SIZE),
        CHILD_STACK_ADDR as isize,
    ) {
        return 1;
    }

    let child_stack = CHILD_STACK_ADDR + CHILD_STACK_SIZE;
    let pid = clone_vm_vfork_with_stack(child_stack);
    if pid < 0 {
        println!("clone_vm_vfork failed: {}", pid);
        let _ = munmap_fixed(CHILD_STACK_ADDR, CHILD_STACK_SIZE);
        return 1;
    }

    if pid == 0 {
        clone_vm_child_body();
    }

    let mut exit_code = 0i32;
    if !check_isize("waitpid", waitpid(pid, &mut exit_code), pid) {
        let _ = munmap_fixed(CHILD_STACK_ADDR, CHILD_STACK_SIZE);
        return 1;
    }
    if exit_code != 0 {
        let child_step = unsafe { core::ptr::read_volatile(&raw const CHILD_STEP) };
        let child_mmap_ret = unsafe { core::ptr::read_volatile(&raw const CHILD_MMAP_RET) };
        println!(
            "child exit mismatch: actual={} expected=0 step={} mmap_ret={}",
            exit_code, child_step, child_mmap_ret
        );
        let _ = munmap_fixed(CHILD_STACK_ADDR, CHILD_STACK_SIZE);
        return 1;
    }

    let observed = unsafe { (TEST_ADDR as *const u8).read_volatile() };
    if !check_u8("shared byte", observed, TEST_BYTE) {
        let _ = munmap_fixed(TEST_ADDR, PAGE_SIZE);
        let _ = munmap_fixed(CHILD_STACK_ADDR, CHILD_STACK_SIZE);
        return 1;
    }
    if !check_isize("munmap test page", munmap_fixed(TEST_ADDR, PAGE_SIZE), 0) {
        let _ = munmap_fixed(CHILD_STACK_ADDR, CHILD_STACK_SIZE);
        return 1;
    }
    if !check_isize(
        "munmap child stack",
        munmap_fixed(CHILD_STACK_ADDR, CHILD_STACK_SIZE),
        0,
    ) {
        return 1;
    }

    println!("clone_vm_mmap_smoke passed");
    0
}
