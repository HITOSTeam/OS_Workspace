use core::panic::PanicInfo;

use crate::{println, syscall::exit};

#[panic_handler]
#[allow(unused)]
fn panic<'b>(info: &PanicInfo<'b>) -> ! {
    println!("PANIC: {}\n", info);
    exit(1)
}
