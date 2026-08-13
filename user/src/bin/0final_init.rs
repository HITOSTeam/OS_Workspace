#![no_std]
#![no_main]

#[macro_use]
extern crate user;

use core::ptr;
use user::syscall::{
    _yield, RDONLY, chdir, close, execve, exit, fork, open, poweroff, read, sync, waitpid,
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

#[cfg(target_arch = "loongarch64")]
fn print_loongarch_buildstorm_script() {
    let path = BUILDSTORM_SCRIPT.trim_end_matches('\0');
    println!(
        "[final_init] ----- BEGIN LoongArch BuildStorm script: {} -----",
        path
    );

    let fd = open(path, RDONLY);
    if fd < 0 {
        println!("[final_init] unable to read BuildStorm script: rc={}", fd);
    } else {
        let fd = fd as usize;
        let mut buf = [0u8; 512];
        loop {
            let n = read(fd, &mut buf);
            if n < 0 {
                println!("[final_init] BuildStorm script read failed: rc={}", n);
                break;
            }
            if n == 0 {
                break;
            }
            match core::str::from_utf8(&buf[..n as usize]) {
                Ok(text) => print!("{}", text),
                Err(_) => {
                    println!("[final_init] BuildStorm script is not valid UTF-8");
                    break;
                }
            }
        }
        let _ = close(fd);
    }

    println!("\n[final_init] ----- END LoongArch BuildStorm script -----");
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
fn run_prebuilt_buildstorm_runtime() -> i32 {
    // 该函数只供 PREBUILT_ONLY 本地诊断模式调用：复用镜像里的预编译 BIN，
    // 按 axbuild/ostool 真实的 LoongArch UEFI 流程运行一次：
    //   BIN -> ESP/EFI/BOOT/BOOTLOONGARCH64.EFI
    //   EDK2 code + 可写 vars pflash + FAT ESP
    // UEFI 路径不使用 `-kernel`，否则会把 BIN/ELF 错当作 Linux 内核装载。
    // 与云端隐藏运行阶段保持相同的 30 秒硬门槛，并用外层 OS 的
    // `/proc/uptime` 单独统计内层 QEMU 墙钟时间，避免混入镜像制作和外层启动时间。
    // 固件进度码出现在日志开头，不能只看 tail；运行结束后单独摘出这些行，
    // 这样本地可以直接和云端的 `PROGRESS CODE: V03040002 I0` 对照。
    const COMMAND: &str = concat!(
        "qemu=/opt/qemu-la64/bin/qemu-system-loongarch64; ",
        "qemu_data=/opt/qemu-la64/share/qemu; ",
        "firmware=/opt/qemu-la64/share/edk2/loongarch64; ",
        "code=$firmware/code.fd; vars_template=$firmware/vars.fd; ",
        "artifact=/work/tgoskits/target/loongarch64-unknown-linux-musl/release/arceos-helloworld.bin; ",
        "runtime=/work/buildstorm-prebuilt-uefi; esp=$runtime/arceos-helloworld.esp; ",
        "vars=$runtime/arceos-helloworld.vars.fd; ",
        "rootfs=/work/tgoskits/tmp/axbuild/rootfs/arceos-loongarch64-fat32.img; ",
        "log=/work/buildstorm.prebuilt.run.out; ",
        "if [ ! -x \"$qemu\" ] || [ ! -r \"$code\" ] || [ ! -r \"$vars_template\" ] || [ ! -r \"$artifact\" ]; then ",
        "echo \"BUILDSTORM_PREBUILT_RUNTIME status=FAIL reason=missing-prerequisite qemu=$qemu code=$code vars=$vars_template artifact=$artifact\"; exit 1; fi; ",
        "mkdir -p \"$esp/EFI/BOOT\" \"$(dirname \"$rootfs\")\" || exit 1; ",
        "cp \"$artifact\" \"$esp/EFI/BOOT/BOOTLOONGARCH64.EFI\" || exit 1; ",
        "cp \"$vars_template\" \"$vars\" || exit 1; ",
        "if [ ! -r \"$rootfs\" ]; then truncate -s 64M \"$rootfs\" && mkfs.fat -F 32 \"$rootfs\" >/dev/null || exit 1; fi; ",
        "echo \"----- boot prebuilt $(basename \"$artifact\") with real UEFI flow (arch=loongarch64; timeout=30s) -----\"; ",
        "t0=$(cut -d' ' -f1 /proc/uptime 2>/dev/null); ",
        "LD_LIBRARY_PATH=/opt/qemu-la64/lib timeout -k 1 30 \"$qemu\" -L \"$qemu_data\" ",
        "-machine virt -cpu la464 -m 2G -smp 1 -nographic ",
        "-device virtio-blk-pci,drive=disk0 -drive id=disk0,if=none,format=raw,file=\"$rootfs\" ",
        "-device virtio-net-pci,netdev=net0 -netdev user,id=net0 -serial mon:stdio ",
        "-drive if=pflash,format=raw,unit=0,readonly=on,file=\"$code\" ",
        "-drive if=pflash,format=raw,unit=1,file=\"$vars\" ",
        "-drive format=raw,file=fat:rw:\"$esp\" </dev/null >\"$log\" 2>&1; rc=$?; ",
        "t1=$(cut -d' ' -f1 /proc/uptime 2>/dev/null); ",
        "elapsed=$(awk \"BEGIN{printf \\\"%.2f\\\", (\\\"$t1\\\"+0)-(\\\"$t0\\\"+0)}\" 2>/dev/null); ",
        "echo \"BUILDSTORM_PREBUILT_RUNTIME elapsed_s=$elapsed qemu_rc=$rc\"; ",
        "progress_count=$(grep -c 'PROGRESS CODE:' \"$log\" 2>/dev/null || true); ",
        "echo \"----- prebuilt EDK2 progress codes (count=$progress_count) -----\"; ",
        "grep 'PROGRESS CODE:' \"$log\" 2>/dev/null | head -n 40 || true; ",
        "echo '----- prebuilt nested QEMU output tail -----'; tail -n 120 \"$log\"; ",
        "if grep -q 'Hello, world!' \"$log\"; then echo \"BUILDSTORM_PREBUILT_RUNTIME status=PASS rc=$rc artifact=$artifact\"; exit 0; fi; ",
        "echo \"BUILDSTORM_PREBUILT_RUNTIME status=FAIL rc=$rc artifact=$artifact log=$log\"; exit 1\0",
    );
    const SHELL: &str = "/bin/sh\0";

    println!("[final_init] start prebuilt LoongArch runtime check");
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
        println!("[final_init] exec prebuilt runtime check failed: rc={}", rc);
        exit(127);
    }
    if pid < 0 {
        println!(
            "[final_init] fork prebuilt runtime check failed: rc={}",
            pid
        );
        return 127;
    }

    let mut status = 0;
    loop {
        let waited = waitpid(pid, &mut status);
        if waited == pid {
            let exit_code = decode_wait_status(status);
            println!(
                "[final_init] prebuilt runtime check exit_code={}",
                exit_code
            );
            return exit_code;
        }
        if waited < 0 {
            println!(
                "[final_init] wait prebuilt runtime check failed: pid={} rc={}",
                pid, waited
            );
            return 127;
        }
        _yield();
    }
}

#[cfg(target_arch = "loongarch64")]
fn run_prebuilt_diagnosis_and_poweroff() -> ! {
    println!("[final_init] PREBUILT_ONLY: skip CAgent and BuildStorm compilation");
    let exit_code = run_prebuilt_buildstorm_runtime();
    println!(
        "[final_init] PREBUILT_ONLY: runtime diagnosis exit_code={}",
        exit_code
    );
    let _ = sync();
    poweroff();
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

    #[cfg(target_arch = "loongarch64")]
    print_loongarch_buildstorm_script();

    #[cfg(target_arch = "loongarch64")]
    if option_env!("FINAL_PREBUILT_ONLY").is_some() {
        // 使用环境控制编译,目的是在本地测试运行流程
        run_prebuilt_diagnosis_and_poweroff();
    }

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
