#![no_std]
#![no_main]

#[macro_use]
extern crate user;

use core::arch::asm;
use user::syscall::{exit, syscall, waitpid};

const PAGE_SIZE: usize = 4096;
const CHILD_STACK_ADDR: usize = 0x36_0000_0000;
const CHILD_STACK_SIZE: usize = PAGE_SIZE * 2;
const MREMAP_TARGET: usize = 0x37_0000_0000;

const SYSCALL_CLONE: usize = 220;
const SYSCALL_MUNMAP: usize = 215;
const SYSCALL_MREMAP: usize = 216;
const SYSCALL_MMAP: usize = 222;
const SYSCALL_SHMGET: usize = 194;
const SYSCALL_SHMCTL: usize = 195;
const SYSCALL_SHMAT: usize = 196;
const SYSCALL_SHMDT: usize = 197;

const SIGCHLD: usize = 17;
const CLONE_VM: usize = 0x0000_0100;
const CLONE_VFORK: usize = 0x0000_4000;

const IPC_PRIVATE: usize = 0;
const IPC_CREAT: usize = 0x200;
const IPC_RMID: usize = 0;

const PROT_READ: usize = 1;
const PROT_WRITE: usize = 2;
const MAP_PRIVATE: usize = 0x02;
const MAP_FIXED: usize = 0x10;
const MAP_FIXED_NOREPLACE: usize = 0x100000;
const MAP_ANONYMOUS: usize = 0x20;
const MREMAP_MAYMOVE: usize = 0x01;
const MREMAP_FIXED: usize = 0x02;

const EINVAL: isize = -22;

const CHILD_CASE_SHMAT: usize = 1;
const CHILD_CASE_SHMDT: usize = 2;
const CHILD_CASE_MREMAP_FIXED: usize = 3;

static mut CHILD_CASE: usize = 0;
static mut CHILD_SHMID: usize = 0;
static mut CHILD_ADDR: usize = 0;
static mut CHILD_RESULT: isize = 0;

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

fn mmap_fixed(addr: usize, len: usize, flags: usize) -> isize {
    syscall(
        SYSCALL_MMAP,
        [
            addr,
            len,
            PROT_READ | PROT_WRITE,
            MAP_PRIVATE | MAP_ANONYMOUS | flags,
            usize::MAX,
            0,
        ],
    )
}

fn munmap(addr: usize, len: usize) -> isize {
    syscall(SYSCALL_MUNMAP, [addr, len, 0, 0, 0, 0])
}

fn mremap_fixed(old_addr: usize, len: usize, new_addr: usize) -> isize {
    syscall(
        SYSCALL_MREMAP,
        [
            old_addr,
            len,
            len,
            MREMAP_MAYMOVE | MREMAP_FIXED,
            new_addr,
            0,
        ],
    )
}

fn shmget(size: usize) -> isize {
    syscall(
        SYSCALL_SHMGET,
        [IPC_PRIVATE, size, IPC_CREAT | 0o600, 0, 0, 0],
    )
}

fn shmctl_rmid(shmid: usize) -> isize {
    syscall(SYSCALL_SHMCTL, [shmid, IPC_RMID, 0, 0, 0, 0])
}

fn shmat(shmid: usize) -> isize {
    syscall(SYSCALL_SHMAT, [shmid, 0, 0, 0, 0, 0])
}

