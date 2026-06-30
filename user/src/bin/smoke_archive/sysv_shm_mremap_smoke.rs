#![no_std]
#![no_main]

#[macro_use]
extern crate user;

use user::syscall::{SIGBUS, SIGSEGV, exit, fork, syscall, waitpid};

const PAGE_SIZE: usize = 4096;

const SYSCALL_SHMGET: usize = 194;
const SYSCALL_SHMCTL: usize = 195;
const SYSCALL_SHMAT: usize = 196;
const SYSCALL_SHMDT: usize = 197;
const SYSCALL_MUNMAP: usize = 215;
const SYSCALL_MREMAP: usize = 216;
const SYSCALL_MMAP: usize = 222;

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
const SHM_REMAP: usize = 0x4000;

const EINVAL: isize = -22;

fn shmget(size: usize, flags: usize) -> isize {
    syscall(SYSCALL_SHMGET, [IPC_PRIVATE, size, flags, 0, 0, 0])
}

fn shmctl(shmid: usize, cmd: usize) -> isize {
    syscall(SYSCALL_SHMCTL, [shmid, cmd, 0, 0, 0, 0])
}

fn shmat(shmid: usize) -> isize {
    syscall(SYSCALL_SHMAT, [shmid, 0, 0, 0, 0, 0])
}

fn shmat_remap(shmid: usize, addr: usize) -> isize {
    syscall(SYSCALL_SHMAT, [shmid, addr, SHM_REMAP, 0, 0, 0])
}

fn shmdt(addr: usize) -> isize {
    syscall(SYSCALL_SHMDT, [addr, 0, 0, 0, 0, 0])
}

