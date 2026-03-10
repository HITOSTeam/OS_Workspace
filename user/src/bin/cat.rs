#![no_std]
#![no_main]

#[macro_use]
extern crate user;

use user::syscall::{close, open, read, RDONLY};

const STDIN_FD: usize = 0;

fn cat_fd(fd: usize) -> i32 {
    let mut buf = [0u8; 512];
    loop {
        let n = read(fd, &mut buf);
        if n < 0 {
            return -1;
        }
        if n == 0 {
            return 0;
        }
        let n = n as usize;
        match core::str::from_utf8(&buf[..n]) {
            Ok(text) => print!("{}", text),
            Err(_) => return -1,
        }
    }
}

#[unsafe(no_mangle)]
pub fn main(argc: usize, argv: &[&str]) -> i32 {
    if argc <= 1 {
        return cat_fd(STDIN_FD);
    }

    let mut status = 0;
    for path in argv.iter().skip(1) {
        let fd = open(path, RDONLY);
        if fd < 0 {
            println!("cat: cannot open '{}'", path);
            status = 1;
            continue;
        }
        let fd = fd as usize;
        if cat_fd(fd) != 0 {
            println!("cat: read error on '{}'", path);
            status = 1;
        }
        let _ = close(fd);
    }
    status
}
