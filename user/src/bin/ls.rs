#![no_std]
#![no_main]

#[macro_use]
extern crate user;

use user::syscall::{RDONLY, close, getdents64, open};

#[unsafe(no_mangle)]
pub fn main(argc: usize, argv: &[&str]) -> i32 {
    let path = if argc > 1 { argv[1] } else { "." };
    let fd = open(path, RDONLY);
    if fd < 0 {
        println!("ls: cannot open '{}'", path);
        return -1;
    }
    let fd = fd as usize;

    let mut buf = [0u8; 1024];
    let mut first = true;
    loop {
        let n = getdents64(fd, &mut buf);
        if n == 0 {
            break;
        }
        if n < 0 {
            println!("{}", path);
            let _ = close(fd);
            return 0;
        }

        let mut pos = 0usize;
        let n = n as usize;
        while pos + 19 <= n {
            let reclen = u16::from_le_bytes([buf[pos + 16], buf[pos + 17]]) as usize;
            if reclen == 0 || pos + reclen > n {
                break;
            }
            let dtype = buf[pos + 18];
            let name_start = pos + 19;
            let name_end = pos + reclen;
            let nul = buf[name_start..name_end]
                .iter()
                .position(|&b| b == 0)
                .unwrap_or(name_end - name_start);
            let name_bytes = &buf[name_start..name_start + nul];
            if let Ok(name) = core::str::from_utf8(name_bytes) {
                if name != "." && name != ".." {
                    if !first {
                        print!(" ");
                    }
                    first = false;
                    if dtype == 4 {
                        print!("{}/", name);
                    } else {
                        print!("{}", name);
                    }
                }
            }
            pos += reclen;
        }
    }

    println!("");
    let _ = close(fd);
    0
}