fn mremap_grow(addr: usize, old_len: usize, new_len: usize) -> isize {
    syscall(
        SYSCALL_MREMAP,
        [addr, old_len, new_len, MREMAP_MAYMOVE, 0, 0],
    )
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

fn mmap_fixed_replace(addr: usize, len: usize) -> isize {
    syscall(
        SYSCALL_MMAP,
        [
            addr,
            len,
            PROT_READ | PROT_WRITE,
            MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED,
            usize::MAX,
            0,
        ],
    )
}

fn mmap_fixed_noreplace(addr: usize, len: usize) -> isize {
    syscall(
        SYSCALL_MMAP,
        [
            addr,
            len,
            PROT_READ | PROT_WRITE,
            MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED_NOREPLACE,
            usize::MAX,
            0,
        ],
    )
}

fn munmap(addr: usize, len: usize) -> isize {
    syscall(SYSCALL_MUNMAP, [addr, len, 0, 0, 0, 0])
}

fn wait_termsig(status: i32) -> i32 {
    status & 0x7f
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let shmid = shmget(PAGE_SIZE, IPC_CREAT | 0o600);
    assert!(shmid > 0);
    let shmid = shmid as usize;

    let mapped = shmat(shmid);
    assert!(mapped > 0);
    assert_eq!(shmctl(shmid, IPC_RMID), 0);

    let grown = mremap_grow(mapped as usize, PAGE_SIZE, PAGE_SIZE * 2);
    assert!(grown > 0);
    let grown = grown as usize;

    // SAFETY: the first page is the original attached SysV shm segment.
    unsafe {
        (grown as *mut u8).write_volatile(0x41);
        assert_eq!((grown as *const u8).read_volatile(), 0x41);
    }

    let child = fork();
    assert!(child >= 0);
    if child == 0 {
        // SAFETY: if mremap incorrectly created an anonymous tail, this write
        // succeeds and the child exits normally. The correct behavior is a
        // SIGBUS/SIGSEGV-style signal termination from the fault path.
        unsafe {
            (grown as *mut u8).add(PAGE_SIZE).write_volatile(0x55);
        }
        exit(7);
    }

    let mut status = 0i32;
    assert_eq!(waitpid(child, &mut status), child);
    let sig = wait_termsig(status);
    assert!(sig == SIGBUS || sig == SIGSEGV);

    assert_eq!(shmdt(grown), 0);

    let tail = grown + PAGE_SIZE;
    let remap = mmap_fixed_noreplace(tail, PAGE_SIZE);
    assert_eq!(remap, tail as isize);
    assert_eq!(munmap(tail, PAGE_SIZE), 0);

    let partial_shmid = shmget(PAGE_SIZE * 3, IPC_CREAT | 0o600);
    assert!(partial_shmid > 0);
    let partial_shmid = partial_shmid as usize;

    let base = shmat(partial_shmid);
    assert!(base > 0);
    assert_eq!(shmctl(partial_shmid, IPC_RMID), 0);
    let base = base as usize;

    // SAFETY: the SysV shm attachment covers three writable pages.
    unsafe {
        (base as *mut u8).write_volatile(0x11);
        (base as *mut u8).add(PAGE_SIZE).write_volatile(0x22);
        (base as *mut u8).add(PAGE_SIZE * 2).write_volatile(0x33);
    }

    let moved = mremap_grow(base + PAGE_SIZE, PAGE_SIZE, PAGE_SIZE * 2);
    assert!(moved > 0);
    let moved = moved as usize;

    // SAFETY: partial mremap should move the middle page and grow through the
    // same SysV shm backing. The grown second page aliases the original third
    // page rather than becoming anonymous memory.
    unsafe {
        assert_eq!((base as *const u8).read_volatile(), 0x11);
        assert_eq!((moved as *const u8).read_volatile(), 0x22);
        assert_eq!((moved as *const u8).add(PAGE_SIZE).read_volatile(), 0x33);
        (moved as *mut u8).add(PAGE_SIZE).write_volatile(0x44);
        assert_eq!((base as *const u8).add(PAGE_SIZE * 2).read_volatile(), 0x44);
    }

    assert_eq!(shmdt(moved), 0);
    let remap = mmap_fixed_noreplace(moved, PAGE_SIZE * 2);
    assert_eq!(remap, moved as isize);
    assert_eq!(munmap(moved, PAGE_SIZE * 2), 0);

    assert_eq!(shmdt(base), 0);
    let remap = mmap_fixed_noreplace(base, PAGE_SIZE);
    assert_eq!(remap, base as isize);
    assert_eq!(munmap(base, PAGE_SIZE), 0);

    let right = base + PAGE_SIZE * 2;
    assert_eq!(shmdt(right), 0);
    let remap = mmap_fixed_noreplace(right, PAGE_SIZE);
    assert_eq!(remap, right as isize);
    assert_eq!(munmap(right, PAGE_SIZE), 0);

    let fixed_shmid = shmget(PAGE_SIZE, IPC_CREAT | 0o600);
    assert!(fixed_shmid > 0);
    let fixed_shmid = fixed_shmid as usize;
    let fixed_addr = shmat(fixed_shmid);
    assert!(fixed_addr > 0);
    assert_eq!(shmctl(fixed_shmid, IPC_RMID), 0);
    let fixed_addr = fixed_addr as usize;

    let replaced = mmap_fixed_replace(fixed_addr, PAGE_SIZE);
    assert_eq!(replaced, fixed_addr as isize);
    assert_eq!(shmdt(fixed_addr), EINVAL);
    assert_eq!(munmap(fixed_addr, PAGE_SIZE), 0);
    let remap = mmap_fixed_noreplace(fixed_addr, PAGE_SIZE);
    assert_eq!(remap, fixed_addr as isize);
    assert_eq!(munmap(fixed_addr, PAGE_SIZE), 0);

    let src_shmid = shmget(PAGE_SIZE, IPC_CREAT | 0o600);
    let dst_shmid = shmget(PAGE_SIZE, IPC_CREAT | 0o600);
    assert!(src_shmid > 0 && dst_shmid > 0);
    let src_shmid = src_shmid as usize;
    let dst_shmid = dst_shmid as usize;
    let src = shmat(src_shmid);
    let dst = shmat(dst_shmid);
    assert!(src > 0 && dst > 0);
    assert_eq!(shmctl(src_shmid, IPC_RMID), 0);
    assert_eq!(shmctl(dst_shmid, IPC_RMID), 0);
    let src = src as usize;
    let dst = dst as usize;
    unsafe {
        (src as *mut u8).write_volatile(0x5a);
        (dst as *mut u8).write_volatile(0xa5);
    }

    let moved = mremap_fixed(src, PAGE_SIZE, dst);
    assert_eq!(moved, dst as isize);
    unsafe {
        assert_eq!((dst as *const u8).read_volatile(), 0x5a);
    }
    assert_eq!(shmdt(src), EINVAL);
    assert_eq!(shmdt(dst), 0);
    assert_eq!(shmdt(dst), EINVAL);
    let remap = mmap_fixed_noreplace(src, PAGE_SIZE);
    assert_eq!(remap, src as isize);
    assert_eq!(munmap(src, PAGE_SIZE), 0);
    let remap = mmap_fixed_noreplace(dst, PAGE_SIZE);
    assert_eq!(remap, dst as isize);
    assert_eq!(munmap(dst, PAGE_SIZE), 0);

    let old_shmid = shmget(PAGE_SIZE, IPC_CREAT | 0o600);
    let new_shmid = shmget(PAGE_SIZE, IPC_CREAT | 0o600);
    assert!(old_shmid > 0 && new_shmid > 0);
    let old_shmid = old_shmid as usize;
    let new_shmid = new_shmid as usize;
    let replace_addr = shmat(old_shmid);
    assert!(replace_addr > 0);
    assert_eq!(shmctl(old_shmid, IPC_RMID), 0);
    let replace_addr = replace_addr as usize;
    let remapped = shmat_remap(new_shmid, replace_addr);
    assert_eq!(remapped, replace_addr as isize);
    assert_eq!(shmctl(new_shmid, IPC_RMID), 0);
    assert_eq!(shmdt(replace_addr), 0);
    assert_eq!(shmdt(replace_addr), EINVAL);
    let remap = mmap_fixed_noreplace(replace_addr, PAGE_SIZE);
    assert_eq!(remap, replace_addr as isize);
    assert_eq!(munmap(replace_addr, PAGE_SIZE), 0);

    println!("sysv_shm_mremap_smoke passed");
    0
}