fn shmdt(addr: usize) -> isize {
    syscall(SYSCALL_SHMDT, [addr, 0, 0, 0, 0, 0])
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

fn check_positive(label: &str, actual: isize) -> bool {
    if actual <= 0 {
        println!("{} failed: {}", label, actual);
        return false;
    }
    true
}

fn run_clone_case(case_id: usize, shmid: usize, addr: usize) -> bool {
    unsafe {
        core::ptr::write_volatile(&raw mut CHILD_CASE, case_id);
        core::ptr::write_volatile(&raw mut CHILD_SHMID, shmid);
        core::ptr::write_volatile(&raw mut CHILD_ADDR, addr);
        core::ptr::write_volatile(&raw mut CHILD_RESULT, 0);
    }

    let pid = clone_vm_vfork_with_stack(CHILD_STACK_ADDR + CHILD_STACK_SIZE);
    if pid < 0 {
        println!("clone_vm_vfork failed: {}", pid);
        return false;
    }
    if pid == 0 {
        clone_vm_child_body();
    }

    let mut status = 0i32;
    if !check_isize("waitpid", waitpid(pid, &mut status), pid) {
        return false;
    }
    if status != 0 {
        let child_result = unsafe { core::ptr::read_volatile(&raw const CHILD_RESULT) };
        println!(
            "child exit mismatch: actual={} expected=0 result={}",
            status, child_result
        );
        return false;
    }
    true
}

fn clone_vm_child_body() -> ! {
    let case_id = unsafe { core::ptr::read_volatile(&raw const CHILD_CASE) };
    let shmid = unsafe { core::ptr::read_volatile(&raw const CHILD_SHMID) };
    let addr = unsafe { core::ptr::read_volatile(&raw const CHILD_ADDR) };
    let ret = match case_id {
        CHILD_CASE_SHMAT => {
            let mapped = shmat(shmid);
            if mapped > 0 {
                unsafe {
                    (mapped as *mut u8).write_volatile(0x61);
                    core::ptr::write_volatile(&raw mut CHILD_ADDR, mapped as usize);
                }
                mapped
            } else {
                mapped
            }
        }
        CHILD_CASE_SHMDT => shmdt(addr),
        CHILD_CASE_MREMAP_FIXED => mremap_fixed(addr, PAGE_SIZE, MREMAP_TARGET),
        _ => EINVAL,
    };
    unsafe {
        core::ptr::write_volatile(&raw mut CHILD_RESULT, ret);
    }
    if ret < 0 {
        exit(2);
    }
    exit(0);
}

#[cfg(target_arch = "riscv64")]
extern "C" fn clone_vm_child_entry() -> ! {
    clone_vm_child_body()
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    if !check_isize(
        "mmap child stack",
        mmap_fixed(CHILD_STACK_ADDR, CHILD_STACK_SIZE, MAP_FIXED),
        CHILD_STACK_ADDR as isize,
    ) {
        return 1;
    }

    let child_shmat_shmid = shmget(PAGE_SIZE);
    if !check_positive("shmget child shmat", child_shmat_shmid) {
        let _ = munmap(CHILD_STACK_ADDR, CHILD_STACK_SIZE);
        return 1;
    }
    let child_shmat_shmid = child_shmat_shmid as usize;
    if !run_clone_case(CHILD_CASE_SHMAT, child_shmat_shmid, 0) {
        let _ = shmctl_rmid(child_shmat_shmid);
        let _ = munmap(CHILD_STACK_ADDR, CHILD_STACK_SIZE);
        return 1;
    }
    let child_addr = unsafe { core::ptr::read_volatile(&raw const CHILD_ADDR) };
    let _ = shmctl_rmid(child_shmat_shmid);
    if !check_isize("parent shmdt child shmat", shmdt(child_addr), 0) {
        let _ = munmap(CHILD_STACK_ADDR, CHILD_STACK_SIZE);
        return 1;
    }

    let child_shmdt_shmid = shmget(PAGE_SIZE);
    if !check_positive("shmget child shmdt", child_shmdt_shmid) {
        let _ = munmap(CHILD_STACK_ADDR, CHILD_STACK_SIZE);
        return 1;
    }
    let child_shmdt_shmid = child_shmdt_shmid as usize;
    let parent_addr = shmat(child_shmdt_shmid);
    if !check_positive("parent shmat for child shmdt", parent_addr) {
        let _ = shmctl_rmid(child_shmdt_shmid);
        let _ = munmap(CHILD_STACK_ADDR, CHILD_STACK_SIZE);
        return 1;
    }
    let parent_addr = parent_addr as usize;
    let _ = shmctl_rmid(child_shmdt_shmid);
    if !run_clone_case(CHILD_CASE_SHMDT, child_shmdt_shmid, parent_addr) {
        let _ = munmap(CHILD_STACK_ADDR, CHILD_STACK_SIZE);
        return 1;
    }
    if !check_isize(
        "parent stale shmdt after child shmdt",
        shmdt(parent_addr),
        EINVAL,
    ) {
        let _ = munmap(CHILD_STACK_ADDR, CHILD_STACK_SIZE);
        return 1;
    }
    if !check_isize(
        "parent old addr reusable after child shmdt",
        mmap_fixed(parent_addr, PAGE_SIZE, MAP_FIXED_NOREPLACE),
        parent_addr as isize,
    ) {
        let _ = munmap(CHILD_STACK_ADDR, CHILD_STACK_SIZE);
        return 1;
    }
    let _ = munmap(parent_addr, PAGE_SIZE);

    let child_mremap_shmid = shmget(PAGE_SIZE);
    if !check_positive("shmget child mremap", child_mremap_shmid) {
        let _ = munmap(CHILD_STACK_ADDR, CHILD_STACK_SIZE);
        return 1;
    }
    let child_mremap_shmid = child_mremap_shmid as usize;
    let move_src = shmat(child_mremap_shmid);
    if !check_positive("parent shmat for child mremap", move_src) {
        let _ = shmctl_rmid(child_mremap_shmid);
        let _ = munmap(CHILD_STACK_ADDR, CHILD_STACK_SIZE);
        return 1;
    }
    let move_src = move_src as usize;
    let _ = shmctl_rmid(child_mremap_shmid);
    if !run_clone_case(CHILD_CASE_MREMAP_FIXED, child_mremap_shmid, move_src) {
        let _ = munmap(CHILD_STACK_ADDR, CHILD_STACK_SIZE);
        return 1;
    }
    if !check_isize(
        "parent stale shmdt after child mremap",
        shmdt(move_src),
        EINVAL,
    ) {
        let _ = munmap(CHILD_STACK_ADDR, CHILD_STACK_SIZE);
        return 1;
    }
    if !check_isize("parent shmdt child mremap target", shmdt(MREMAP_TARGET), 0) {
        let _ = munmap(CHILD_STACK_ADDR, CHILD_STACK_SIZE);
        return 1;
    }
    if !check_isize(
        "parent target reusable after shmdt",
        mmap_fixed(MREMAP_TARGET, PAGE_SIZE, MAP_FIXED_NOREPLACE),
        MREMAP_TARGET as isize,
    ) {
        let _ = munmap(CHILD_STACK_ADDR, CHILD_STACK_SIZE);
        return 1;
    }
    let _ = munmap(MREMAP_TARGET, PAGE_SIZE);

    if !check_isize(
        "munmap child stack",
        munmap(CHILD_STACK_ADDR, CHILD_STACK_SIZE),
        0,
    ) {
        return 1;
    }

    println!("clone_vm_sysv_shm_smoke passed");
    0
}
