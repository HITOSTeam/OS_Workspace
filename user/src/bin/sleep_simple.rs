#![no_std]
#![no_main]

use user::{
    println,
    syscall::{exit, fork, get_time, sleep, waitpid},
};

#[unsafe(no_mangle)]
unsafe fn main() -> usize {
    println!("=== Simple Sleep Test ===");

    // Test 1: Basic sleep
    println!("\n[Test 1] Sleep for 1 second...");
    let t1 = get_time();
    sleep(1000);
    let t2 = get_time();
    println!("Expected: ~1000ms, Actual: {}ms", t2 - t1);

    // Test 2: Two processes sleeping concurrently
    println!("\n[Test 2] Two processes sleeping...");
    let start = get_time();

    let pid1 = fork();
    if pid1 == 0 {
        println!("  Child 1: sleeping 500ms");
        sleep(500);
        println!("  Child 1: done!");
        exit(0);
    }

    let pid2 = fork();
    if pid2 == 0 {
        println!("  Child 2: sleeping 1000ms");
        sleep(1000);
        println!("  Child 2: done!");
        exit(0);
    }

    // Wait for both children
    let mut exit_code = 0;
    waitpid(-1, &mut exit_code);
    waitpid(-1, &mut exit_code);

    let total = get_time() - start;
    println!("Both children done. Total time: {}ms", total);
    println!("(Should be ~1000ms since they run concurrently)");

    // Test 3: Sequential sleeps should add up
    println!("\n[Test 3] Sequential sleeps...");
    let t1 = get_time();
    sleep(300);
    sleep(300);
    sleep(400);
    let t2 = get_time();
    println!("Expected: ~1000ms, Actual: {}ms", t2 - t1);

    println!("\n=== All Tests Complete ===");
    0
}
