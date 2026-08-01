#![no_std]
#![no_main]

#[macro_use]
extern crate user;

#[cfg(target_arch = "riscv64")]
mod riscv_test {
    use core::arch::asm;
    use core::sync::atomic::{AtomicUsize, Ordering};
    use user::syscall::{_yield, exit, syscall};

    const PAGE_SIZE: usize = 4096;
    const CODE_ADDR: usize = 0x32_0000_0000;
    const CHILD_STACK_ADDR: usize = 0x33_0000_0000;
    const CHILD_STACK_SIZE: usize = PAGE_SIZE * 16;
    const MAX_HARTS: usize = 8;
    const MAX_WORKERS: usize = MAX_HARTS - 1;
    const UPDATE_COUNT: usize = 128;
    const FINAL_EPOCH: usize = UPDATE_COUNT + 1;

    const SYSCALL_CLONE: usize = 220;
    const SYSCALL_MMAP: usize = 222;
    const SYSCALL_MPROTECT: usize = 226;
    const SYSCALL_SCHED_SETAFFINITY: usize = 122;
    const SYSCALL_SCHED_GETAFFINITY: usize = 123;

    const CLONE_VM: usize = 0x0000_0100;
    const CLONE_SIGHAND: usize = 0x0000_0800;
    const CLONE_THREAD: usize = 0x0001_0000;

    const PROT_READ: usize = 1;
    const PROT_WRITE: usize = 2;
    const PROT_EXEC: usize = 4;
    const MAP_PRIVATE: usize = 0x02;
    const MAP_FIXED: usize = 0x10;
    const MAP_ANONYMOUS: usize = 0x20;

    static START: AtomicUsize = AtomicUsize::new(0);
    static ABORT: AtomicUsize = AtomicUsize::new(0);
    static EPOCH: AtomicUsize = AtomicUsize::new(0);
    static EXPECTED: AtomicUsize = AtomicUsize::new(0);
    static ACKNOWLEDGED: [AtomicUsize; MAX_WORKERS] = [const { AtomicUsize::new(0) }; MAX_WORKERS];
    static FAILED_WORKER: AtomicUsize = AtomicUsize::new(MAX_WORKERS);
    static FAILED_EPOCH: AtomicUsize = AtomicUsize::new(0);
    static FAILED_EXPECTED: AtomicUsize = AtomicUsize::new(0);
    static FAILED_OBSERVED: AtomicUsize = AtomicUsize::new(0);

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

    fn mprotect(addr: usize, len: usize, prot: usize) -> isize {
        syscall(SYSCALL_MPROTECT, [addr, len, prot, 0, 0, 0])
    }

    fn pin_thread_to_cpu(tid: usize, cpu: usize) -> isize {
        let mask = 1usize << cpu;
        syscall(
            SYSCALL_SCHED_SETAFFINITY,
            [
                tid,
                core::mem::size_of::<usize>(),
                &mask as *const usize as usize,
                0,
                0,
                0,
            ],
        )
    }

    fn online_cpu_mask() -> Option<usize> {
        let mut mask = 0usize;
        let result = syscall(
            SYSCALL_SCHED_GETAFFINITY,
            [
                0,
                core::mem::size_of::<usize>(),
                &mut mask as *mut usize as usize,
                0,
                0,
                0,
            ],
        );
        (result >= 0).then_some(mask)
    }

    fn install_return_value(value: usize) {
        debug_assert!(value < 0x800);
        // addi a0, zero, value; ret. The page is writable whenever this
        // helper is called and mprotect publishes it as executable afterward.
        let addi_a0 = ((value as u32) << 20) | (10 << 7) | 0x13;
        // SAFETY: CODE_ADDR names the private page created by run(), is
        // naturally aligned, and currently has write permission.
        unsafe {
            (CODE_ADDR as *mut u32).write_volatile(addi_a0);
            ((CODE_ADDR + 4) as *mut u32).write_volatile(0x0000_8067);
        }
    }

    fn call_generated_code() -> usize {
        // SAFETY: install_return_value writes a valid two-instruction RISC-V
        // function and the caller switches the page to RX before publishing
        // the corresponding epoch.
        let function: extern "C" fn() -> usize = unsafe { core::mem::transmute(CODE_ADDR) };
        function()
    }

    fn child_body(worker: usize) -> ! {
        while START.load(Ordering::Acquire) == 0 {
            core::hint::spin_loop();
        }

        let mut completed_epoch = 0usize;
        loop {
            if ABORT.load(Ordering::Acquire) != 0 {
                exit(2);
            }
            let epoch = EPOCH.load(Ordering::Acquire);
            if epoch == 0 || epoch == completed_epoch {
                core::hint::spin_loop();
                continue;
            }
            let expected = EXPECTED.load(Ordering::Relaxed);
            let observed = call_generated_code();
            if observed != expected {
                if FAILED_WORKER
                    .compare_exchange(MAX_WORKERS, worker, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    FAILED_EXPECTED.store(expected, Ordering::Relaxed);
                    FAILED_OBSERVED.store(observed, Ordering::Relaxed);
                    FAILED_EPOCH.store(epoch, Ordering::Release);
                }
                ABORT.store(1, Ordering::Release);
                exit(1);
            }
            completed_epoch = epoch;
            ACKNOWLEDGED[worker].store(epoch, Ordering::Release);
            if epoch == FINAL_EPOCH {
                exit(0);
            }
        }
    }

    extern "C" fn child_entry(worker: usize) -> ! {
        child_body(worker)
    }

