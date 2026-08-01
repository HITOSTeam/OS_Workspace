#![no_std]
#![no_main]

extern crate alloc;
#[macro_use]
extern crate user;

#[cfg(target_arch = "loongarch64")]
use alloc::{string::String, vec::Vec};
use core::ptr;
use user::syscall::{
    _yield, RDONLY, chdir, close, execve, exit, fork, open, poweroff, sync, waitpid,
};
#[cfg(target_arch = "loongarch64")]
use user::syscall::{CREATE, TRUNC, WRONLY, read, write};

const EVAL_DIR: &str = "/glibc";
const BUILDSTORM_SCRIPT: &str = "/glibc/buildstorm_testcode.sh\0";
const CAGENT_SCRIPT: &str = "/glibc/cagent_testcode.sh\0";

//loongarch部分不能执行官方的原版镜像,使用loongarch里面的bash执行会报错
#[cfg(target_arch = "loongarch64")]
const LOONGARCH_CAGENT_SCRIPT_PATH: &str = "/tmp/cagent_testcode-posix.sh";
#[cfg(target_arch = "loongarch64")]
const LOONGARCH_CAGENT_SCRIPT: &str = "/tmp/cagent_testcode-posix.sh\0";

///判断文件是不是存在
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

#[cfg(target_arch = "loongarch64")]
fn read_file(path: &str) -> Option<Vec<u8>> {
    let fd = open(path, RDONLY);
    if fd < 0 {
        return None;
    }
    let fd = fd as usize;
    let mut data = Vec::new();
    let mut buf = [0u8; 512];
    loop {
        let n = read(fd, &mut buf);
        if n < 0 {
            let _ = close(fd);
            return None;
        }
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buf[..n as usize]);
    }
    let _ = close(fd);
    Some(data)
}

#[cfg(target_arch = "loongarch64")]
fn write_file(path: &str, mut data: &[u8]) -> bool {
    let fd = open(path, WRONLY | CREATE | TRUNC);
    if fd < 0 {
        return false;
    }
    let fd = fd as usize;
    while !data.is_empty() {
        let n = write(fd, data);
        if n <= 0 {
            let _ = close(fd);
            return false;
        }
        data = &data[n as usize..];
    }
    close(fd) == 0
}

#[cfg(target_arch = "loongarch64")]
fn prepare_loongarch_cagent_script() -> bool {
    let Some(bytes) = read_file(CAGENT_SCRIPT.trim_end_matches('\0')) else {
        println!("[final_init] failed to read the official CAgent script");
        return false;
    };
    let Ok(source) = String::from_utf8(bytes) else {
        println!("[final_init] official CAgent script is not UTF-8");
        return false;
    };

    // 官方脚本仅使用 Bash 数组保存子进程 PID。LoongArch 官方 Bash 当前
    // 无法在本内核启动，因此保留所有测试命令，只把 PID 记录改成 POSIX sh 写法。
    let append_count = source.matches("TEST_PIDS+=($!)").count();
    if !source.contains("TEST_PIDS=()")
        || append_count == 0
        || !source.contains("wait \"${TEST_PIDS[@]}\"")
    {
        println!("[final_init] unrecognized official CAgent script layout");
        return false;
    }
    let adapted = source
        .replace("TEST_PIDS=()", "TEST_PIDS=\"\"")
        .replace("TEST_PIDS+=($!)", "TEST_PIDS=\"$TEST_PIDS $!\"")
        .replace(
            "wait \"${TEST_PIDS[@]}\"",
            "for test_pid in $TEST_PIDS; do wait \"$test_pid\"; done",
        );
    println!(
        "[final_init] adapted {} CAgent PID-array operations for LoongArch",
        append_count
    );
    write_file(LOONGARCH_CAGENT_SCRIPT_PATH, adapted.as_bytes())
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

#[cfg(target_arch = "loongarch64")]
fn exec_busybox_script(script: &'static str) -> ! {
    const BUSYBOX: &str = "/glibc/busybox\0";
    const SH: &str = "sh\0";
    let argv = [BUSYBOX.as_ptr(), SH.as_ptr(), script.as_ptr(), ptr::null()];
    let envp = [
        c"PATH=/root/.cargo/bin:/usr/local/bin:/usr/bin:/bin:/sbin:/usr/sbin"
            .as_ptr()
            .cast(),
        c"HOME=/root".as_ptr().cast(),
        c"TERM=vt100".as_ptr().cast(),
        ptr::null(),
    ];

    let rc = execve(BUSYBOX, &argv, &envp);
    println!(
        "[final_init] exec failed: shell={} script={} rc={}",
        BUSYBOX.trim_end_matches('\0'),
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

#[cfg(target_arch = "loongarch64")]
fn run_loongarch_cagent_script() -> i32 {
    if !prepare_loongarch_cagent_script() {
        return 127;
    }

    let pid = fork();
    if pid == 0 {
        exec_busybox_script(LOONGARCH_CAGENT_SCRIPT);
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

///回收剩下的进程
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

    let cagent_ready = path_exists(CAGENT_SCRIPT.trim_end_matches('\0'))
        && path_exists("/glibc/busybox")
        && path_exists("/glibc/simple_llm_server")
        && path_exists("/glibc/agent_lite");
    let buildstorm_ready =
        path_exists(BUILDSTORM_SCRIPT.trim_end_matches('\0')) && path_exists("/work/tgoskits");

    if !cagent_ready || !buildstorm_ready {
        println!(
            "[final_init] evaluation payload incomplete: cagent={} buildstorm={}",
            cagent_ready, buildstorm_ready
        );
        let _ = sync();
        poweroff();
    }

    println!("[final_init] detected CAgent payload");
    let _ = chdir(EVAL_DIR);
    #[cfg(target_arch = "loongarch64")]
    let cagent_exit_code = run_loongarch_cagent_script();
    #[cfg(not(target_arch = "loongarch64"))]
    let cagent_exit_code = run_script("/bin/bash\0", CAGENT_SCRIPT);
    reap_remaining_children();
    println!(
        "[final_init] evaluation finished: suite=cagent exit_code={}",
        cagent_exit_code
    );

    println!("[final_init] detected BuildStorm payload");
    let _ = chdir(EVAL_DIR);
    let buildstorm_exit_code = run_script("/bin/sh\0", BUILDSTORM_SCRIPT);
    reap_remaining_children();
    println!(
        "[final_init] evaluation finished: suite=buildstorm exit_code={}",
        buildstorm_exit_code
    );

    let exit_code = if cagent_exit_code != 0 {
        cagent_exit_code
    } else {
        buildstorm_exit_code
    };

    println!("[final_init] serial evaluation exit_code={}", exit_code);
    let _ = sync();
    poweroff();
}
