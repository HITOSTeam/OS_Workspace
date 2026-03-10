#![no_std]
#![no_main]

use user::{
    self, print, println,
    syscall::{CREATE, RDONLY, RDWR, close, open, read},
};
#[unsafe(no_mangle)]
fn main() -> usize {
    println!("06 read test");
    let file = open("test", RDONLY);
    let mut buffer = [0u8; 4096];
    let read_file = read(file as usize, buffer.as_mut());
    for i in 0..read_file {
        if buffer[i as usize] != 0 {
            print!("{}", buffer[i as usize] as char);
        }
    }

    return 0;
}
