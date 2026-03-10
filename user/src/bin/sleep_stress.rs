#![no_std]
#![no_main]

use alloc::vec::Vec;
use user::{
    println,
    syscall::{exit, get_time, sleep, thread_create, waittid},
};
extern crate alloc;

const THREAD_COUNT: usize = 10;

fn sleeper_thread(id: usize) -> ! {
    let sleep_duration = ((id % 5) + 1) * 200; // 200ms to 1000ms
    let start = get_time();
    println!("  Thread {}: sleeping for {}ms", id, sleep_duration);
    sleep(sleep_duration);
    let end = get_time();
    let actual = end - start;
    println!(
        "  Thread {}: woke up (expected {}ms, actual {}ms, diff {}ms)",
        id,
        sleep_duration,
        actual,
        (actual as isize - sleep_duration as isize).abs()
    );
    exit(0)
}

#[unsafe(no_mangle)]
unsafe fn main() -> usize {
    println!("=== Sleep Stress Test ===");
    println!(
        "Creating {} threads with different sleep times...\n",
        THREAD_COUNT
    );

    let start_time = get_time();
    let mut threads = Vec::new();

    // Create multiple threads
    for i in 0..THREAD_COUNT {
        let tid = thread_create(sleeper_thread as usize, i);
        threads.push(tid);
    }

    println!("All threads created. Waiting for completion...\n");

    // Wait for all threads
    for (i, tid) in threads.iter().enumerate() {
        let wait_start = get_time();
        waittid(*tid as usize);
        let wait_time = get_time() - wait_start;
        println!(
            "Thread {} (tid={}) joined after waiting {}ms",
            i, tid, wait_time
        );
    }

    let total_time = get_time() - start_time;
    println!("\n=== Test Complete ===");
    println!("Total execution time: {}ms", total_time);
    println!("Expected: ~1000ms (longest sleep duration)");
    println!("All {} threads completed successfully!", THREAD_COUNT);

    0
}