    #[inline(never)]
    fn clone_same_mm_thread(child_stack: usize, worker: usize) -> isize {
        let flags = CLONE_VM | CLONE_SIGHAND | CLONE_THREAD;
        let ret: isize;
        // SAFETY: clone receives a mapped, aligned child stack. The child
        // branches directly to a diverging Rust entry before using the
        // parent's stack frame.
        unsafe {
            asm!(
                "ecall",
                "bnez a0, 2f",
                "mv a0, a6",
                "j {child_entry}",
                "2:",
                inlateout("a0") flags => ret,
                in("a1") child_stack,
                in("a2") 0usize,
                in("a3") 0usize,
                in("a4") 0usize,
                in("a5") 0usize,
                in("a6") worker,
                in("a7") SYSCALL_CLONE,
                child_entry = sym child_entry,
            );
        }
        ret
    }

    fn wait_for_epoch(epoch: usize, workers: usize) -> bool {
        loop {
            if (0..workers).all(|worker| ACKNOWLEDGED[worker].load(Ordering::Acquire) == epoch) {
                return true;
            }
            if FAILED_EPOCH.load(Ordering::Acquire) != 0 {
                return false;
            }
            _yield();
        }
    }

    pub fn run() -> i32 {
        let Some(online_mask) = online_cpu_mask() else {
            println!("riscv_icache_smp_smoke: getaffinity failed");
            return 1;
        };
        let mut worker_cpus = [0usize; MAX_WORKERS];
        let mut worker_count = 0usize;
        for cpu in 1..MAX_HARTS {
            if online_mask & (1usize << cpu) != 0 {
                worker_cpus[worker_count] = cpu;
                worker_count += 1;
            }
        }
        if worker_count == 0 {
            println!("riscv_icache_smp_smoke skipped: one online hart");
            return 0;
        }
        if pin_thread_to_cpu(0, 0) != 0 {
            println!("riscv_icache_smp_smoke: parent affinity failed");
            return 1;
        }
        if mmap_fixed(CODE_ADDR, PAGE_SIZE) != CODE_ADDR as isize {
            println!("riscv_icache_smp_smoke: code mmap failed");
            return 1;
        }
        install_return_value(7);
        if mprotect(CODE_ADDR, PAGE_SIZE, PROT_READ | PROT_EXEC) != 0 {
            println!("riscv_icache_smp_smoke: initial mprotect failed");
            return 1;
        }
        let stack_mapping_size = CHILD_STACK_SIZE * worker_count;
        if mmap_fixed(CHILD_STACK_ADDR, stack_mapping_size) != CHILD_STACK_ADDR as isize {
            println!("riscv_icache_smp_smoke: stack mmap failed");
            return 1;
        }

        for worker in 0..worker_count {
            let child_stack = CHILD_STACK_ADDR + CHILD_STACK_SIZE * (worker + 1);
            let tid = clone_same_mm_thread(child_stack, worker);
            if tid < 0 {
                println!(
                    "riscv_icache_smp_smoke: clone failed worker={} ret={}",
                    worker, tid
                );
                ABORT.store(1, Ordering::Release);
                START.store(1, Ordering::Release);
                return 1;
            }
            if pin_thread_to_cpu(tid as usize, worker_cpus[worker]) != 0 {
                println!(
                    "riscv_icache_smp_smoke: affinity failed worker={} cpu={}",
                    worker, worker_cpus[worker]
                );
                ABORT.store(1, Ordering::Release);
                START.store(1, Ordering::Release);
                return 1;
            }
        }
        println!(
            "riscv_icache_smp_smoke: workers={} online_mask={:#x}",
            worker_count, online_mask
        );

        START.store(1, Ordering::Release);
        EXPECTED.store(7, Ordering::Relaxed);
        EPOCH.store(1, Ordering::Release);
        let mut ok = wait_for_epoch(1, worker_count);

        for epoch in 2..=FINAL_EPOCH {
            if !ok {
                break;
            }
            let value = epoch + 7;
            if mprotect(CODE_ADDR, PAGE_SIZE, PROT_READ | PROT_WRITE) != 0 {
                println!("riscv_icache_smp_smoke: RW mprotect failed at {}", epoch);
                ABORT.store(1, Ordering::Release);
                ok = false;
                break;
            }
            install_return_value(value);
            if mprotect(CODE_ADDR, PAGE_SIZE, PROT_READ | PROT_EXEC) != 0 {
                println!("riscv_icache_smp_smoke: RX mprotect failed at {}", epoch);
                ABORT.store(1, Ordering::Release);
                ok = false;
                break;
            }
            EXPECTED.store(value, Ordering::Relaxed);
            EPOCH.store(epoch, Ordering::Release);
            ok = wait_for_epoch(epoch, worker_count);
        }

        if !ok {
            ABORT.store(1, Ordering::Release);
        }
        let failed_worker = FAILED_WORKER.load(Ordering::Acquire);
        let failed_epoch = FAILED_EPOCH.load(Ordering::Acquire);
        let failed_expected = FAILED_EXPECTED.load(Ordering::Relaxed);
        let failed_observed = FAILED_OBSERVED.load(Ordering::Relaxed);

        if !ok || failed_epoch != 0 {
            println!(
                "riscv_icache_smp_smoke failed: worker={} epoch={} expected={} observed={}",
                failed_worker, failed_epoch, failed_expected, failed_observed
            );
            return 1;
        }

        println!(
            "riscv_icache_smp_smoke passed: {} updates x {} remote harts",
            UPDATE_COUNT, worker_count
        );
        0
    }
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    #[cfg(target_arch = "riscv64")]
    {
        riscv_test::run()
    }
    #[cfg(not(target_arch = "riscv64"))]
    {
        println!("riscv_icache_smp_smoke skipped: RISC-V only");
        0
    }
}
