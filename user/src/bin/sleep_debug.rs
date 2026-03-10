#![no_std]
#![no_main]

use user::{
    println,
    syscall::{get_time, sleep},
};

#[unsafe(no_mangle)]
unsafe fn main() -> usize {
    println!("=== Sleep Debug Test ===\n");

    // Test 1: Very short sleep (100ms)
    println!("[Test 1] Sleep for 100ms...");
    let before = get_time();
    println!("  Before sleep: {}", before);
    sleep(100);
    let after = get_time();
    println!("  After sleep: {}", after);
    println!("  Difference: {}", after - before);
    println!("  Expected: ~100ms");

    // Test 2: Sleep for 500ms
    println!("\n[Test 2] Sleep for 500ms...");
    let before = get_time();
    println!("  Before sleep: {}", before);
    sleep(500);
    let after = get_time();
    println!("  After sleep: {}", after);
    println!("  Difference: {}", after - before);
    println!("  Expected: ~500ms");

    println!("\n=== All Tests Complete ===");
    0
}
