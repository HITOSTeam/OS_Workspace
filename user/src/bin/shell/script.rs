extern crate alloc;

use alloc::{collections::BTreeMap, string::String, vec::Vec};

use user::syscall::{RDONLY, close, open, read};

use super::{command::parse_command, environment::Environment};

fn read_file_to_string(path: &str) -> Option<String> {
    let fd = open(path, RDONLY);
    if fd < 0 {
        return None;
    }
    let fd = fd as usize;
    let mut buf = [0u8; 1024];
    let mut out = String::new();
    loop {
        let n = read(fd, &mut buf);
        if n <= 0 {
            break;
        }
        let n = n as usize;
        out.push_str(core::str::from_utf8(&buf[..n]).ok()?);
    }
    let _ = close(fd);
    Some(out)
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

pub fn split_script_statements(script: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut in_quote: Option<char> = None;
    let mut escape = false;
    let mut in_comment = false;

    for ch in script.chars() {
        if in_comment {
            if ch == '\n' {
                in_comment = false;
                let seg = current.trim();
                if !seg.is_empty() {
                    out.push(String::from(seg));
                }
                current.clear();
            }
            continue;
        }

        if escape {
            current.push(ch);
            escape = false;
            continue;
        }

        if ch == '\\' {
            escape = true;
            current.push(ch);
            continue;
        }

        if ch == '"' || ch == '\'' {
            if in_quote == Some(ch) {
                in_quote = None;
            } else if in_quote.is_none() {
                in_quote = Some(ch);
            }
            current.push(ch);
            continue;
        }

        if ch == '#' && in_quote.is_none() {
            // Start of comment (including shebang); drop until newline.
            in_comment = true;
            continue;
        }

        if (ch == '\n' || ch == ';') && in_quote.is_none() {
            let seg = current.trim();
            if !seg.is_empty() {
                out.push(String::from(seg));
            }
            current.clear();
            continue;
        }

        current.push(ch);
    }

    let seg = current.trim();
    if !seg.is_empty() {
        out.push(String::from(seg));
    }
    out
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

fn parse_assignment(stmt: &str) -> Option<(String, String)> {
    let cmd = parse_command(stmt);
    if cmd.argv.len() != 1 {
        return None;
    }
    let s = &cmd.argv[0];
    let (k, v) = s.split_once('=')?;
    if !is_ident(k) {
        return None;
    }
    Some((String::from(k), String::from(v)))
}

fn is_func_def(stmt: &str) -> Option<String> {
    let cmd = parse_command(stmt);
    if cmd.argv.len() < 2 {
        return None;
    }
    if cmd.argv[1] != "{" {
        return None;
    }
    let name_tok = cmd.argv[0].as_str();
    let Some(name) = name_tok.strip_suffix("()") else {
        return None;
    };
    if !is_ident(name) {
        return None;
    }
    Some(String::from(name))
}

fn eval_test(tokens: &[String]) -> bool {
    if tokens.len() < 3 {
        return false;
    }
    let lhs = tokens[0].as_str();
    let op = tokens[1].as_str();
    let rhs = tokens[2].as_str();
    match op {
        "==" | "=" => lhs == rhs,
        "!=" => lhs != rhs,
        "-eq" => lhs.parse::<isize>().ok() == rhs.parse::<isize>().ok(),
        "-ne" => lhs.parse::<isize>().ok() != rhs.parse::<isize>().ok(),
        "-lt" => lhs.parse::<isize>().ok() < rhs.parse::<isize>().ok(),
        "-le" => lhs.parse::<isize>().ok() <= rhs.parse::<isize>().ok(),
        "-gt" => lhs.parse::<isize>().ok() > rhs.parse::<isize>().ok(),
        "-ge" => lhs.parse::<isize>().ok() >= rhs.parse::<isize>().ok(),
        _ => false,
    }
}

fn eval_if_condition(stmt: &str) -> Option<bool> {
    let cmd = parse_command(stmt);
    if cmd.argv.first().map(|s| s.as_str()) != Some("if") {
        return None;
    }
    if cmd.argv.len() >= 2 && cmd.argv[1] == "[" {
        let mut end = None;
        for (i, t) in cmd.argv.iter().enumerate().skip(2) {
            if t == "]" {
                end = Some(i);
                break;
            }
        }
        let end = end?;
        let inner = &cmd.argv[2..end];
        return Some(eval_test(inner));
    }
    None
}

fn run_stmts(
    stmts: &[String],
    idx: &mut usize,
    end: usize,
    env: &mut Environment,
    funcs: &mut BTreeMap<String, Vec<String>>,
    eval_stmt: &mut impl FnMut(&str, &mut Environment) -> bool,
) -> bool {
    while *idx < end {
        let raw = stmts[*idx].trim();
        if raw.is_empty() {
            *idx += 1;
            continue;
        }

        // Function definition: name() { ... }
        if let Some(name) = is_func_def(raw) {
            let mut body: Vec<String> = Vec::new();
            *idx += 1;
            while *idx < end {
                let s = stmts[*idx].trim();
                if s == "}" {
                    *idx += 1;
                    break;
                }
                body.push(String::from(stmts[*idx].as_str()));
                *idx += 1;
            }
            funcs.insert(name, body);
            continue;
        }

        // Expand variables for control + commands.
        // NOTE: do not expand before parsing bare assignments.
        // In POSIX shell, `x=$2` assigns the full expanded string (including spaces)
        // without word splitting. If we expand first and then tokenize, we lose that.
        if let Some((k, v_raw)) = parse_assignment(raw) {
            let v = env.expand_variables(&v_raw);
            env.set(k, v);
            *idx += 1;
            continue;
        }

        let expanded = env.expand_variables(raw);

        // Ignore structural tokens.
        if expanded == "do"
            || expanded == "done"
            || expanded == "then"
            || expanded == "else"
            || expanded == "fi"
            || expanded == "{"
            || expanded == "}"
        {
            *idx += 1;
            continue;
        }

        // for var in LIST ; do ... ; done
        {
            let cmd = parse_command(&expanded);
            if cmd.argv.first().map(|s| s.as_str()) == Some("for") {
                if cmd.argv.len() < 4 || cmd.argv.get(2).map(|s| s.as_str()) != Some("in") {
                    *idx += 1;
                    continue;
                }
                let var = cmd.argv[1].clone();
                if !is_ident(&var) {
                    *idx += 1;
                    continue;
                }
                let list_expr_raw = cmd.argv[3..].join(" ");
                let list_expanded = env.expand_variables(&list_expr_raw);
                let items: Vec<&str> = list_expanded.split_whitespace().collect();

                // Expect "do" as the next non-empty statement.
                let mut j = *idx + 1;
                while j < end && stmts[j].trim().is_empty() {
                    j += 1;
                }
                if j >= end || env.expand_variables(stmts[j].trim()) != "do" {
                    *idx += 1;
                    continue;
                }
                let body_start = j + 1;
                let mut k = body_start;
                while k < end {
                    if env.expand_variables(stmts[k].trim()) == "done" {
                        break;
                    }
                    k += 1;
                }
                if k >= end {
                    *idx = end;
                    return false;
                }
                let body_end = k;
                for it in items {
                    env.set(var.clone(), String::from(it));
                    let mut inner = body_start;
                    if run_stmts(stmts, &mut inner, body_end, env, funcs, eval_stmt) {
                        return true;
                    }
                }
                *idx = k + 1;
                continue;
            }
        }

        // if [ ... ] ; then ... ; else ... ; fi
        if expanded.starts_with("if ") || expanded == "if" {
            let cond = eval_if_condition(&expanded).unwrap_or(false);
            let mut j = *idx + 1;
            while j < end && stmts[j].trim().is_empty() {
                j += 1;
            }
            if j >= end || env.expand_variables(stmts[j].trim()) != "then" {
                *idx += 1;
                continue;
            }
            let then_start = j + 1;

            let mut depth = 1usize;
            let mut else_at: Option<usize> = None;
            j = then_start;
            while j < end {
                let s = env.expand_variables(stmts[j].trim());
                if s.starts_with("if ") || s == "if" {
                    depth += 1;
                } else if s == "fi" {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                } else if depth == 1 && s == "else" {
                    else_at = Some(j);
                }
                j += 1;
            }
            if j >= end {
                *idx = end;
                return false;
            }
            let fi_at = j;
            let then_end = else_at.unwrap_or(fi_at);
            let else_start = else_at.map(|p| p + 1);

            if cond {
                let mut inner = then_start;
                if run_stmts(stmts, &mut inner, then_end, env, funcs, eval_stmt) {
                    return true;
                }
            } else if let Some(es) = else_start {
                let mut inner = es;
                if run_stmts(stmts, &mut inner, fi_at, env, funcs, eval_stmt) {
                    return true;
                }
            }
            *idx = fi_at + 1;
            continue;
        }

        // Function invocation.
        {
            let cmd = parse_command(&expanded);
            if let Some(name) = cmd.argv.first() {
                if let Some(body) = funcs.get(name).cloned() {
                    // Save old positional params.
                    let mut old: Vec<(String, Option<String>)> = Vec::new();
                    for (i, arg) in cmd.argv.iter().skip(1).enumerate() {
                        let key = alloc::format!("{}", i + 1);
                        old.push((key.clone(), env.get(&key).map(|s| String::from(s))));
                        env.set(key, arg.clone());
                    }
                    // Clear leftover higher args (best-effort).
                    let key3 = String::from("3");
                    if cmd.argv.len() <= 3 {
                        old.push((key3.clone(), env.get(&key3).map(|s| String::from(s))));
                        env.set(key3, String::new());
                    }

                    let mut inner = 0usize;
                    let exit = run_stmts(&body, &mut inner, body.len(), env, funcs, eval_stmt);

                    // Restore.
                    for (k, v) in old {
                        if let Some(v) = v {
                            env.set(k, v);
                        } else {
                            env.unset(&k);
                        }
                    }
                    *idx += 1;
                    if exit {
                        return true;
                    }
                    continue;
                }
            }
        }

        if eval_stmt(&expanded, env) {
            return true;
        }
        *idx += 1;
    }
    false
}

pub fn run_script(
    path: &str,
    env: &mut Environment,
    mut eval_stmt: impl FnMut(&str, &mut Environment) -> bool,
) -> i32 {
    let Some(content) = read_file_to_string(path) else {
        return -1;
    };
    let stmts = split_script_statements(&content);
    let mut funcs: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut i = 0usize;
    let exit = run_stmts(&stmts, &mut i, stmts.len(), env, &mut funcs, &mut eval_stmt);
    if exit { 0 } else { 0 }
}
