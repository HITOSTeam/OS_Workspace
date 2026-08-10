#![no_std]
#![no_main]

#[macro_use]
extern crate user;

use user::syscall::{fork, sleep, syscall, waitpid};

const SYSCALL_MMAP: usize = 222;
const SYSCALL_MUNMAP: usize = 215;
const SYSCALL_MADVISE: usize = 233;
const SYSCALL_MPROTECT: usize = 226;

const PROT_READ: usize = 1;
const PROT_WRITE: usize = 2;
const MAP_PRIVATE: usize = 0x02;
const MAP_ANONYMOUS: usize = 0x20;
const MADV_DONTNEED: usize = 4;
const PAGE: usize = 4096;

fn mmap_anon(len: usize) -> usize {
    let ret = syscall(SYSCALL_MMAP, [
        0,
        len,
        PROT_READ | PROT_WRITE,
        MAP_PRIVATE | MAP_ANONYMOUS,
        usize::MAX,
        0,
    ]);
    assert!(ret > 0, "mmap failed ret={}", ret);
    ret as usize
}

fn fill(addr: usize, len: usize, seed: usize) {
    let p = addr as *mut u8;
    for i in 0..len {
        unsafe {
            p.add(i)
                .write((seed as u8).wrapping_mul(31).wrapping_add((i & 0xff) as u8))
        };
    }
}

fn verify(addr: usize, len: usize, seed: usize, tag: &str) -> bool {
    let p = addr as *const u8;
    for i in 0..len {
        let expect = (seed as u8).wrapping_mul(31).wrapping_add((i & 0xff) as u8);
        if unsafe { p.add(i).read() } != expect {
            println!(
                "[asid-stress] FAIL {}: byte {} of {} (addr={:#x} seed={})",
                tag, i, len, addr, seed
            );
            return false;
        }
    }
    true
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    println!("[asid-stress] start pid={}", user::syscall::getpid());

    // 阶段 1：页级定向失效。写 -> 改权限 -> 读 -> 再写，反复验证 flush_user_page。
    for round in 0..64usize {
        let addr = mmap_anon(PAGE);
        fill(addr, PAGE, round + 1);
        // 降级为只读，强制旧可写 PTE 作废。
        let ret = syscall(SYSCALL_MPROTECT, [addr, PAGE, PROT_READ, 0, 0, 0]);
        assert!(ret == 0, "mprotect r failed ret={}", ret);
        if !verify(addr, PAGE, round + 1, "phase1-read") {
            return 1;
        }
        let ret = syscall(SYSCALL_MPROTECT, [
            addr,
            PAGE,
            PROT_READ | PROT_WRITE,
            0,
            0,
            0,
        ]);
        assert!(ret == 0, "mprotect rw failed ret={}", ret);
        fill(addr, PAGE, round + 2);
        if !verify(addr, PAGE, round + 2, "phase1-rw") {
            return 2;
        }
        let ret = syscall(SYSCALL_MUNMAP, [addr, PAGE, 0, 0, 0, 0]);
        assert!(ret == 0, "munmap failed ret={}", ret);
    }
    println!("[asid-stress] phase1 ok");

    // 阶段 2：madvise DONTNEED 拆页 + 重写，验证 ASID 定向失效与旧帧释放。
    let big = mmap_anon(256 * PAGE);
    for round in 0..32usize {
        fill(big, 256 * PAGE, 0x40 + round);
        let ret = syscall(SYSCALL_MADVISE, [big, 256 * PAGE, MADV_DONTNEED, 0, 0, 0]);
        assert!(ret == 0, "madvise failed ret={}", ret);
        fill(big, 256 * PAGE, 0x80 + round);
        if !verify(big, 256 * PAGE, 0x80 + round, "phase2") {
            return 3;
        }
    }
    let ret = syscall(SYSCALL_MUNMAP, [big, 256 * PAGE, 0, 0, 0, 0]);
    assert!(ret == 0, "munmap big failed ret={}", ret);
    println!("[asid-stress] phase2 ok");

    // 阶段 3：fork 批量 COW + 地址空间销毁，驱动 ASID 分配/回绕/懒失效。
    for round in 0..24usize {
        let shared = mmap_anon(8 * PAGE);
        fill(shared, 8 * PAGE, 0x100 + round);
        let pid = fork();
        assert!(pid >= 0, "fork failed pid={}", pid);
        if pid == 0 {
            // 子进程：整页改写（触发 COW），立即退出销毁地址空间。
            fill(shared, 8 * PAGE, 0x200 + round);
            if !verify(shared, 8 * PAGE, 0x200 + round, "phase3-child") {
                user::syscall::exit(-10);
            }
            user::syscall::exit(0);
        } else {
            // 父进程：睡眠让子进程在别的 hart 上跑完后校验自己的旧内容。
            sleep(10);
            if !verify(shared, 8 * PAGE, 0x100 + round, "phase3-parent") {
                return 4;
            }
            let mut code = 0;
            waitpid(pid, &mut code);
            let ret = syscall(SYSCALL_MUNMAP, [shared, 8 * PAGE, 0, 0, 0, 0]);
            assert!(ret == 0, "munmap phase3 failed ret={}", ret);
        }
    }
    println!("[asid-stress] phase3 ok");

    // 阶段 4：多进程并发各自 mmap/madvise 同一批地址，挤压远端 shootdown 路径。
    let base = mmap_anon(64 * PAGE);
    fill(base, 64 * PAGE, 0x300);
    let n = 8;
    let mut children = [0isize; 8];
    for i in 0..n {
        let pid = fork();
        assert!(pid >= 0, "fork4 failed pid={}", pid);
        if pid == 0 {
            for round in 0..40usize {
                let off = ((round * 7) % 64) * PAGE;
                let addr = base + off;
                let seed = 0x400 + i + round;
                fill(addr, PAGE, seed);
                let ret = syscall(SYSCALL_MADVISE, [addr, PAGE, MADV_DONTNEED, 0, 0, 0]);
                assert!(ret == 0, "madvise4 failed ret={}", ret);
                fill(addr, PAGE, seed + 0x40);
                if !verify(addr, PAGE, seed + 0x40, "phase4-child") {
                    user::syscall::exit(-20);
                }
            }
            user::syscall::exit(0);
        } else {
            children[i as usize] = pid;
        }
    }
    for i in 0..n {
        let mut code = 0;
        waitpid(children[i as usize], &mut code);
        if code != 0 {
            println!("[asid-stress] FAIL phase4 child {} code={}", i, code);
            return 5;
        }
    }
    if !verify(base, 64 * PAGE, 0x300, "phase4-parent") {
        return 6;
    }
    let ret = syscall(SYSCALL_MUNMAP, [base, 64 * PAGE, 0, 0, 0, 0]);
    assert!(ret == 0, "munmap4 failed ret={}", ret);
    println!("[asid-stress] phase4 ok");

    println!("[asid-stress] ALL OK");
    0
}
