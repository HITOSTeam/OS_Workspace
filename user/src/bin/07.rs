#![no_std]
#![no_main]

use user::{
    self, print, println,
    syscall::{CREATE, RDONLY, RDWR, close, open, read, write},
};
#[unsafe(no_mangle)]
fn main() -> usize {
    println!("07 write test");
    let file = open("test", CREATE | RDWR);
    let mut buffer = [0u8; 4096];
    buffer[0..13].copy_from_slice(b"Hello, world!");
    let write_file = write(file as usize, buffer.as_mut());
    println!("Wrote {} bytes to file", write_file);

    return 0;
}
