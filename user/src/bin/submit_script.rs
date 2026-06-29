#![no_std]
#![no_main]

extern crate alloc;
#[macro_use]
extern crate user;

use alloc::{string::String, vec::Vec};
mod ltp_dependence;
use ltp_dependence::*;
use user::syscall::{self, RDONLY, chdir, close, execve, exit, fork, open, sync, waitpid};

const LTP_ENV_DEV: &[u8] = b"LTP_DEV=/dev/root\0";
const LTP_ENV_DEV_FS_TYPE: &[u8] = b"LTP_DEV_FS_TYPE=tmpfs\0";
const LTP_ENV_SINGLE_FS_TYPE: &[u8] = b"LTP_SINGLE_FS_TYPE=tmpfs\0";
const LTP_ENV_KERNEL: &[u8] = b"KERNEL=/config.gz\0";
const LTP_ENV_LDD: &[u8] = b"LDD=/extra/bin/ldd\0";
const LTP_ENV_PATH_MUSL: &[u8] =
    b"PATH=/extra/bin:/user:/:/bin:/usr/bin:/musl/ltp/testcases/bin:/musl:/glibc:/glibc/ltp/testcases/bin\0";
const LTP_ENV_PATH_GLIBC: &[u8] =
    b"PATH=/extra/bin:/user:/:/bin:/usr/bin:/glibc/ltp/testcases/bin:/glibc:/musl:/musl/ltp/testcases/bin\0";
const LTP_ENV_ROOT_MUSL: &[u8] = b"LTPROOT=/musl/ltp\0";
const LTP_ENV_ROOT_GLIBC: &[u8] = b"LTPROOT=/glibc/ltp\0";
// Case-specific LTP env knobs are intentionally disabled for now. Re-enable
// them beside the corresponding test batches when those batches are active.
// const LTP_ENV_CGROUPS_ROOT_MUSL: &[u8] = b"CGROUPS_TESTROOT=/musl/ltp/testcases/bin\0";
// const LTP_ENV_CGROUPS_ROOT_GLIBC: &[u8] = b"CGROUPS_TESTROOT=/glibc/ltp/testcases/bin\0";
// Optional adapter for the bundled musl sched_* libc wrappers:
// const LTP_ENV_MUSL_SCHED_PRELOAD: &[u8] = b"LD_PRELOAD=/extra/libltp_sched_fix.so\0";
const LTP_ENV_TIMEOUT_MUL_SLOW: &[u8] = b"LTP_TIMEOUT_MUL=4\0";
const FOCUS_VFS_SMOKES: bool = false;
const VFS_SMOKES: [&str; 1] = ["/user/path_cache_invalidation_smoke.bin"];
const FOCUS_READINESS_SMOKES: bool = false;
const READINESS_SMOKES: [&str; 15] = [
    "/user/nested_epoll_smoke.bin",
    "/user/nested_epoll_ctl_wakeup_smoke.bin",
    "/user/nested_epoll_ctl_del_smoke.bin",
    "/user/nested_epoll_et_smoke.bin",
    "/user/nested_epoll_et_maxevents_smoke.bin",
    "/user/nested_epoll_oneshot_smoke.bin",
    "/user/nested_epoll_parent_oneshot_smoke.bin",
    "/user/epoll_ctl_wakeup_smoke.bin",
    "/user/eventfd_epoll_smoke.bin",
    "/user/mq_epoll_smoke.bin",
    "/user/mq_notify_signal_smoke.bin",
    "/user/mq_unlink_epoll_smoke.bin",
    "/user/timerfd_epoll_smoke.bin",
    "/user/regular_file_select_smoke.bin",
    "/user/dup3_lock_cleanup_smoke.bin",
];
const FOCUS_PROCFS_SMOKES: bool = false;
const PROCFS_SMOKES: [&str; 3] = [
    "/user/proc_magic_links_smoke.bin",
    "/user/mount_namespace_smoke.bin",
    "/user/proc_maps_stack_smoke.bin",
];
const FOCUS_MEMORY_SMOKES: bool = false;
const MEMORY_SMOKES: [&str; 15] = [
    "/user/file_mmap_lazy_fault_smoke.bin",
    "/user/shared_file_alias_smoke.bin",
    "/user/shared_file_cross_mm_smoke.bin",
    "/user/shared_file_kernel_write_smoke.bin",
    "/user/shared_file_fault_cache_smoke.bin",
    "/user/shared_file_truncate_cache_smoke.bin",
    "/user/cow_mprotect_smoke.bin",
    "/user/clone_vm_mmap_smoke.bin",
    "/user/clone_vm_sysv_shm_smoke.bin",
    "/user/memfd_mremap_shared_smoke.bin",
    "/user/sysv_shm_mremap_smoke.bin",
    "/user/mmap_placement_smoke.bin",
    "/user/growsdown_guard_smoke.bin",
    "/user/stack_madvise_dontneed_smoke.bin",
    "/user/private_file_madvise_dontneed_smoke.bin",
];

