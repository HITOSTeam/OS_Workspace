#![no_std]
#![no_main]
use user::{
    println,
    syscall::{_yield, exec, fork, mkdirat, mount, waitpid},
};

#[cfg(feature = "bash-shell")]
use user::syscall::execve;

#[cfg(feature = "bash-shell")]
fn start_bash() -> isize {
    const PATH: &[u8] =
        b"PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:/user\0";
    const HOME: &[u8] = b"HOME=/root\0";
    const TERM: &[u8] = b"TERM=xterm-256color\0";
    const SHELL: &[u8] = b"SHELL=/bin/bash\0";
    const PS1: &[u8] = b"PS1=CongCore:\\w$ \0";

    let argv = [
        c"bash".as_ptr().cast(),
        c"--noprofile".as_ptr().cast(),
        c"--norc".as_ptr().cast(),
        c"-i".as_ptr().cast(),
        core::ptr::null(),
    ];
    let envp = [
        PATH.as_ptr(),
        HOME.as_ptr(),
        TERM.as_ptr(),
        SHELL.as_ptr(),
        PS1.as_ptr(),
        core::ptr::null(),
    ];
    execve("/bin/bash\0", &argv, &envp)
}

#[unsafe(no_mangle)]
fn main(_argc: usize, _argv: &[&usize]) -> usize {
    println!("[init_proc] start");
    for (source, target, fs_type) in [
        ("proc\0", "/proc\0", "proc\0"),
        ("sysfs\0", "/sys\0", "sysfs\0"),
        ("devtmpfs\0", "/dev\0", "devtmpfs\0"),
        // Linux documents /dev/shm as a user-visible tmpfs mount used by
        // glibc shm_open/shm_unlink. Keep it distinct from devtmpfs so POSIX
        // shared-memory names are ordinary VFS dentries and inodes.
        ("tmpfs\0", "/dev/shm\0", "tmpfs\0"),
    ] {
        let mkdir_rc = mkdirat(-100, target, 0o755);
        if mkdir_rc < 0 && mkdir_rc != -17 {
            println!(
                "[init_proc] mkdir mountpoint {} failed: errno={}",
                target.trim_end_matches('\0'),
                -mkdir_rc
            );
            return 1;
        }
        let mount_rc = mount(source, target, fs_type, 0);
        if mount_rc < 0 {
            println!(
                "[init_proc] mount {} on {} failed: errno={}",
                fs_type.trim_end_matches('\0'),
                target.trim_end_matches('\0'),
                -mount_rc
            );
            return 1;
        }
    }
    if fork() == 0 {
        if cfg!(feature = "submit") {
            if exec("submit_script.bin\0", &[core::ptr::null::<u8>()]) < 0 {
                exec("00shell.bin\0", &[core::ptr::null::<u8>()]);
            }
        } else if cfg!(feature = "bash-shell") {
            #[cfg(feature = "bash-shell")]
            if start_bash() < 0 {
                println!("[init_proc] /bin/bash failed; falling back to 00shell");
                exec("00shell.bin\0", &[core::ptr::null::<u8>()]);
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
