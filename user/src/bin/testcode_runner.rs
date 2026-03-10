#![no_std]
#![no_main]

extern crate alloc;

#[macro_use]
extern crate user;

mod shell;

use alloc::{string::String, vec::Vec};
use user::syscall::exec;

use shell::{command::split_sequences, environment::Environment, script::run_script};
use user::syscall::{
    RDONLY, chdir, close, dup3, execve, exit, fork, getcwd, getdents64, kill, open, pipe, sleep,
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

fn exec_command(argv: &[String], env: &Environment) -> ! {
    use shell::command::resolve_exec_candidates;
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
        let rc = execve(&path, &arg_ptrs, &env_ptrs);
        if rc == 0 {
            unreachable!();
        }
        // ENOEXEC fallback: run scripts without shebang via busybox sh.
        if rc == -8 {
            let mut sh_argv: Vec<String> = Vec::new();
            sh_argv.push(String::from("busybox"));
            sh_argv.push(String::from("sh"));
            sh_argv.push(cand);
            exec_command(&sh_argv, env);
        }
    }

    println!("{}: command not found", argv[0]);
    exit(-1);
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
                "sh" | "source" | "." => {
                    let Some(path) = argv.get(1) else {
                        println!("sh: expected script path");
                        env.set(String::from("?"), String::from("1"));
                        continue;
                    };
                    let code = run_script(path, env, eval_stmt);
                    env.set(String::from("?"), alloc::format!("{}", code));
                    continue;
                }
                _ => {}
            }

            if argv[0].ends_with(".sh") {
                let code = run_script(&argv[0], env, eval_stmt);
                env.set(String::from("?"), alloc::format!("{}", code));
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
                    env.set(String::from("?"), alloc::format!("{}", exit_code));
                }
            }
        } else {
            let mut prev_read: Option<usize> = None;
            let mut last_exit: i32 = 0;
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
            for _ in 0..pipeline.commands.len() {
                let mut exit_code: i32 = 0;
                let _ = waitpid(-1, &mut exit_code);
                last_exit = exit_code;
            }
            env.set(String::from("?"), alloc::format!("{}", last_exit));
        }
    }
    false
}

fn list_testcode_scripts(dir_path: &str) -> Vec<String> {
    let fd = open(dir_path, RDONLY);
    if fd < 0 {
        return Vec::new();
    }
    let fd = fd as usize;
    let mut buf = [0u8; 2048];
    let mut scripts: Vec<String> = Vec::new();

    loop {
        let n = getdents64(fd, &mut buf);
        if n <= 0 {
            break;
        }
        let mut pos = 0usize;
        let n = n as usize;
        while pos + 19 <= n {
            let reclen = u16::from_le_bytes([buf[pos + 16], buf[pos + 17]]) as usize;
            if reclen == 0 || pos + reclen > n {
                break;
            }
            let name_start = pos + 19;
            let name_end = pos + reclen;
            let nul = buf[name_start..name_end]
                .iter()
                .position(|&b| b == 0)
                .unwrap_or(name_end - name_start);
            let name_bytes = &buf[name_start..name_start + nul];
            if let Ok(name) = core::str::from_utf8(name_bytes) {
                if name.ends_with("_testcode.sh") {
                    scripts.push(String::from(name));
                }
            }
            pos += reclen;
        }
    }

    let _ = close(fd);
    scripts.sort();
    scripts
}

fn run_busybox_sh(script_name: &str) -> i32 {
    let pid = fork();
    if pid == 0 {
        let mut a0 = String::from("./busybox");
        a0.push('\0');
        let mut a1 = String::from("busybox");
        a1.push('\0');
        let mut a2 = String::from("sh");
        a2.push('\0');
        let mut a3 = String::from(script_name);
        a3.push('\0');
        let args = [a1.as_ptr(), a2.as_ptr(), a3.as_ptr(), core::ptr::null()];
        // Prefer ./busybox (scripts live alongside it), fallback to PATH busybox.
        if exec(&a0, &args) == -1 {
            let mut p = String::from("busybox");
            p.push('\0');
            let _ = exec(&p, &args);
        }
        exit(-1);
    }
    let mut code: i32 = 0;
    let _ = waitpid(pid as isize, &mut code);
    decode_wait_status(code)
}

#[unsafe(no_mangle)]
pub fn main(argc: usize, argv: &[&str]) -> i32 {
    let dir = if argc > 1 { argv[1] } else { "." };
    let cwd0 = getcwd();
    if chdir(dir) != 0 {
        println!("[testcode_runner] cannot cd to '{}'", dir);
        return -1;
    }

    let base = getcwd();
    let scripts = list_testcode_scripts(".");
    println!("[testcode_runner] dir='{}' scripts={}", base, scripts.len());

    let mut env = Environment::new();
    for s in scripts {
        let _ = chdir(&base);
        println!("==== RUN {} ====", s);
        // If busybox exists in this directory, run scripts with busybox sh for full shell support.
        let code = if {
            // Open relative path without "./" to avoid filesystem implementations
            // that don't treat "." as a normal directory entry.
            let fd = open("busybox", RDONLY);
            if fd >= 0 {
                let _ = close(fd as usize);
                true
            } else {
                false
            }
        } {
            run_busybox_sh(&s)
        } else {
            run_script(&s, &mut env, eval_stmt)
        };
        let _ = chdir(&base);
        println!("==== END {} exit_code={} ====", s, code);
    }

    let _ = chdir(&cwd0);
    0
}
