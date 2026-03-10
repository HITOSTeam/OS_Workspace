extern crate alloc;

use alloc::{string::String, vec::Vec};

use super::environment::Environment;

#[derive(Clone, Debug)]
pub struct Command {
    pub argv: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Pipeline {
    pub commands: Vec<Command>,
}

pub fn split_sequences(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut in_quote: Option<char> = None;
    let mut escape = false;

    for ch in line.chars() {
        if escape {
            current.push(ch);
            escape = false;
            continue;
        }
        if ch == '\\' {
            escape = true;
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
        if ch == ';' && in_quote.is_none() {
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

pub fn parse_pipeline(line: &str) -> Pipeline {
    let mut commands = Vec::new();
    let mut current = String::new();
    let mut in_quote: Option<char> = None;
    let mut escape = false;

    for ch in line.chars() {
        if escape {
            current.push(ch);
            escape = false;
            continue;
        }
        if ch == '\\' {
            escape = true;
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
        if ch == '|' && in_quote.is_none() {
            let seg = current.trim();
            if !seg.is_empty() {
                commands.push(parse_command(seg));
            }
            current.clear();
            continue;
        }
        current.push(ch);
    }

    let seg = current.trim();
    if !seg.is_empty() {
        commands.push(parse_command(seg));
    }
    Pipeline { commands }
}

pub fn parse_command(segment: &str) -> Command {
    Command {
        argv: tokenize(segment),
    }
}

fn tokenize(segment: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut in_quote: Option<char> = None;
    let mut escape = false;

    for ch in segment.chars() {
        if escape {
            current.push(match ch {
                'n' => '\n',
                't' => '\t',
                'r' => '\r',
                _ => ch,
            });
            escape = false;
            continue;
        }

        if ch == '\\' {
            escape = true;
            continue;
        }

        if ch == '"' || ch == '\'' {
            if in_quote == Some(ch) {
                in_quote = None;
            } else if in_quote.is_none() {
                in_quote = Some(ch);
            } else {
                current.push(ch);
            }
            continue;
        }

        if ch.is_whitespace() && in_quote.is_none() {
            if !current.is_empty() {
                out.push(core::mem::take(&mut current));
            }
            continue;
        }

        current.push(ch);
    }

    if !current.is_empty() {
        out.push(current);
    }
    out
}

pub fn resolve_exec_candidates(env: &Environment, cmd: &str) -> Vec<String> {
    if cmd.contains('/') {
        let mut out = Vec::new();
        out.push(String::from(cmd));
        return out;
    }

    let mut out = Vec::new();
    out.push(String::from(cmd));
    for dir in env.path_dirs() {
        let mut path = String::from(dir);
        if !path.ends_with('/') {
            path.push('/');
        }
        path.push_str(cmd);
        out.push(path);
    }
    out
}
