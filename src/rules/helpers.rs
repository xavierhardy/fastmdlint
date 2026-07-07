//! Shared helpers for rules: config accessors and small text utilities that
//! mirror markdownlint's helper functions.

use serde_json::Value;

/// `ellipsify(text, start, end)` from helpers.cjs.
pub fn ellipsify(text: &str, start: bool, end: bool) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= 30 {
        text.to_string()
    } else if start && end {
        let head: String = chars[..15].iter().collect();
        let tail: String = chars[chars.len() - 15..].iter().collect();
        format!("{head}...{tail}")
    } else if end {
        let tail: String = chars[chars.len() - 30..].iter().collect();
        format!("...{tail}")
    } else {
        let head: String = chars[..30].iter().collect();
        format!("{head}...")
    }
}

/// `isBlankLine`: blank if empty/whitespace, or — after removing HTML
/// comments and all `>` characters — only whitespace remains.
pub fn is_blank_line(line: &str) -> bool {
    if line.trim().is_empty() {
        return true;
    }
    remove_comments(line).replace('>', "").trim().is_empty()
}

fn remove_comments(input: &str) -> String {
    let mut s = input.to_string();
    loop {
        let start = s.find("<!--");
        let end = s.find("-->");
        match (start, end) {
            (s0, Some(e)) if s0.is_none() || e < s0.unwrap() => {
                s = s[e + 3..].to_string();
            }
            (Some(st), Some(e)) => {
                s = format!("{}{}", &s[..st], &s[e + 3..]);
            }
            (Some(st), None) => {
                s = s[..st].to_string();
            }
            _ => return s,
        }
    }
}

/// Mirror of `isHtmlFlowComment`.
pub fn is_html_flow_comment(tree: &crate::md::Tree, idx: usize) -> bool {
    let t = tree.get(idx);
    if t.kind != "htmlFlow" {
        return false;
    }
    let text = &t.text;
    if text.starts_with("<!--") && text.ends_with("-->") && text.chars().count() >= 7 {
        let comment: String = {
            let chars: Vec<char> = text.chars().collect();
            chars[4..chars.len() - 3].iter().collect()
        };
        !comment.starts_with('>') && !comment.starts_with("->") && !comment.ends_with('-')
    } else {
        false
    }
}

/// Config option access on a rule's options object.
pub trait ConfigExt {
    fn opt(&self, key: &str) -> Option<&Value>;
    fn opt_bool(&self, key: &str, default: bool) -> bool;
    fn opt_i64(&self, key: &str, default: i64) -> i64;
    fn opt_str<'a>(&'a self, key: &str) -> Option<&'a str>;
    fn opt_str_or<'a>(&'a self, key: &str, default: &'a str) -> &'a str;
    fn opt_array(&self, key: &str) -> Option<&Vec<Value>>;
}

impl ConfigExt for Value {
    fn opt(&self, key: &str) -> Option<&Value> {
        self.get(key).filter(|v| !v.is_null())
    }
    fn opt_bool(&self, key: &str, default: bool) -> bool {
        match self.get(key) {
            Some(Value::Bool(b)) => *b,
            Some(Value::Null) | None => default,
            Some(v) => v.as_bool().unwrap_or(default),
        }
    }
    fn opt_i64(&self, key: &str, default: i64) -> i64 {
        match self.get(key) {
            Some(v) => v.as_i64().or_else(|| v.as_f64().map(|f| f as i64)).unwrap_or(default),
            None => default,
        }
    }
    fn opt_str<'a>(&'a self, key: &str) -> Option<&'a str> {
        self.get(key).and_then(|v| v.as_str())
    }
    fn opt_str_or<'a>(&'a self, key: &str, default: &'a str) -> &'a str {
        self.opt_str(key).unwrap_or(default)
    }
    fn opt_array(&self, key: &str) -> Option<&Vec<Value>> {
        self.get(key).and_then(|v| v.as_array())
    }
}

/// True when the front matter contains a title (used by MD001/MD025/MD041).
/// Mirrors `frontMatterHasTitle`. Default pattern: `^\s*"?title"?\s*[:=]`.
pub fn front_matter_has_title(front_matter_lines: &[String], pattern: Option<&str>) -> bool {
    if front_matter_lines.is_empty() {
        return false;
    }
    // The default pattern regex is compiled once; a custom pattern compiles
    // per call.
    let default_re = || -> &'static regex::Regex {
        use std::sync::OnceLock;
        static RE: OnceLock<regex::Regex> = OnceLock::new();
        RE.get_or_init(|| regex::Regex::new(r#"(?i)^\s*"?title"?\s*[:=]"#).unwrap())
    };
    let custom;
    let re: &regex::Regex = match pattern {
        None => default_re(),
        Some(p) => {
            custom = match regex::Regex::new(&format!("(?i){p}")) {
                Ok(r) => r,
                Err(_) => return false,
            };
            &custom
        }
    };
    front_matter_lines.iter().any(|l| re.is_match(l))
}
