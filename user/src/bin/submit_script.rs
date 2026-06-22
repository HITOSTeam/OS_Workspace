#![no_std]
#![no_main]

extern crate alloc;
#[macro_use]
extern crate user;

use alloc::{string::String, vec::Vec};
mod ltp_dependence;
use ltp_dependence::*;
use user::syscall::{
    self, chdir, close, execve, exit, fork, open, read, sleep, sync, waitpid, RDONLY,
};

const LTP_ENV_DEV: &[u8] = b"LTP_DEV=/dev/root\0";
const LTP_ENV_DEV_FS_TYPE: &[u8] = b"LTP_DEV_FS_TYPE=tmpfs\0";
const LTP_ENV_SINGLE_FS_TYPE: &[u8] = b"LTP_SINGLE_FS_TYPE=tmpfs\0";
const LTP_ENV_KERNEL: &[u8] = b"KERNEL=/config.gz\0";
const LTP_ENV_PATH: &[u8] =
    b"PATH=/extra/bin:/user:/:/bin:/usr/bin:/musl:/glibc:/musl/ltp/testcases/bin:/glibc/ltp/testcases/bin\0";
const LTP_ENV_ROOT_MUSL: &[u8] = b"LTPROOT=/musl/ltp\0";
const LTP_ENV_ROOT_GLIBC: &[u8] = b"LTPROOT=/glibc/ltp\0";
const LTP_ENV_CGROUPS_ROOT_MUSL: &[u8] = b"CGROUPS_TESTROOT=/musl/ltp/testcases/bin\0";
const LTP_ENV_CGROUPS_ROOT_GLIBC: &[u8] = b"CGROUPS_TESTROOT=/glibc/ltp/testcases/bin\0";
const LTP_ENV_COLORIZE_OUTPUT: &[u8] = b"LTP_COLORIZE_OUTPUT=1\0";
const LTP_ENV_TIMEOUT_MUL_SLOW: &[u8] = b"LTP_TIMEOUT_MUL=4\0";
const GLIBC_ENV_LANG: &[u8] = b"LANG=C.UTF-8\0";
const GLIBC_ENV_LC_ALL: &[u8] = b"LC_ALL=C.UTF-8\0";
const GLIBC_ENV_LOCPATH: &[u8] = b"LOCPATH=/usr/lib/locale\0";
const FOCUS_READINESS_SMOKES: bool = false;

const READINESS_SMOKES: [&str; 14] = [
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
    "/user/dup3_lock_cleanup_smoke.bin",
];
// const FOCUS_PROCFS_SMOKES: bool = false;
// const PROCFS_SMOKES: [&str; 2] = [
//     "/user/proc_magic_links_smoke.bin",
//     "/user/mount_namespace_smoke.bin",
// ];


///输入测试用例子写的数组，挨个测试
fn run_part_of_ltp_script_in_dir(dir: &str, script_names: &[&str]) {
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
        let path = resolve_ltp_case_path(dir, script);
        println!("RUN LTP CASE {}", script);
        let ret = run_script(path.as_str(), &extra_args);
        println!("FAIL LTP CASE {} : {}",script,ret);
    }
}