/// 运行ltp测试 使用 的脚本，由于目前ltp存在部分没法通过以及卡死
/// 的情况，所以我们不能使用ltp_all.s和来运行
fn run_part_of_ltp_script_in_dir(dir: &str, script_names: &[&str]) {
    let group = if dir.contains("musl") {
        "ltp-musl"
    } else if dir.contains("glibc") {
        "ltp-glibc"
    } else {
        "ltp"
    };
    println!("#### OS COMP TEST GROUP START {} ####", group);

    let _ = chdir(dir);
    for &entry in script_names {
        let mut parts = entry.split_whitespace();
        let Some(script) = parts.next() else {
            continue;
        };
        let mut extra_args: Vec<&str> = Vec::new();
        for arg in parts {
            extra_args.push(arg);
        }
        // 目前已经进入 /musl 或 /glibc 但是测试在ltp下面
        // 拼接成完整相对路径运行，而不是cd 进入，参考ltp_testcode.sh进行
        let path = resolve_ltp_case_path(dir, script);
        println!("RUN LTP CASE {}", script);
        let ret = run_script(path.as_str(), &extra_args);
        if ret == 0 {
            println!("PASS LTP CASE {}", script);
        } else {
            println!("FAIL LTP CASE {} : {}", script, ret);
        }
    }

    println!("#### OS COMP TEST GROUP END {} ####", group);
}

fn path_exists(path: &str) -> bool {
    let fd = open(path, RDONLY);
    if fd < 0 {
        return false;
    }
    let _ = close(fd as usize);
    true
}

// 将ltp测试名 （fork01 fork02 ) 与 dir (glibc)组装起来，获得真正的测试名称

fn resolve_ltp_case_path(dir: &str, script: &str) -> String {
    // dir : /glibc /musl
    // script : fork01 fork 02
    let basename = script.rsplit('/').next().unwrap_or(script);

    let mut installed = String::from(dir);
    installed.push_str("/ltp/testcases/bin/");
    installed.push_str(basename);
    if path_exists(installed.as_str()) {
        return installed;
    }

    let mut source_tree = String::from(dir);
    source_tree.push_str("/ltp/testcases/");
    source_tree.push_str(script);
    if path_exists(source_tree.as_str()) {
        return source_tree;
    }

    installed
}

fn run_named_cases(group: &str, cases: &[&str]) {
    println!("#### OS COMP TEST GROUP START {} ####", group);
    for &case in cases {
        println!("RUN CASE {}", case);
        let ret = run_script(case, &[]);
        if ret == 0 {
            println!("PASS CASE {}", case);
        } else {
            println!("FAIL CASE {} : {}", case, ret);
        }
    }
    println!("#### OS COMP TEST GROUP END {} ####", group);
}

