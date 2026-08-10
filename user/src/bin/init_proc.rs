#![no_std]
#![no_main]
use user::{
    println,
    syscall::{_yield, exec, fork, mkdirat, mount, waitpid},
};

/// 在启动评测前挂载内核提供的运行时文件系统。
///
/// 官方 CAgent 和 BuildStorm 会访问 `/proc`、`/sys` 及 `/dev/shm`；这些目录
/// 即使存在于根镜像中，也必须通过对应文件系统挂载后才具备 Linux 语义。
fn mount_runtime_filesystems() -> bool {
    for (source, target, fs_type) in [
        ("proc\0", "/proc\0", "proc\0"),
        ("sysfs\0", "/sys\0", "sysfs\0"),
        ("devtmpfs\0", "/dev\0", "devtmpfs\0"),
        ("tmpfs\0", "/dev/shm\0", "tmpfs\0"),
    ] {
        let mkdir_rc = mkdirat(-100, target, 0o755);
        if mkdir_rc < 0 && mkdir_rc != -17 {
            println!(
                "[init_proc] mkdir {} failed: errno={}",
                target.trim_end_matches('\0'),
                -mkdir_rc
            );
            return false;
        }
        let mount_rc = mount(source, target, fs_type, 0);
        if mount_rc < 0 {
            println!(
                "[init_proc] mount {} on {} failed: errno={}",
                fs_type.trim_end_matches('\0'),
                target.trim_end_matches('\0'),
                -mount_rc
            );
            return false;
        }
    }
    true
}

#[unsafe(no_mangle)]
fn main(_argc: usize, _argv: &[&usize]) -> usize {
    println!("[init_proc] start");
    if !mount_runtime_filesystems() {
        return 1;
    }

    let child = fork();
    println!("[init_proc] fork returned {}", child);
    if child == 0 {
        if cfg!(feature = "submit") {
            println!("[init_proc] exec /local/user/0final_init");
            let exec_rc = exec("/local/user/0final_init.bin\0", &[core::ptr::null::<u8>()]);
            println!("[init_proc] exec 0final_init failed: rc={}", exec_rc);
            if exec_rc < 0 {
                exec("/local/user/00shell.bin\0", &[core::ptr::null::<u8>()]);
            }
        } else {
            exec("00shell.bin\0", &[core::ptr::null::<u8>()]);
        }
    } else if child < 0 {
        println!("[init_proc] fork failed: rc={}", child);
        return 1;
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
