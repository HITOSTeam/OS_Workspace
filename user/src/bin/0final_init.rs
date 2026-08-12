#![no_std]
#![no_main]

#[macro_use]
extern crate user;

use core::ptr;
use user::syscall::{
    _yield, RDONLY, chdir, close, execve, exit, fork, open, poweroff, sync, waitpid,
};

const EVAL_DIR: &str = "/glibc";
const BUILDSTORM_SCRIPT: &str = "/glibc/buildstorm_testcode.sh\0";
const CAGENT_SCRIPT: &str = "/glibc/cagent_testcode.sh\0";

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

#[cfg(target_arch = "loongarch64")]
fn run_local_ual_runtime_check() {
    // QEMU 安装在 /opt 下，必须显式指定数据目录；否则会在加载 efi-virtio.rom
    // 时提前退出，尚未执行到云端实际失败的 UAL 能力检查。
    const COMMAND: &str = "LD_LIBRARY_PATH=/opt/qemu-la64/lib timeout -k 1 5 /opt/qemu-la64/bin/qemu-system-loongarch64 -L /opt/qemu-la64/share/qemu -machine virt -cpu la464 -m 128M -smp 1 -nographic -S; rc=$?; if [ $rc -eq 124 ] || [ $rc -eq 137 ]; then echo 'LOCAL_UAL_RUNTIME_CHECK=PASS'; exit 0; fi; echo LOCAL_UAL_RUNTIME_CHECK=FAIL rc=$rc; exit $rc\0";
    const SHELL: &str = "/bin/sh\0";

    println!("[final_init] start local LoongArch UAL runtime check");
    let pid = fork();
    if pid == 0 {
        let argv = [
            SHELL.as_ptr(),
            c"-c".as_ptr().cast(),
            COMMAND.as_ptr(),
            ptr::null(),
        ];
        let envp = [
            c"PATH=/root/.cargo/bin:/usr/local/bin:/usr/bin:/bin:/sbin:/usr/sbin"
                .as_ptr()
                .cast(),
            c"HOME=/root".as_ptr().cast(),
            c"TERM=vt100".as_ptr().cast(),
            ptr::null(),
        ];
        let rc = execve(SHELL, &argv, &envp);
        println!("[final_init] exec local UAL check failed: rc={}", rc);
        exit(127);
    }
    if pid < 0 {
        println!("[final_init] fork local UAL check failed: rc={}", pid);
        return;
    }

    let mut status = 0;
    loop {
        let waited = waitpid(pid, &mut status);
        if waited == pid {
            println!(
                "[final_init] local UAL runtime check exit_code={}",
                decode_wait_status(status)
            );
            return;
        }
        if waited < 0 {
            println!(
                "[final_init] wait local UAL check failed: pid={} rc={}",
                pid, waited
            );
            return;
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
        && path_exists("/bin/bash")
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

    // 本地 LoongArch 诊断入口；上传云端前直接注释下一行即可。
    #[cfg(target_arch = "loongarch64")]
    run_local_ual_runtime_check();

    let exit_code = if cagent_exit_code != 0 {
        cagent_exit_code
    } else if buildstorm_exit_code != 0 {
        buildstorm_exit_code
    } else {
        0
    };

    println!("[final_init] serial evaluation exit_code={}", exit_code);
    let _ = sync();
    poweroff();
}