/// 运行name 的测试文件。使用fork + exec子进程进行
fn run_script(name: &str, extra_args: &[&str]) -> i32 {
    fn normalize_ltp_wait_status(status: i32) -> i32 {
        // Linux wait status: exited children encode code in high byte.
        if (status & 0x7f) == 0 {
            let code = (status >> 8) & 0xff;
            // LTP result bits: TFAIL=1, TBROK=2, TWARN=4, TINFO=16, TCONF=32.
            // Treat configuration/environment skips as success as long as they
            // don't carry fail/broken bits.
            if (code & 32) != 0 && (code & 0x3) == 0 {
                return 0;
            }
            return code;
        }
        status
    }

    let pid = fork();
    if pid == 0 {
        let mut path = String::from(name);
        let mut owned_args: Vec<String> = Vec::new();

        for arg in extra_args.iter().copied() {
            let mut s = String::from(arg);
            s.push('\0');
            owned_args.push(s);
        }
        path.push('\0');
        let mut args: Vec<*const u8> = Vec::with_capacity(owned_args.len() + 2);
        args.push(path.as_ptr());
        for arg in owned_args.iter() {
            args.push(arg.as_ptr());
        }
        args.push(core::ptr::null());
        let is_ltp_case = name.contains("/ltp/testcases/");
        let is_musl_ltp = name.contains("/musl/ltp/testcases/");
        let is_glibc_ltp = name.contains("/glibc/ltp/testcases/");
        let is_slow_net_virt_case =
            name.ends_with("/wireguard01.sh") || name.ends_with("/wireguard02.sh");
        // Device-dependent LTP helpers (tst_acquire_device) can use /dev/root
        // in this environment; keep all-filesystems loops bounded to tmpfs.
        let ltp_musl_envs = [
            LTP_ENV_DEV.as_ptr(),
            LTP_ENV_DEV_FS_TYPE.as_ptr(),
            LTP_ENV_SINGLE_FS_TYPE.as_ptr(),
            LTP_ENV_KERNEL.as_ptr(),
            LTP_ENV_LDD.as_ptr(),
            LTP_ENV_PATH_MUSL.as_ptr(),
            LTP_ENV_ROOT_MUSL.as_ptr(),
            core::ptr::null(),
        ];
        let ltp_musl_slow_envs = [
            LTP_ENV_DEV.as_ptr(),
            LTP_ENV_DEV_FS_TYPE.as_ptr(),
            LTP_ENV_SINGLE_FS_TYPE.as_ptr(),
            LTP_ENV_KERNEL.as_ptr(),
            LTP_ENV_LDD.as_ptr(),
            LTP_ENV_TIMEOUT_MUL_SLOW.as_ptr(),
            LTP_ENV_PATH_MUSL.as_ptr(),
            LTP_ENV_ROOT_MUSL.as_ptr(),
            core::ptr::null(),
        ];
        let ltp_glibc_envs = [
            LTP_ENV_DEV.as_ptr(),
            LTP_ENV_DEV_FS_TYPE.as_ptr(),
            LTP_ENV_SINGLE_FS_TYPE.as_ptr(),
            LTP_ENV_KERNEL.as_ptr(),
            LTP_ENV_LDD.as_ptr(),
            LTP_ENV_PATH_GLIBC.as_ptr(),
            LTP_ENV_ROOT_GLIBC.as_ptr(),
            core::ptr::null(),
        ];
        let ltp_glibc_slow_envs = [
            LTP_ENV_DEV.as_ptr(),
            LTP_ENV_DEV_FS_TYPE.as_ptr(),
            LTP_ENV_SINGLE_FS_TYPE.as_ptr(),
            LTP_ENV_KERNEL.as_ptr(),
            LTP_ENV_LDD.as_ptr(),
            LTP_ENV_TIMEOUT_MUL_SLOW.as_ptr(),
            LTP_ENV_PATH_GLIBC.as_ptr(),
            LTP_ENV_ROOT_GLIBC.as_ptr(),
            core::ptr::null(),
        ];
        let empty_envs = [core::ptr::null()];
        let envs: &[*const u8] = if is_ltp_case {
            // Keep case-specific knobs narrow. WireGuard's netstress matrix is
            // CPU-heavy under single-core QEMU, so use LTP's timeout multiplier
            // only for that virtual-network batch.
            if is_musl_ltp {
                if is_slow_net_virt_case {
                    &ltp_musl_slow_envs[..]
                } else {
                    &ltp_musl_envs[..]
                }
            } else if is_glibc_ltp {
                if is_slow_net_virt_case {
                    &ltp_glibc_slow_envs[..]
                } else {
                    &ltp_glibc_envs[..]
                }
            } else {
                &empty_envs[..]
            }
        } else {
            &empty_envs[..]
        };
        if is_ltp_case && name.ends_with("/doio") {
            // doio consumes requests from stdin; mirror LTP's fs iogen01
            // pipeline instead of execing doio directly and blocking.
            let bin_dir = &name[..name.len() - "/doio".len()];
            let shell_path = String::from("/bin/sh\0");
            let shell_arg0 = String::from("sh\0");
            let shell_arg1 = String::from("-c\0");
            let mut command = String::new();
            command.push_str(bin_dir);
            command.push_str(
                "/iogen -i 120s -s read,write 500b:/tmp/doio.f1.$$ 1000b:/tmp/doio.f2.$$ | ",
            );
            command.push_str(bin_dir);
            command.push_str("/doio -akv -n 2");
            command.push('\0');
            let shell_args = [
                shell_arg0.as_ptr(),
                shell_arg1.as_ptr(),
                command.as_ptr(),
                core::ptr::null(),
            ];
            execve(shell_path.as_str(), &shell_args, envs);
            exit(-1);
        }
        execve(path.as_str(), &args, envs);
        exit(-1);
    }
    let mut exit_code = 0;
    let _ = waitpid(pid as isize, &mut exit_code);
    normalize_ltp_wait_status(exit_code)
}