///入口函数，打印#### 对应的信息
fn run_ltp_lane(
    group: &str,
    dir: &str,
    script_names: &[&str],
) {
    println!("#### OS COMP TEST GROUP START {} ####", group);
    run_part_of_ltp_script_in_dir(dir, script_names);
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

fn resolve_ltp_case_path(dir: &str, script: &str) -> String {
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

fn settle_after_cyclictest() {
    sleep(60000);
}

///如果是LTP测试用例，添加对应的的环境信息使得他输出颜色
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
        // doio is a low-level engine that blocks on stdin when run directly.
        // Feed it with iogen through a simple shell pipeline.
        let doio_pipeline_case = name.ends_with("/ltp/testcases/bin/doio") && extra_args.is_empty();
        let mut path = String::from(name);
        let mut owned_args: Vec<String> = Vec::new();
        if doio_pipeline_case {
            let mut bin_dir = String::from(name);
            bin_dir.truncate(bin_dir.len() - "doio".len());

            let mut iogen = bin_dir.clone();
            iogen.push_str("iogen");
            let mut doio = bin_dir;
            doio.push_str("doio");

            let mut pipeline_cmd = String::new();
            pipeline_cmd.push_str(iogen.as_str());
            pipeline_cmd.push_str(" -i 30s -s read,write 500b:/tmp/doio.f1 1000b:/tmp/doio.f2 | ");
            pipeline_cmd.push_str(doio.as_str());
            pipeline_cmd.push_str(" -akv -n 2");

            path = String::from("/bin/sh");
            for arg in ["-c", pipeline_cmd.as_str()] {
                let mut s = String::from(arg);
                s.push('\0');
                owned_args.push(s);
            }
        } else {
            for arg in extra_args.iter().copied() {
                let mut s = String::from(arg);
                s.push('\0');
                owned_args.push(s);
            }
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
        let is_glibc_case = name.starts_with("/glibc/");
        let is_ltp_mmap1 = name.ends_with("/mmap1");
        let is_freezer_controller_script = name.ends_with("/write_freezing.sh")
            || name.ends_with("/freeze_write_freezing.sh")
            || name.ends_with("/freeze_thaw.sh")
            || name.ends_with("/freeze_self_thaw.sh")
            || name.ends_with("/freeze_sleep_thaw.sh")
            || name.ends_with("/freeze_move_thaw.sh")
            || name.ends_with("/freeze_cancel.sh")
            || name.ends_with("/freeze_kill_thaw.sh")
            || name.ends_with("/fork_freeze.sh")
            || name.ends_with("/stop_freeze_thaw_cont.sh")
            || name.ends_with("/stop_freeze_sleep_thaw_cont.sh")
            || name.ends_with("/vfork_freeze.sh")
            || name.ends_with("/run_freezer.sh");
        let is_msgstress01 = name.contains("/ltp/testcases/bin/msgstress01");
        let is_slow_futex_cmp_requeue = cfg!(target_arch = "loongarch64")&& name.ends_with("/futex_cmp_requeue01");
        // Device-dependent LTP helpers (tst_acquire_device) can use /dev/root
        // in this environment; keep all-filesystems loops bounded to tmpfs.
        let ltp_musl_envs = [
            LTP_ENV_DEV.as_ptr(),
            LTP_ENV_DEV_FS_TYPE.as_ptr(),
            LTP_ENV_SINGLE_FS_TYPE.as_ptr(),
            LTP_ENV_KERNEL.as_ptr(),
            LTP_ENV_COLORIZE_OUTPUT.as_ptr(),
            LTP_ENV_PATH.as_ptr(),
            LTP_ENV_ROOT_MUSL.as_ptr(),
            core::ptr::null(),
        ];
        let ltp_glibc_envs = [
            LTP_ENV_DEV.as_ptr(),
            LTP_ENV_DEV_FS_TYPE.as_ptr(),
            LTP_ENV_SINGLE_FS_TYPE.as_ptr(),
            LTP_ENV_KERNEL.as_ptr(),
            LTP_ENV_COLORIZE_OUTPUT.as_ptr(),
            LTP_ENV_PATH.as_ptr(),
            LTP_ENV_ROOT_GLIBC.as_ptr(),
            GLIBC_ENV_LANG.as_ptr(),
            GLIBC_ENV_LC_ALL.as_ptr(),
            GLIBC_ENV_LOCPATH.as_ptr(),
            core::ptr::null(),
        ];
        let ltp_musl_envs_cgroup_freezer = [
            LTP_ENV_DEV.as_ptr(),
            LTP_ENV_DEV_FS_TYPE.as_ptr(),
            LTP_ENV_SINGLE_FS_TYPE.as_ptr(),
            LTP_ENV_KERNEL.as_ptr(),
            LTP_ENV_COLORIZE_OUTPUT.as_ptr(),
            LTP_ENV_PATH.as_ptr(),
            LTP_ENV_ROOT_MUSL.as_ptr(),
            LTP_ENV_CGROUPS_ROOT_MUSL.as_ptr(),
            core::ptr::null(),
        ];
        let ltp_glibc_envs_cgroup_freezer = [
            LTP_ENV_DEV.as_ptr(),
            LTP_ENV_DEV_FS_TYPE.as_ptr(),
            LTP_ENV_SINGLE_FS_TYPE.as_ptr(),
            LTP_ENV_KERNEL.as_ptr(),
            LTP_ENV_COLORIZE_OUTPUT.as_ptr(),
            LTP_ENV_PATH.as_ptr(),
            LTP_ENV_ROOT_GLIBC.as_ptr(),
            LTP_ENV_CGROUPS_ROOT_GLIBC.as_ptr(),
            GLIBC_ENV_LANG.as_ptr(),
            GLIBC_ENV_LC_ALL.as_ptr(),
            GLIBC_ENV_LOCPATH.as_ptr(),
            core::ptr::null(),
        ];
        let ltp_musl_envs_slow_timeout = [
            LTP_ENV_DEV.as_ptr(),
            LTP_ENV_DEV_FS_TYPE.as_ptr(),
            LTP_ENV_SINGLE_FS_TYPE.as_ptr(),
            LTP_ENV_KERNEL.as_ptr(),
            LTP_ENV_COLORIZE_OUTPUT.as_ptr(),
            LTP_ENV_PATH.as_ptr(),
            LTP_ENV_ROOT_MUSL.as_ptr(),
            LTP_ENV_TIMEOUT_MUL_SLOW.as_ptr(),
            core::ptr::null(),
        ];
        let ltp_glibc_envs_slow_timeout = [
            LTP_ENV_DEV.as_ptr(),
            LTP_ENV_DEV_FS_TYPE.as_ptr(),
            LTP_ENV_SINGLE_FS_TYPE.as_ptr(),
            LTP_ENV_KERNEL.as_ptr(),
            LTP_ENV_COLORIZE_OUTPUT.as_ptr(),
            LTP_ENV_PATH.as_ptr(),
            LTP_ENV_ROOT_GLIBC.as_ptr(),
            LTP_ENV_TIMEOUT_MUL_SLOW.as_ptr(),
            GLIBC_ENV_LANG.as_ptr(),
            GLIBC_ENV_LC_ALL.as_ptr(),
            GLIBC_ENV_LOCPATH.as_ptr(),
            core::ptr::null(),
        ];
        let glibc_envs = [
            LTP_ENV_PATH.as_ptr(),
            GLIBC_ENV_LANG.as_ptr(),
            GLIBC_ENV_LC_ALL.as_ptr(),
            GLIBC_ENV_LOCPATH.as_ptr(),
            core::ptr::null(),
        ];
        let empty_envs = [core::ptr::null()];
        let envs: &[*const u8] = if is_ltp_case {
            if is_ltp_mmap1 || is_msgstress01 || is_slow_futex_cmp_requeue{
                if is_musl_ltp {
                    &ltp_musl_envs_slow_timeout[..]
                } else if is_glibc_ltp {
                    &ltp_glibc_envs_slow_timeout[..]
                } else {
                    &empty_envs[..]
                }
            } else if is_freezer_controller_script && is_musl_ltp {
                &ltp_musl_envs_cgroup_freezer[..]
            } else if is_freezer_controller_script && is_glibc_ltp {
                &ltp_glibc_envs_cgroup_freezer[..]
            } else if is_musl_ltp {
                &ltp_musl_envs[..]
            } else if is_glibc_ltp {
                &ltp_glibc_envs[..]
            } else {
                &empty_envs[..]
            }
        } else if is_glibc_case {
            &glibc_envs[..]
        } else {
            &empty_envs[..]
        };
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

#[allow(unused)]
fn test_rv() {

        //先跑cyclictest
    chdir("/musl");
    run_script("/musl/cyclictest_testcode.sh", &[]);
    chdir("/glibc");
    run_script("/glibc/cyclictest_testcode.sh", &[]);


    // basic_test
    chdir("/musl");
    run_script("/musl/basic_testcode.sh", &[]);

    chdir("/glibc");
    run_script("/glibc/basic_testcode.sh", &[]);
    // busybox
    chdir("/musl");
    run_script("/musl/busybox_testcode.sh", &[]);
    chdir("/glibc");
    run_script("/glibc/busybox_testcode.sh", &[]);
    //lua
    chdir("/musl");
    run_script("/musl/lua_testcode.sh", &[]);
    chdir("/glibc");
    run_script("/glibc/lua_testcode.sh", &[]);
    // netperf
    chdir("/musl");
    run_script("/musl/netperf_testcode.sh", &[]);
    chdir("/glibc");
    run_script("/glibc/netperf_testcode.sh", &[]);

    // libctest 不需要跑glibc的libc test,跑了也没有分数
    chdir("/musl");
    run_script("/musl/libctest_testcode.sh", &[]);

    // // iozone
    chdir("/musl");
    run_script("/musl/iozone_testcode.sh", &[]);
    chdir("/glibc");
    run_script("/glibc/iozone_testcode.sh", &[]);

    // // libcbench
    chdir("/musl");
    run_script("/musl/libcbench_testcode.sh", &[]);
    chdir("/glibc");
    run_script("/glibc/libcbench_testcode.sh", &[]);

    chdir("/musl");
    run_script("/musl/iperf_testcode.sh", &[]);
    chdir("/glibc");
    run_script("/glibc/iperf_testcode.sh", &[]);

    run_ltp_lane("ltp-musl", "/musl", RISCV_LTP_CASES);
    run_ltp_lane("ltp-glibc", "/glibc", RISCV_LTP_CASES);

    // //lmbench
    chdir("/musl");
    run_script("/musl/lmbench_testcode.sh", &[]);
    chdir("/glibc");
    run_script("/glibc/lmbench_testcode.sh", &[]);


    // // 等待网络功能再完善些
    // chdir("/musl");
    // run_script("/musl/iperf_testcode.sh", &[]);
    // chdir("/glibc");
    // run_script("/glibc/iperf_testcode.sh", &[]);
}

#[allow(unused)]
fn test_la() {
    // basic_test

    chdir("/glibc");
    run_script("/glibc/cyclictest_testcode.sh", &[]);

    chdir("/musl");
    run_script("/musl/basic_testcode.sh", &[]);

    chdir("/glibc");
    run_script("/glibc/basic_testcode.sh", &[]);
    // busybox
    chdir("/musl");
    run_script("/musl/busybox_testcode.sh", &[]);
    chdir("/glibc");
    run_script("/glibc/busybox_testcode.sh", &[]);
    //lua
    chdir("/musl");
    run_script("/musl/lua_testcode.sh", &[]);
    chdir("/glibc");
    run_script("/glibc/lua_testcode.sh", &[]);
    // netperf
    chdir("/musl");
    run_script("/musl/netperf_testcode.sh", &[]);
    chdir("/glibc");
    run_script("/glibc/netperf_testcode.sh", &[]);

    // // libctest
    // // //不需要跑glibc的libc test,跑了也没有分数
    chdir("/musl");
    run_script("/musl/libctest_testcode.sh", &[]);

    // iozone
    chdir("/musl");
    run_script("/musl/iozone_testcode.sh", &[]);
    chdir("/glibc");
    run_script("/glibc/iozone_testcode.sh", &[]);

    // libbench
    chdir("/musl");
    run_script("/musl/libcbench_testcode.sh", &[]);
    chdir("/glibc");
    run_script("/glibc/libcbench_testcode.sh", &[]);

    run_ltp_lane("ltp-musl", "/musl", LOONGARCH_LTP_CASES);
    run_ltp_lane("ltp-glibc", "/glibc", LOONGARCH_LTP_CASES);

    //lmbench很消耗时间,先把上面的测了
    chdir("/musl");
    run_script("/musl/lmbench_testcode.sh", &[]);

    chdir("/glibc");
    run_script("/glibc/lmbench_testcode.sh", &[]);


    // // iperf (run last, to prevent the possible dead lock (dont know why todo))
    // chdir("/musl");
    // run_script("/musl/iperf_testcode.sh", &[]);
    // chdir("/glibc");
    // run_script("/glibc/iperf_testcode.sh", &[]);
}

// fn lmbench_simple_musl() {
//     chdir("/musl");
//     println!("#### OS COMP TEST GROUP START lmbench-musl ####");
//     println!("latency measurements");

//     let _ = run_script("/musl/busybox", &["mkdir", "-p", "/var/tmp"]);
//     let _ = run_script("/musl/busybox", &["touch", "/var/tmp/lmbench"]);

//     let _ = run_script("/musl/lmbench_all", &["lat_syscall", "-P", "1", "null"]);
//     let _ = run_script("/musl/lmbench_all", &["lat_syscall", "-P", "1", "read"]);
//     let _ = run_script("/musl/lmbench_all", &["lat_syscall", "-P", "1", "write"]);

//     let _ = run_script(
//         "/musl/lmbench_all",
//         &["lat_syscall", "-P", "1", "stat", "/var/tmp/lmbench"],
//     );
//     let _ = run_script(
//         "/musl/lmbench_all",
//         &["lat_syscall", "-P", "1", "fstat", "/var/tmp/lmbench"],
//     );
//     let _ = run_script(
//         "/musl/lmbench_all",
//         &["lat_syscall", "-P", "1", "open", "/var/tmp/lmbench"],
//     );

//     let _ = run_script(
//         "/musl/lmbench_all",
//         &["lat_select", "-n", "100", "-P", "1", "file"],
//     );

//     let _ = run_script("/musl/lmbench_all", &["lat_sig", "-P", "1", "install"]);
//     let _ = run_script("/musl/lmbench_all", &["lat_sig", "-P", "1", "catch"]);

//     let _ = run_script("/musl/lmbench_all", &["lat_pipe", "-P", "1"]);

//     let _ = run_script("/musl/lmbench_all", &["lat_fs", "/var/tmp"]);
//     println!("context switch overhead");
//     let _ = run_script("/musl/lmbench_all", &["lat_ctx","-P", "1", "-s" ,"32", "2", "4" ,"8" ,"16", "24", "32", "64", "96"]);

//     println!("#### OS COMP TEST GROUP END lmbench-musl ####");
// }


//测试函数,单独开始一个测试用例子
fn test_ltp_bin_single() {
    println!("#### OS COMP TEST GROUP START ltp-musl-single ####");
    run_part_of_ltp_script_in_dir("/musl", &["epoll_wait01"]);
    println!("#### OS COMP TEST GROUP END ltp-musl-single ####");
}



#[unsafe(no_mangle)]
pub fn main() -> i32 {
    // only run for riscv arch
    if cfg!(target_arch = "riscv64") {
        // if FOCUS_READINESS_SMOKES {
        //     run_named_cases("readiness-smoke", READINESS_SMOKES.as_ref());
        // }
        // if FOCUS_PROCFS_SMOKES {
        //     run_named_cases("procfs-smoke", PROCFS_SMOKES.as_ref());
        // }
        // chdir("/musl");
        // run_script("/musl/cyclictest_testcode.sh", &[]);
        // settle_after_cyclictest();
        // chdir("/glibc");
        // run_script("/glibc/cyclictest_testcode.sh", &[]);
        // settle_after_cyclictest();
        // lmbench_simple_musl();
        // chdir("/glibc");
        // test_ltp_bin_single();
        // run_script("/glibc/lmbench_testcode.sh", &[]);
        run_ltp_lane("ltp-musl", "/musl", RISCV_LTP_CASES);
        run_ltp_lane("ltp-glibc", "/glibc", RISCV_LTP_CASES);
        // test_rv();
    }
    if !cfg!(target_arch = "riscv64") {
        //musl的loongarch有问题

        // chdir("/glibc");
        // run_script("/glibc/cyclictest_testcode.sh", &[]);
        // settle_after_cyclictest();
        // chdir("/musl");
        // lmbench_simple_musl();
        // chdir("/glibc");
        // run_script("/glibc/lmbench_testcode.sh",&[]);
        test_ltp_bin_single();

        // run_ltp_lane("ltp-musl", "/musl", LOONGARCH_LTP_CASES);
        // run_ltp_lane("ltp-glibc", "/glibc", LOONGARCH_LTP_CASES);
        // test_la();
    }

    println!("#### ALL TESTS DONE ####");
    let _ = sync();
    try_poweroff();
}
