#![no_std]
#![no_main]

#[macro_use]
extern crate user;

use core::ptr;
use user::syscall::{
    _yield, RDONLY, chdir, close, execve, exit, fork, open, poweroff, sync, waitpid,
};

const BUILDSTORM_SCRIPT: &str = "/buildstorm_testcode.sh\0";
const CAGENT_SCRIPT: &str = "/cagent_testcode.sh\0";

fn path_exists(path: &str) -> bool {
    let fd = open(path, RDONLY);
    if fd < 0 {
        return false;
    }
    let _ = close(fd as usize);
    true
}

fn decode_wait_status(status: i32) -> i32 {
    let signal = status & 0x7f;
    if signal != 0 {
        128 + signal
    } else {
        (status >> 8) & 0xff
    }
}

fn exec_script(shell: &'static str, script: &'static str) -> ! {
    let argv = [shell.as_ptr(), script.as_ptr(), ptr::null()];
    let envp = [
        c"PATH=/root/.cargo/bin:/usr/local/bin:/usr/bin:/bin:/sbin:/usr/sbin"
            .as_ptr()
            .cast(),
        c"HOME=/root".as_ptr().cast(),
        c"TERM=vt100".as_ptr().cast(),
        ptr::null(),
    ];

    let rc = execve(shell, &argv, &envp);
    println!(
        "[final_init] exec failed: shell={} script={} rc={}",
        shell.trim_end_matches('\0'),
        script.trim_end_matches('\0'),
        rc
    );
    exit(127);
}

fn run_script(shell: &'static str, script: &'static str) -> i32 {
    let pid = fork();
    if pid == 0 {
        exec_script(shell, script);
    }
    if pid < 0 {
        println!("[final_init] fork failed: rc={}", pid);
        return 127;
    }

    let mut status = 0;
    loop {
        let waited = waitpid(pid, &mut status);
        if waited == pid {
            return decode_wait_status(status);
        }
        if waited < 0 {
            println!("[final_init] waitpid failed: pid={} rc={}", pid, waited);
            return 127;
        }
        _yield();
    }
}

fn reap_remaining_children() {
    loop {
        let mut status = 0;
        if waitpid(-1, &mut status) <= 0 {
            break;
        }
    }
}

#[unsafe(no_mangle)]
pub fn main(_argc: usize, _argv: &[&str]) -> i32 {
    // println!("[final_init] automatic evaluation init started");
    let _ = chdir("/");

    let cagent_ready =
        path_exists("/busybox") && path_exists("/simple_llm_server") && path_exists("/agent_lite");
    let buildstorm_ready = path_exists("/work/tgoskits");

    let (name, exit_code) = if cagent_ready {
        println!("[final_init] detected CAgent payload");
        ("cagent", run_script("/bin/bash\0", CAGENT_SCRIPT))
    } else if buildstorm_ready {
        println!("[final_init] detected BuildStorm payload");
        ("buildstorm", run_script("/bin/sh\0", BUILDSTORM_SCRIPT))
    } else {
        println!(
            "[final_init] no evaluation payload found: expected /work/tgoskits or CAgent binaries in /"
        );
        ("none", 127)
    };

    reap_remaining_children();
    println!(
        "[final_init] evaluation finished: suite={} exit_code={}",
        name, exit_code
    );
    let _ = sync();
    poweroff();
}
