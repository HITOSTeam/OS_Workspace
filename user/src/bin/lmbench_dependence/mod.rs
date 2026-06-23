use super::{monotonic_time_ms, run_script};
use user::syscall::chdir;

pub const LMBENCH_MUSL_CASES: &[&str] = ALL_LMBENCH_CASES;
pub const LMBENCH_GLIBC_CASES: &[&str] = ALL_LMBENCH_CASES;

const ALL_LMBENCH_CASES: &[&str] = &[
    "lat_syscall-null",
    "lat_syscall-read",
    "lat_syscall-write",
    "lat_syscall-stat",
    "lat_syscall-fstat",
    "lat_syscall-open",
    "lat_select-file",
    "lat_sig-install",
    "lat_sig-catch",
    "lat_sig-prot",
    "lat_pipe",
    "lat_proc-fork",
    "lat_proc-exec",
    "lat_proc-shell",
    "lmdd-write",
    "lat_pagefault",
    "lat_mmap",
    "lat_fs",
    "bw_pipe",
    "bw_file_rd-io_only",
    "bw_file_rd-open2close",
    "bw_mmap_rd-mmap_only",
    "bw_mmap_rd-open2close",
    "lat_ctx",
];

fn run_lmbench_case(selected: &[&str], binary: &str, name: &str, args: &[&str]) -> i32 {
    if !selected.contains(&name) {
        return 0;
    }
    let start_ms = monotonic_time_ms();
    println!("LMBENCH CASE START {} TIME_MS {}", name, start_ms);
    let ret = run_script(binary, args);
    let end_ms = monotonic_time_ms();
    println!(
        "LMBENCH CASE END {} TIME_MS {} DURATION_MS {}",
        name,
        end_ms,
        end_ms.saturating_sub(start_ms)
    );
    ret
}

fn run_lmbench_suite(
    group: &str,
    dir: &str,
    binary: &str,
    busybox: &str,
    hello_wrapper: &str,
    selected: &[&str],
) {
    let _ = chdir(dir);
    println!("#### OS COMP TEST GROUP START {} ####", group);
    println!("latency measurements");

    let run = |name: &str, args: &[&str]| run_lmbench_case(selected, binary, name, args);
    let _ = run("lat_syscall-null", &["lat_syscall", "-P", "1", "null"]);
    let _ = run("lat_syscall-read", &["lat_syscall", "-P", "1", "read"]);
    let _ = run("lat_syscall-write", &["lat_syscall", "-P", "1", "write"]);

    let _ = run_script(busybox, &["mkdir", "-p", "/var/tmp"]);
    let _ = run_script(busybox, &["touch", "/var/tmp/lmbench"]);
    let _ = run("lat_syscall-stat", &[
        "lat_syscall",
        "-P",
        "1",
        "stat",
        "/var/tmp/lmbench",
    ]);
    let _ = run("lat_syscall-fstat", &[
        "lat_syscall",
        "-P",
        "1",
        "fstat",
        "/var/tmp/lmbench",
    ]);
    let _ = run("lat_syscall-open", &[
        "lat_syscall",
        "-P",
        "1",
        "open",
        "/var/tmp/lmbench",
    ]);
    let _ = run("lat_select-file", &[
        "lat_select",
        "-n",
        "100",
        "-P",
        "1",
        "file",
    ]);
    let _ = run("lat_sig-install", &["lat_sig", "-P", "1", "install"]);
    let _ = run("lat_sig-catch", &["lat_sig", "-P", "1", "catch"]);
    let _ = run("lat_sig-prot", &["lat_sig", "-P", "1", "prot", "lat_sig"]);
    let _ = run("lat_pipe", &["lat_pipe", "-P", "1"]);
    let _ = run("lat_proc-fork", &["lat_proc", "-P", "1", "fork"]);
    let _ = run("lat_proc-exec", &["lat_proc", "-P", "1", "exec"]);

    //镜像里面的 hello 有问题
    if selected.contains(&"lat_proc-shell") {
        let prepare_ret = run_script(busybox, &["sh", "-c", hello_wrapper]);
        if prepare_ret == 0 {
            let _ = run("lat_proc-shell", &["lat_proc", "-P", "1", "shell"]);
        } else {
            println!(
                "LMBENCH CASE SKIP lat_proc-shell: failed to prepare /tmp/hello ({})",
                prepare_ret
            );
        }
    }
    let _ = run("lmdd-write", &[
        "lmdd",
        "label=File /var/tmp/XXX write bandwidth:",
        "of=/var/tmp/XXX",
        "move=1m",
        "fsync=1",
        "print=3",
    ]);
    let _ = run("lat_pagefault", &[
        "lat_pagefault",
        "-P",
        "1",
        "/var/tmp/XXX",
    ]);
    let _ = run("lat_mmap", &["lat_mmap", "-P", "1", "512k", "/var/tmp/XXX"]);

    println!("file system latency");
    let _ = run("lat_fs", &["lat_fs", "/var/tmp"]);

    println!("Bandwidth measurements");
    let _ = run("bw_pipe", &["bw_pipe", "-P", "1"]);
    let _ = run("bw_file_rd-io_only", &[
        "bw_file_rd",
        "-P",
        "1",
        "512k",
        "io_only",
        "/var/tmp/XXX",
    ]);
    let _ = run("bw_file_rd-open2close", &[
        "bw_file_rd",
        "-P",
        "1",
        "512k",
        "open2close",
        "/var/tmp/XXX",
    ]);
    let _ = run("bw_mmap_rd-mmap_only", &[
        "bw_mmap_rd",
        "-P",
        "1",
        "512k",
        "mmap_only",
        "/var/tmp/XXX",
    ]);
    let _ = run("bw_mmap_rd-open2close", &[
        "bw_mmap_rd",
        "-P",
        "1",
        "512k",
        "open2close",
        "/var/tmp/XXX",
    ]);

    println!("context switch overhead");
    let _ = run("lat_ctx", &[
        "lat_ctx", "-P", "1", "-s", "32", "2", "4", "8", "16", "24", "32", "64", "96",
    ]);
    println!("#### OS COMP TEST GROUP END {} ####", group);
}

#[allow(unused)]
pub fn lmbench_simple_musl() {
    run_lmbench_suite(
        "lmbench-musl",
        "/musl",
        "/musl/lmbench_all",
        "/musl/busybox",
        "printf '#!/bin/sh\\n/musl/lmbench_all hello \"$@\"\\n' > /tmp/hello && /musl/busybox chmod +x /tmp/hello",
        LMBENCH_MUSL_CASES,
    );
}

#[allow(unused)]
pub fn lmbench_simple_glibc() {
    run_lmbench_suite(
        "lmbench-glibc",
        "/glibc",
        "/glibc/lmbench_all",
        "/glibc/busybox",
        "printf '#!/bin/sh\\n/glibc/lmbench_all hello \"$@\"\\n' > /tmp/hello && /glibc/busybox chmod +x /tmp/hello",
        LMBENCH_GLIBC_CASES,
    );
}
