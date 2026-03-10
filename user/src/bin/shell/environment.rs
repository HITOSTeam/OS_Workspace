extern crate alloc;

use alloc::{collections::BTreeMap, string::String, vec::Vec};

pub struct Environment {
    vars: BTreeMap<String, String>,
}

impl Environment {
    pub fn new() -> Self {
        let mut env = Self {
            vars: BTreeMap::new(),
        };
        env.vars.insert(String::from("HOME"), String::from("/"));
        env.vars.insert(
            String::from("PATH"),
            String::from("/user:/:/bin:/usr/bin:/musl:/glibc"),
        );
        env
    }

    pub fn set(&mut self, key: String, value: String) {
        self.vars.insert(key, value);
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.vars.get(key).map(|s| s.as_str())
    }

    pub fn unset(&mut self, key: &str) {
        self.vars.remove(key);
    }

    pub fn list_all(&self) -> Vec<(String, String)> {
        self.vars
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    pub fn path_dirs(&self) -> Vec<&str> {
        self.get("PATH")
            .unwrap_or("")
            .split(':')
            .filter(|s| !s.is_empty())
            .collect()
    }

    // Expand variables in the input string, e.g., $HOME or ${PATH}
    pub fn expand_variables(&self, input: &str) -> String {
        let mut out = String::new();
        let mut chars = input.chars().peekable();
        let mut in_single = false;
        let mut in_double = false;
        let mut escape = false;
        while let Some(ch) = chars.next() {
            if escape {
                out.push(ch);
                escape = false;
                continue;
            }
            if ch == '\\' && !in_single {
                escape = true;
                out.push(ch);
                continue;
            }
            if ch == '\'' && !in_double {
                in_single = !in_single;
                out.push(ch);
                continue;
            }
            if ch == '"' && !in_single {
                in_double = !in_double;
                out.push(ch);
                continue;
            }
            if ch != '$' {
                out.push(ch);
                continue;
            }

            if in_single {
                out.push(ch);
                continue;
            }

            if chars.peek() == Some(&'?') {
                let _ = chars.next();
                if let Some(v) = self.get("?") {
                    out.push_str(v);
                }
                continue;
            }
            if chars.peek() == Some(&'!') {
                let _ = chars.next();
                if let Some(v) = self.get("!") {
                    out.push_str(v);
                }
                continue;
            }

            if chars.peek() == Some(&'{') {
                let _ = chars.next();
                let mut name = String::new();
                while let Some(&c) = chars.peek() {
                    let _ = chars.next();
                    if c == '}' {
                        break;
                    }
                    name.push(c);
                }
                if let Some(v) = self.get(&name) {
                    out.push_str(v);
                }
                continue;
            }

            let mut name = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_alphanumeric() || c == '_' {
                    let _ = chars.next();
                    name.push(c);
                } else {
                    break;
                }
            }
            if name.is_empty() {
                out.push('$');
            } else if let Some(v) = self.get(&name) {
                out.push_str(v);
            }
        }
        out
    }
}