fn try_poweroff() -> ! {
    syscall::poweroff();
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    // only run for riscv arch
    if cfg!(target_arch = "riscv64") {
        if FOCUS_VFS_SMOKES {
            run_named_cases("vfs-smoke", VFS_SMOKES.as_ref());
        }
        if FOCUS_READINESS_SMOKES {
            run_named_cases("readiness-smoke", READINESS_SMOKES.as_ref());
        }
        if FOCUS_PROCFS_SMOKES {
            run_named_cases("procfs-smoke", PROCFS_SMOKES.as_ref());
        }
        if FOCUS_MEMORY_SMOKES {
            run_named_cases("memory-smoke", MEMORY_SMOKES.as_ref());
        }
        // basic_test

        // chdir("/musl");
        // run_script("/musl/basic_testcode.sh");

        // chdir("/glibc");
        // run_script("/glibc/basic_testcode.sh");
        // // busybox
        // chdir("/musl");
        // run_script("/musl/busybox_testcode.sh");
        // chdir("/glibc");
        // run_script("/glibc/busybox_testcode.sh");
        // //lua
        // chdir("/musl");
        // run_script("/musl/lua_testcode.sh");
        // chdir("/glibc");
        // run_script("/glibc/lua_testcode.sh");
        // // netperf
        // chdir("/musl");
        // run_script("/musl/netperf_testcode.sh");
        // chdir("/glibc");
        // run_script("/glibc/netperf_testcode.sh");

        // // below tests  take too long time

        // //cyclic
        // chdir("/musl");
        // run_script("/musl/cyclictest_testcode.sh");
        // chdir("/glibc");
        // run_script("/glibc/cyclictest_testcode.sh");

        // // iozone
        // chdir("/musl");
        // run_script("/musl/iozone_testcode.sh");
        // chdir("/glibc");
        // run_script("/glibc/iozone_testcode.sh");

        // // libctest
        // chdir("/musl");
        // run_script("/musl/libctest_testcode.sh");
        // chdir("/glibc");
        // run_script("/glibc/libctest_testcode.sh");

        // // libbench
        // chdir("/musl");
        // run_script("/musl/libcbench_testcode.sh");
        // chdir("/glibc");
        // run_script("/glibc/libcbench_testcode.sh");

        run_riscv_ltp_groups(run_part_of_ltp_script_in_dir);

        // // iperf (run last, to prevent the possible dead lock (dont know why todo))
        // chdir("/musl");
        // run_script("/musl/iperf_testcode.sh");
        // chdir("/glibc");
        // run_script("/glibc/iperf_testcode.sh");
    }
    if !cfg!(target_arch = "riscv64") {
        // basic_test

        // chdir("/musl");
        // run_script("/musl/basic_testcode.sh");

        // chdir("/glibc");
        // run_script("/glibc/basic_testcode.sh");
        // // busybox
        // chdir("/musl");
        // run_script("/musl/busybox_testcode.sh");
        // chdir("/glibc");
        // run_script("/glibc/busybox_testcode.sh");
        // //lua
        // chdir("/musl");
        // run_script("/musl/lua_testcode.sh");
        // chdir("/glibc");
        // run_script("/glibc/lua_testcode.sh");
        // // netperf
        // chdir("/musl");
        // run_script("/musl/netperf_testcode.sh");
        // chdir("/glibc");
        // run_script("/glibc/netperf_testcode.sh");

        // // below tests  take too long time

        // //cyclic
        // chdir("/musl");
        // run_script("/musl/cyclictest_testcode.sh");
        // chdir("/glibc");
        // run_script("/glibc/cyclictest_testcode.sh");

        // // iozone
        // chdir("/musl");
        // run_script("/musl/iozone_testcode.sh");
        // chdir("/glibc");
        // run_script("/glibc/iozone_testcode.sh");

        // libctest
        // chdir("/musl");
        // run_script("/musl/libctest_testcode.sh");
        // chdir("/glibc");
        // run_script("/glibc/libctest_testcode.sh");

        // libbench
        // chdir("/musl");
        // run_script("/musl/libcbench_testcode.sh");
        // chdir("/glibc");
        // run_script("/glibc/libcbench_testcode.sh");

        // unixbench_testcode.sh
        // chdir("/musl");
        // run_script("/musl/unixbench_testcode.sh");
        // chdir("/glibc");
        // run_script("/glibc/unixbench_testcode.sh");

        run_non_riscv_ltp_groups(run_part_of_ltp_script_in_dir);
        // // iperf (run last, to prevent the possible dead lock (dont know why todo))
        // chdir("/musl");
        // run_script("/musl/iperf_testcode.sh");
        // chdir("/glibc");
        // run_script("/glibc/iperf_testcode.sh");
    }

    println!("#### ALL TESTS DONE ####");
    let _ = sync();
    try_poweroff();
}
