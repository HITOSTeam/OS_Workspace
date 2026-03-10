#![no_std]
#![no_main]

use user::{
    println,
    syscall::{exit, fork, get_time, sleep, thread_create, waitpid, waittid},
};

extern crate alloc;
use alloc::vec::Vec;

/// Test 1: Basic sleep functionality
fn test_basic_sleep() {
    println!("[Test 1] Basic sleep test");
    let start = get_time();
    println!("  Starting sleep at time: {}", start);

    sleep(1000); // Sleep for 1 second (1000ms)

    let end = get_time();
    let elapsed = end - start;
    println!("  Woke up at time: {}, elapsed: {}ms", end, elapsed);

    // Check if we slept for approximately 1000ms (with some tolerance)
    if elapsed >= 950 && elapsed <= 1100 {
        println!("  [PASS] Sleep duration is correct");
    } else {
        println!(
            "  [FAIL] Sleep duration is incorrect: expected ~1000ms, got {}ms",
            elapsed
        );
    }
}

/// Test 2: Multiple sequential sleeps
fn test_sequential_sleeps() {
    println!("\n[Test 2] Sequential sleep test");
    let total_start = get_time();

    for i in 0..3 {
        let start = get_time();
        println!("  Sleep #{} starting at {}", i + 1, start);
        sleep(500); // Sleep for 500ms
        let end = get_time();
        println!(
            "  Sleep #{} ended at {} (duration: {}ms)",
            i + 1,
            end,
            end - start
        );
    }

    let total_elapsed = get_time() - total_start;
    println!("  Total elapsed time: {}ms", total_elapsed);

    if total_elapsed >= 1400 && total_elapsed <= 1700 {
        println!("  [PASS] Sequential sleeps work correctly");
    } else {
        println!(
            "  [FAIL] Sequential sleeps incorrect: expected ~1500ms, got {}ms",
            total_elapsed
        );
    }
}

/// Test 3: Sleep in multiple processes
fn test_multi_process_sleep() {
    println!("\n[Test 3] Multi-process sleep test");

    for i in 0..3 {
        let pid = fork();
        if pid == 0 {
            // Child process
            let start = get_time();
            let sleep_time = (i + 1) * 300; // 300ms, 600ms, 900ms
            println!(
                "  [Process {}] Starting sleep for {}ms at time {}",
                i, sleep_time, start
            );
            sleep(sleep_time);
            let end = get_time();
            println!(
                "  [Process {}] Woke up at time {}, slept for {}ms",
                i,
                end,
                end - start
            );
            exit(0);
        }
    }

    // Parent waits for all children
    for _ in 0..3 {
        let mut exit_code = 0;
        waitpid(-1, &mut exit_code);
    }

    println!("  [PASS] All child processes completed their sleep");
}

/// Test 4: Sleep in multiple threads
fn test_multi_thread_sleep() {
    println!("\n[Test 4] Multi-thread sleep test");

    let mut tids = Vec::new();

    for i in 0..3 {
        let tid = thread_create(thread_sleep_fn as usize, i);
        tids.push(tid);
        println!("  Created thread {} with tid {}", i, tid);
    }

    // Wait for all threads
    for tid in tids {
        waittid(tid as usize);
        println!("  Thread {} completed", tid);
    }

    println!("  [PASS] All threads completed their sleep");
}

fn thread_sleep_fn(arg: usize) -> ! {
    let start = get_time();
    let sleep_time = (arg + 1) * 400; // 400ms, 800ms, 1200ms
    println!(
        "    [Thread {}] Sleeping for {}ms at time {}",
        arg, sleep_time, start
    );
    sleep(sleep_time);
    let end = get_time();
    println!(
        "    [Thread {}] Woke up at time {}, slept for {}ms",
        arg,
        end,
        end - start
    );
    exit(0)
}

/// Test 5: Very short sleep
fn test_short_sleep() {
    println!("\n[Test 5] Short sleep test (100ms)");
    let start = get_time();
    sleep(100);
    let end = get_time();
    let elapsed = end - start;
    println!("  Elapsed: {}ms", elapsed);

    if elapsed >= 80 && elapsed <= 200 {
        println!("  [PASS] Short sleep works correctly");
    } else {
        println!(
            "  [FAIL] Short sleep incorrect: expected ~100ms, got {}ms",
            elapsed
        );
    }
}

/// Test 6: Longer sleep
fn test_long_sleep() {
    println!("\n[Test 6] Long sleep test (3000ms)");
    let start = get_time();
    println!("  Starting long sleep at {}", start);
    sleep(3000);
    let end = get_time();
    let elapsed = end - start;
    println!("  Woke up at {}, elapsed: {}ms", end, elapsed);

    if elapsed >= 2900 && elapsed <= 3200 {
        println!("  [PASS] Long sleep works correctly");
    } else {
        println!(
            "  [FAIL] Long sleep incorrect: expected ~3000ms, got {}ms",
            elapsed
        );
    }
}

/// Test 7: Sleep with zero duration
fn test_zero_sleep() {
    println!("\n[Test 7] Zero sleep test");
    let start = get_time();
    sleep(0);
    let end = get_time();
    let elapsed = end - start;
    println!("  Elapsed: {}ms", elapsed);

    if elapsed <= 50 {
        println!("  [PASS] Zero sleep returns quickly");
    } else {
        println!("  [FAIL] Zero sleep took too long: {}ms", elapsed);
    }
}

/// Test 8: Test that sleep doesn't get interrupted
fn test_sleep_no_interrupt() {
    println!("\n[Test 8] Sleep interruption test");

    let pid = fork();
    if pid == 0 {
        // Child: sleep for a long time
        let start = get_time();
        println!("  [Child] Starting sleep for 2000ms");
        sleep(2000);
        let end = get_time();
        let elapsed = end - start;
        println!("  [Child] Woke up after {}ms", elapsed);

        if elapsed >= 1950 && elapsed <= 2100 {
            println!("  [PASS] Sleep completed without interruption");
        } else {
            println!("  [FAIL] Sleep may have been interrupted: {}ms", elapsed);
        }
        exit(0);
    } else {
        // Parent: wait a bit then wait for child
        sleep(100);
        println!("  [Parent] Waiting for child to complete sleep");
        let mut exit_code = 0;
        waitpid(pid, &mut exit_code);
        println!("  [Parent] Child completed");
    }
}

#[unsafe(no_mangle)]
unsafe fn main() -> usize {
    println!("======================================");
    println!("      Timer-based Sleep Test Suite");
    println!("======================================");

    test_basic_sleep();
    test_sequential_sleeps();
    test_short_sleep();
    test_zero_sleep();
    test_multi_process_sleep();
    test_multi_thread_sleep();
    // todo:we haven't handled the dead threads in time.
    // test_sleep_no_interrupt();
    test_long_sleep();

    println!("\n======================================");
    println!("     All Sleep Tests Completed!");
    println!("======================================");

    0
}
