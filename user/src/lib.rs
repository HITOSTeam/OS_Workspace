#![allow(unreachable_code)]
#![no_std]
#![no_main]
#![feature(linkage)]
#![feature(alloc_error_handler)]
pub mod console;

mod lang_items;
pub mod syscall;

use alloc::vec::Vec;
use buddy_system_allocator::LockedHeap;
use core::ptr::addr_of_mut;
use core::str;
// Increase user-space heap to reduce alloc panics in heavier apps (e.g., shell).
const USER_HEAP_SIZE: usize = 0x40000; // 256 KiB
extern crate alloc;
static mut HEAP_SPACE: [u8; USER_HEAP_SIZE] = [0; USER_HEAP_SIZE];

#[global_allocator]
static HEAP: LockedHeap = LockedHeap::empty();

#[cfg(target_arch = "loongarch64")]
fn early_user_write(msg: &'static [u8]) {
    let _ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall 0",
            inlateout("$r4") 1usize => _ret,
            in("$r5") msg.as_ptr() as usize,
            in("$r6") msg.len(),
            in("$r7") 0usize,
            in("$r8") 0usize,
            in("$r9") 0usize,
            in("$r11") 64usize,
        );
    }
}

#[alloc_error_handler]
pub fn handle_alloc_error(layout: core::alloc::Layout) -> ! {
    panic!("Heap allocation error, layout = {:?}", layout);
}

fn clear_bss() {
    unsafe extern "C" {
        safe fn sbss();
        safe fn ebss();
    }
    (sbss as *const () as usize..ebss as *const () as usize).for_each(|addr| unsafe {
        (addr as *mut u8).write_volatile(0);
    });
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.entry")]
pub extern "C" fn _start(_argc: usize, argv: usize) {
    #[cfg(target_arch = "loongarch64")]
    early_user_write(b"[user _start] enter\n");
    clear_bss();
    unsafe {
        HEAP.lock()
            .init(addr_of_mut!(HEAP_SPACE) as usize, USER_HEAP_SIZE);
    }
    let mut final_parameters: Vec<&str> = Vec::new();
    let ptr_size = core::mem::size_of::<usize>();
    if argv != 0 {
        let mut index = 0usize;
        loop {
            let base = argv + index * ptr_size;
            let mut raw = [0u8; core::mem::size_of::<usize>()];
            // argv is a pointer to the array of pointers
            // we need to read the pointer first
            for (offset, byte) in raw.iter_mut().enumerate() {
                *byte = unsafe { *((base + offset) as *const u8) };
            }
            let arg_ptr = usize::from_ne_bytes(raw);
            if arg_ptr == 0 {
                break;
            }
            let mut len = 0usize;
            while unsafe { *((arg_ptr + len) as *const u8) } != 0 {
                len += 1;
            }
            unsafe {
                let string =
                    str::from_utf8(core::slice::from_raw_parts(arg_ptr as *const u8, len)).unwrap();
                final_parameters.push(string);
                // println!("[user] Arg {} : {}", index, string);
            }
            index += 1;
        }
    }
    let parsed_argc = final_parameters.len();
    // println!("[user] argc register {} -> parsed {}", argc, parsed_argc);
    syscall::exit(main(parsed_argc, final_parameters.as_slice()) as isize);
    panic!("Execution should never reach here after exit syscall!");
}
#[linkage = "weak"]
#[unsafe(no_mangle)]
fn main(_argc: usize, _argv: &[&str]) -> usize {
    println!("Hello, world!");
    return 0;
}
