#![no_std]
#![no_main]
use user::{
    println,
    syscall::{_yield, exec, fork, waitpid},
};

#[unsafe(no_mangle)]
fn main(argc: usize, argv: &[&usize]) -> usize {
    println!("[init_proc] start");
    if fork() == 0 {
        if cfg!(feature = "submit") {
            // 暂时先使用这个
            if exec("0final_init.bin\0", &[core::ptr::null::<u8>()]) < 0 {
                exec("00shell.bin\0", &[core::ptr::null::<u8>()]);
                // if exec("submit_script.bin\0", &[core::ptr::null::<u8>()]) < 0 {
                // }
            }
        } else {
            exec("00shell.bin\0", &[core::ptr::null::<u8>()]);
        }
    } else {
        loop {
            let mut _exit = 0;
            let pid = waitpid(-1, &mut _exit);
            if pid > 0 {
                // println!("[init_proc] the exit pid is {}", pid);
            } else {
                _yield();
            }
        }
    }
    0
}
