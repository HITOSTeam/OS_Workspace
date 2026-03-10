#![no_std]
#![no_main]
#![allow(clippy::println_empty_string)]

extern crate alloc;

#[macro_use]
extern crate user;

mod shell;

const LF: u8 = 0x0au8;
const CR: u8 = 0x0du8;
const DL: u8 = 0x7fu8;
const BS: u8 = 0x08u8;

const THEME_COLOR: &str = "\u{1B}[38;5;14m";
const RESET_COLOR: &str = "\u{1B}[0m";

use alloc::{string::String, vec::Vec};
use shell::command::{resolve_exec_candidates, split_sequences};
use shell::environment::Environment;
use shell::script::run_script;
use user::syscall::{
    RDONLY, chdir, close, dup3, execve, exit, fork, getchar, getcwd, kill, open, pipe, sleep,
    waitpid,
};

fn decode_wait_status(status: i32) -> i32 {
    let sig = status & 0x7f;
    if sig != 0 {
        128 + sig
    } else {
        (status >> 8) & 0xff
    }
}

fn print_prompt() {
    let cwd = getcwd();
    print!("{}CongCore:{}$ {}", THEME_COLOR, cwd, RESET_COLOR);
}

fn exec_command(argv: &[String], env: &Environment) -> ! {
    if argv.is_empty() {
        exit(-1);
    }

    let mut arg_strings: Vec<String> = Vec::new();
    let mut arg_ptrs: Vec<*const u8> = Vec::new();
    for a in argv.iter() {
        let mut s = a.clone();
        s.push('\0');
        arg_ptrs.push(s.as_ptr());
        arg_strings.push(s);
    }
    arg_ptrs.push(core::ptr::null());

    let mut env_strings: Vec<String> = Vec::new();
    let mut env_ptrs: Vec<*const u8> = Vec::new();
    for (k, v) in env.list_all() {
        let mut s = k;
        s.push('=');
        s.push_str(&v);
        s.push('\0');
        env_ptrs.push(s.as_ptr());
        env_strings.push(s);
    }
    env_ptrs.push(core::ptr::null());

    for cand in resolve_exec_candidates(env, &argv[0]) {
        let mut path = cand.clone();
        path.push('\0');
        // In our kernel ABI, syscalls return:
        // - `0` on success (exec will not return to this code path), or
        // - a negative errno value on failure.
        let rc = execve(&path, &arg_ptrs, &env_ptrs);
        if rc == 0 {
            unreachable!();
        }
        // ENOEXEC fallback: run scripts without shebang via busybox sh.
        if rc == -8 {
            let mut sh_argv: Vec<String> = Vec::new();
            let busybox = find_busybox(env).unwrap_or_else(|| String::from("busybox"));
            sh_argv.push(busybox);
            sh_argv.push(String::from("sh"));
            sh_argv.push(cand);
            exec_command(&sh_argv, env);
        }
    }

    println!("{}: command not found", argv[0]);
    exit(-1);
}

fn path_exists(path: &str) -> bool {
    let fd = open(path, RDONLY);
    if fd >= 0 {
        let _ = close(fd as usize);
        true
    } else {
        false
    }
}

fn find_busybox(env: &Environment) -> Option<String> {
    let candidates = [
        "/musl/busybox",
        "/glibc/busybox",
        "/bin/busybox",
        "/busybox",
    ];
    for cand in candidates {
        if path_exists(cand) {
            return Some(String::from(cand));
        }
    }
    for dir in env.path_dirs() {
        let mut path = String::from(dir);
        if !path.ends_with('/') {
            path.push('/');
        }
        path.push_str("busybox");
        if path_exists(&path) {
            return Some(path);
        }
    }
    None
}

fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn eval_stmt(stmt: &str, env: &mut Environment) -> bool {
    let expanded = env.expand_variables(stmt);
    for seq in split_sequences(&expanded) {
        let pipeline = shell::command::parse_pipeline(&seq);
        if pipeline.commands.is_empty() {
            continue;
        }

        if pipeline.commands.len() == 1 {
            let argv = &pipeline.commands[0].argv;
            if argv.is_empty() {
                continue;
            }

            if argv.len() == 1 {
                if let Some((k, v)) = argv[0].split_once('=') {
                    if is_ident(k) {
                        env.set(String::from(k), String::from(v));
                        continue;
                    }
                }
            }

            match argv[0].as_str() {
                "exit" => return true,
                "eval" => {
                    let joined = argv.iter().skip(1).cloned().collect::<Vec<_>>().join(" ");
                    if !joined.is_empty() {
                        if eval_stmt(&joined, env) {
                            return true;
                        }
                    }
                    env.set(String::from("?"), String::from("0"));
                    continue;
                }
                "pwd" => {
                    println!("{}", getcwd());
                    env.set(String::from("?"), String::from("0"));
                    continue;
                }
                "cd" => {
                    let target = argv.get(1).map(|s| s.as_str()).unwrap_or("/");
                    if chdir(target) != 0 {
                        println!("cd: failed to change directory to '{}'", target);
                        env.set(String::from("?"), String::from("1"));
                    } else {
                        env.set(String::from("?"), String::from("0"));
                    }
                    continue;
                }
                "export" => {
                    if let Some(kv) = argv.get(1) {
                        if let Some((k, v)) = kv.split_once('=') {
                            env.set(String::from(k), String::from(v));
                        } else {
                            println!("export: expected KEY=VALUE");
                        }
                    } else {
                        println!("export: expected KEY=VALUE");
                    }
                    continue;
                }
                "unset" => {
                    if let Some(k) = argv.get(1) {
                        env.unset(k);
                    } else {
                        println!("unset: expected KEY");
                    }
                    continue;
                }
                "env" => {
                    for (k, v) in env.list_all() {
                        println!("{}={}", k, v);
                    }
                    env.set(String::from("?"), String::from("0"));
                    continue;
                }
                "echo" => {
                    let mut first = true;
                    for a in argv.iter().skip(1) {
                        if !first {
                            print!(" ");
                        }
                        first = false;
                        print!("{}", a);
                    }
                    println!("");
                    env.set(String::from("?"), String::from("0"));
                    continue;
                }
                "sleep" => {
                    let secs = argv
                        .get(1)
                        .and_then(|s| s.parse::<usize>().ok())
                        .unwrap_or(0);
                    sleep(secs * 1000);
                    env.set(String::from("?"), String::from("0"));
                    continue;
                }
                "kill" => {
                    let mut signum: i32 = 15;
                    let mut argi = 1usize;
                    if let Some(s) = argv.get(1) {
                        if let Some(rest) = s.strip_prefix('-') {
                            if let Ok(v) = rest.parse::<i32>() {
                                signum = v;
                                argi = 2;
                            }
                        }
                    }
                    let pid = argv
                        .get(argi)
                        .and_then(|s| s.parse::<usize>().ok())
                        .unwrap_or(0);
                    let ret = kill(pid, signum);
                    env.set(
                        String::from("?"),
                        String::from(if ret == 0 { "0" } else { "1" }),
                    );
                    continue;
                }

                "sh" | "ash" => {
                    // Prefer busybox for a full-feature POSIX shell.
                    // This is required by OSComp scripts using `while`, pipes, `eval`, and `-c`.
                    let mut bb_argv: Vec<String> = Vec::new();
                    let busybox = find_busybox(env).unwrap_or_else(|| String::from("busybox"));
                    bb_argv.push(busybox);
                    bb_argv.push(argv[0].clone());
                    for a in argv.iter().skip(1) {
                        bb_argv.push(a.clone());
                    }
                    let pid = fork();
                    if pid == 0 {
                        exec_command(&bb_argv, env);
                    } else {
                        let mut status: i32 = 0;
                        let _ = waitpid(pid as isize, &mut status);
                        env.set(
                            String::from("?"),
                            alloc::format!("{}", decode_wait_status(status)),
                        );
                    }
                    continue;
                }
                "source" | "." => {
                    let Some(path) = argv.get(1) else {
                        println!("sh: expected script path");
                        env.set(String::from("?"), String::from("1"));
                        continue;
                    };
                    if run_script(path, env, eval_stmt) != 0 {
                        println!("sh: failed to run '{}'", path);
                        env.set(String::from("?"), String::from("1"));
                    } else {
                        env.set(String::from("?"), String::from("0"));
                    }
                    continue;
                }
                _ => {}
            }

            if argv[0].ends_with(".sh") {
                // Prefer busybox `sh` for OSComp scripts (while/read/eval/redirections).
                let script = argv[0].clone();
                let mut sh_argv: Vec<String> = Vec::new();
                let busybox = find_busybox(env).unwrap_or_else(|| String::from("busybox"));
                sh_argv.push(busybox);
                sh_argv.push(String::from("sh"));
                sh_argv.push(script);
                for a in argv.iter().skip(1) {
                    sh_argv.push(a.clone());
                }
                let pid = fork();
                if pid == 0 {
                    exec_command(&sh_argv, env);
                } else {
                    let mut status: i32 = 0;
                    let _ = waitpid(pid as isize, &mut status);
                    env.set(
                        String::from("?"),
                        alloc::format!("{}", decode_wait_status(status)),
                    );
                }
                continue;
            }
        }

        if pipeline.commands.len() == 1 {
            let mut argv = pipeline.commands[0].argv.clone();
            let background = argv.last().map(|s| s.as_str()) == Some("&");
            if background {
                let _ = argv.pop();
            }
            let pid = fork();
            if pid == 0 {
                exec_command(&argv, env);
            } else {
                if background {
                    env.set(String::from("!"), alloc::format!("{}", pid));
                    env.set(String::from("?"), String::from("0"));
                } else {
                    let mut exit_code: i32 = 0;
                    let _ = waitpid(pid as isize, &mut exit_code);
                    env.set(
                        String::from("?"),
                        alloc::format!("{}", decode_wait_status(exit_code)),
                    );
                }
            }
        } else {
            let mut prev_read: Option<usize> = None;
            let mut pids: Vec<isize> = Vec::new();
            let mut last_pid: isize = -1;
            for (i, cmd) in pipeline.commands.iter().enumerate() {
                let last = i + 1 == pipeline.commands.len();
                let mut fds = [0usize; 2];
                if !last {
                    if pipe(&mut fds) != 0 {
                        println!("pipe: failed");
                        break;
                    }
                }

                let argv = cmd.argv.clone();
                let pid = fork();
                if pid == 0 {
                    if let Some(fd) = prev_read {
                        let _ = dup3(fd, 0, 0);
                        let _ = close(fd);
                    }
                    if !last {
                        let _ = close(fds[0]);
                        let _ = dup3(fds[1], 1, 0);
                        let _ = close(fds[1]);
                    }
                    exec_command(&argv, env);
                }
                pids.push(pid as isize);
                if last {
                    last_pid = pid as isize;
                }

                if let Some(fd) = prev_read {
                    let _ = close(fd);
                }
                if !last {
                    prev_read = Some(fds[0]);
                    let _ = close(fds[1]);
                } else {
                    prev_read = None;
                }
            }

            if let Some(fd) = prev_read {
                let _ = close(fd);
            }
            let mut last_status: i32 = 0;
            for pid in pids {
                let mut status: i32 = 0;
                let _ = waitpid(pid, &mut status);
                if pid == last_pid {
                    last_status = status;
                }
            }
            env.set(
                String::from("?"),
                alloc::format!("{}", decode_wait_status(last_status)),
            );
        }
    }
    false
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    // store every line
    let mut line: String = String::new();
    let mut history: Vec<String> = Vec::new();
    let mut history_index: usize = 0;

    // envionment var
    let mut env = Environment::new();
    print_prompt();
    loop {
        let c = getchar();
        if c == 0 {
            // No input available yet; yield to avoid building up NULs.
            user::syscall::_yield();
            continue;
        }
        match c {
            // LF CR = new line
            LF | CR => {
                println!("");
                if !line.is_empty() {
                    history.push(line.clone());
                    history_index = history.len();
                    if eval_stmt(&line, &mut env) {
                        return 0;
                    }
                    line.clear();
                }
                print_prompt();
            }
            BS | DL => {
                if !line.is_empty() {
                    print!("{}", BS as char);
                    print!(" ");
                    print!("{}", BS as char);
                    line.pop();
                }
            }
            0x1B => {
                // ESC sequence for arrow keys: ESC [ A/B
                let next = getchar();
                if next == 0x5B {
                    match getchar() {
                        0x41 => {
                            // up
                            if history_index > 0 {
                                history_index -= 1;
                                print!("\r\x1B[2K");
                                print_prompt();
                                line = history[history_index].clone();
                                print!("{}", line);
                            }
                        }
                        0x42 => {
                            // down
                            if history_index < history.len() {
                                history_index += 1;
                                print!("\r\x1B[2K");
                                print_prompt();
                                if history_index < history.len() {
                                    line = history[history_index].clone();
                                    print!("{}", line);
                                } else {
                                    line.clear();
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {
                print!("{}", c as char);
                line.push(c as char);
            }
        }
    }
}
