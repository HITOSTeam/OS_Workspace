#![no_std]
#![no_main]

use user::{
    println,
    syscall::{SIGUSR1, SignalAction, exit, getpid, kill, sigaction, sigreturn},
};

extern crate user;

fn func() {
    println!("user_sig_test passed");
    sigreturn();
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let mut new = SignalAction::default();
    let mut old = SignalAction::default();
    new.handler = func as usize;

    println!("signal_simple: sigaction");
    if sigaction(SIGUSR1, Some(&new), Some(&mut old)) < 0 {
        panic!("Sigaction failed!");
    }
    println!("signal_simple: kill");
    if kill(getpid() as usize, SIGUSR1) < 0 {
        println!("Kill failed!");
        exit(1);
    }
    println!("signal_simple: Done");
    0
}
