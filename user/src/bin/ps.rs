#![no_std]
#![no_main]

extern crate alloc;
#[macro_use]
extern crate user;

use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use user::syscall::{RDONLY, close, getdents64, open, read};

struct ProcInfo {
    pid: u32,
    ppid: u32,
    pgrp: u32,
    tpgid: i32,
    state: char,
    tty_nr: i32,
    comm: String,
}

fn read_file(path: &str) -> Option<Vec<u8>> {
    let fd = open(path, RDONLY);
    if fd < 0 {
        return None;
    }
    let fd = fd as usize;
    let mut data = Vec::new();
    let mut buf = [0u8; 256];
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

fn parse_pid_list(text: &str) -> Vec<u32> {
    let mut out = Vec::new();
    let mut cur: u32 = 0;
    let mut in_num = false;
    for b in text.bytes() {
        if b.is_ascii_digit() {
            cur = cur.saturating_mul(10).saturating_add((b - b'0') as u32);
            in_num = true;
        } else if in_num {
            out.push(cur);
            cur = 0;
            in_num = false;
        }
    }
    if in_num {
        out.push(cur);
    }
    out
}

fn list_proc_pids() -> Vec<u32> {
    let fd = open("/proc", RDONLY);
    if fd < 0 {
        return Vec::new();
    }
    let fd = fd as usize;
    let mut out = Vec::new();
    let mut buf = [0u8; 1024];
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
                let mut pids = parse_pid_list(name);
                if pids.len() == 1 && pids[0].to_string() == name {
                    out.push(pids.pop().unwrap());
                }
            }
            pos += reclen;
        }
    }
    let _ = close(fd);
    out.sort_unstable();
    out
}

fn proc_info(pid: u32) -> Option<ProcInfo> {
    let mut path = String::from("/proc/");
    path.push_str(pid.to_string().as_str());
    path.push_str("/stat");
    let stat = read_file(&path)?;
    let stat = core::str::from_utf8(&stat).ok()?;
    let lparen = stat.find('(')?;
    let rparen = stat.rfind(')')?;
    let comm = stat[lparen + 1..rparen].to_string();
    let rest = stat.get(rparen + 2..)?.trim();
    let fields: Vec<&str> = rest.split_whitespace().collect();
    if fields.len() < 6 {
        return None;
    }
    Some(ProcInfo {
        pid,
        state: fields[0].chars().next().unwrap_or('?'),
        ppid: fields[1].parse().ok()?,
        pgrp: fields[2].parse().ok()?,
        tty_nr: fields[4].parse().ok()?,
        tpgid: fields[5].parse().ok()?,
        comm,
    })
}

fn render_tty(tty_nr: i32) -> String {
    if tty_nr == 0 {
        String::from("?")
    } else {
        tty_nr.to_string()
    }
}

fn render_field(info: &ProcInfo, field: &str) -> String {
    let name = field.split('=').next().unwrap_or(field).trim();
    match name {
        "state" | "stat" => info.state.to_string(),
        "pid" | "tid" => info.pid.to_string(),
        "ppid" => info.ppid.to_string(),
        "pgid" => info.pgrp.to_string(),
        "tpgid" => info.tpgid.to_string(),
        "tty" => render_tty(info.tty_nr),
        "args" => info.comm.clone(),
        "blocked" | "ignored" | "pending" => String::from("0"),
        _ => String::from("?"),
    }
}

fn usage() -> i32 {
    println!("usage: ps [-e | -p PID[,PID...]] -o FORMAT");
    1
}

#[unsafe(no_mangle)]
pub fn main(argc: usize, argv: &[&str]) -> i32 {
    let mut pids: Vec<u32> = Vec::new();
    let mut format = "";
    let mut all = false;
    let mut i = 1usize;
    while i < argc {
        match argv[i] {
            "-e" => {
                all = true;
            }
            "-p" => {
                i += 1;
                if i >= argc {
                    return usage();
                }
                pids = parse_pid_list(argv[i]);
            }
            "-o" => {
                i += 1;
                if i >= argc {
                    return usage();
                }
                format = argv[i];
            }
            _ => return usage(),
        }
        i += 1;
    }

    if all {
        pids = list_proc_pids();
    }

    if pids.is_empty() || format.is_empty() {
        return usage();
    }

    let cols: Vec<&str> = format
        .split(',')
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .collect();
    if cols.is_empty() {
        return usage();
    }

    for pid in pids {
        let Some(info) = proc_info(pid) else {
            continue;
        };
        let mut line = String::new();
        for (idx, col) in cols.iter().enumerate() {
            if idx != 0 {
                line.push(' ');
            }
            line.push_str(render_field(&info, col).as_str());
        }
        println!("{}", line);
    }

    0
}
