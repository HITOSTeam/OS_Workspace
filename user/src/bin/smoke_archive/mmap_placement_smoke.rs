#![no_std]
#![no_main]

#[macro_use]
extern crate user;

use user::syscall::syscall;

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
const EINVAL: isize = -22;

fn mmap_anon_flags(addr: usize, len: usize, flags: usize) -> isize {
    syscall(
        SYSCALL_MMAP,
        [addr, len, PROT_READ | PROT_WRITE, flags, usize::MAX, 0],
    )
}

fn mmap_anon(addr: usize, len: usize, fixed_noreplace: bool) -> isize {
    let mut flags = MAP_PRIVATE | MAP_ANONYMOUS;
    if fixed_noreplace {
        flags |= MAP_FIXED_NOREPLACE;
    }
    mmap_anon_flags(addr, len, flags)
}

fn munmap(addr: usize, len: usize) -> isize {
    syscall(SYSCALL_MUNMAP, [addr, len, 0, 0, 0, 0])
}

fn mremap_grow(addr: usize, old_len: usize, new_len: usize) -> isize {
    syscall(
        SYSCALL_MREMAP,
        [addr, old_len, new_len, MREMAP_MAYMOVE, 0, 0],
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

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let first = mmap_anon(0, PAGE_SIZE, false);
    assert!(first > 0);
    let first = first as usize;

    let second = mmap_anon(0, PAGE_SIZE, false);
    assert!(second > 0);
    let second = second as usize;
    assert!(second < first);
    assert_eq!(munmap(second, PAGE_SIZE), 0);

    let hinted = mmap_anon(first, PAGE_SIZE, false);
    assert!(hinted > 0);
    assert_ne!(hinted as usize, first);
    assert!((hinted as usize) < first);
    assert_eq!(munmap(hinted as usize, PAGE_SIZE), 0);

    assert_eq!(
        mmap_anon_flags(
            first + 1,
            PAGE_SIZE,
            MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED
        ),
        EINVAL
    );
    assert_eq!(
        mmap_anon_flags(
            first + 1,
            PAGE_SIZE,
            MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED_NOREPLACE,
        ),
        EINVAL
    );

    let shmid = shmget(PAGE_SIZE);
    assert!(shmid > 0);
    let shmid = shmid as usize;
    let attached = shmat(shmid);
    assert!(attached > 0);
    assert_ne!(attached as usize, first);
    assert!((attached as usize) < first);
    assert_eq!(shmctl_rmid(shmid), 0);
    assert_eq!(shmdt(attached as usize), 0);

    let src = mmap_anon(0, PAGE_SIZE, false);
    assert!(src > 0);
    let src = src as usize;

    let grown = mremap_grow(src, PAGE_SIZE, PAGE_SIZE * 2);
    assert!(grown > 0);
    assert_ne!(grown as usize, src);
    assert_eq!(munmap(grown as usize, PAGE_SIZE * 2), 0);

    assert_eq!(munmap(first, PAGE_SIZE), 0);

    println!("mmap_placement_smoke passed");
    0
}
