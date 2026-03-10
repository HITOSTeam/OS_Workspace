#![no_std]
#![no_main]

#[macro_use]
extern crate user;

fn basename(path: &str) -> &str {
    if path.is_empty() {
        return ".";
    }
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return "/";
    }
    trimmed.rsplit('/').next().unwrap_or("/")
}

#[unsafe(no_mangle)]
pub fn main(argc: usize, argv: &[&str]) -> i32 {
    if argc < 2 || argc > 3 {
        println!("usage: basename NAME [SUFFIX]");
        return -1;
    }

    let base = basename(argv[1]);
    let suffix = if argc == 3 { argv[2] } else { "" };
    let output = if !suffix.is_empty() && base.ends_with(suffix) && base.len() > suffix.len() {
        &base[..base.len() - suffix.len()]
    } else {
        base
    };

    println!("{}", output);
    0
}
