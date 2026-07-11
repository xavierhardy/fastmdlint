//! The linting pipeline: front-matter removal, inline-config comment
//! handling, parsing, rule execution and line-number offsetting — mirroring
//! markdownlint's `lintContent`.

use std::collections::HashMap;

use regex::Regex;
use std::sync::OnceLock;

use crate::config::ResolvedConfig;
use crate::md::Parser;
use crate::rules::{Emit, FixInfo, Params, RULES, Severity};

/// A fully-resolved problem, ready for output.
#[derive(Debug, Clone)]
pub struct LintError {
    pub line_number: usize,
    pub rule_names: &'static [&'static str],
    pub rule_description: &'static str,
    pub rule_information: String,
    pub error_detail: Option<String>,
    pub error_context: Option<String>,
    pub error_range: Option<(usize, usize)>,
    pub fix_info: Option<FixInfo>,
    pub severity: Severity,
}

fn front_matter_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(concat!(
            r"(?m)^(?:",
            r"(?:---[^\S\r\n\u{2028}\u{2029}]*$[\s\S]+?^---\s*)",
            r"|(?:\+\+\+[^\S\r\n\u{2028}\u{2029}]*$[\s\S]+?^(?:\+\+\+|\.\.\.)\s*)",
            r"|(?:\{[^\S\r\n\u{2028}\u{2029}]*$[\s\S]+?^\}\s*)",
            r")(?:\r\n|\r|\n|$)"
        ))
        .unwrap()
    })
}

fn inline_comment_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)(<!--\s*markdownlint-(disable|enable|capture|restore|disable-file|enable-file|disable-line|disable-next-line|configure-file))(?:\s|-->)",
        )
        .unwrap()
    })
}

/// Split on `\r\n`, `\r`, or `\n` like markdownlint's newLineRe.
pub fn split_lines(content: &str) -> Vec<String> {
    let re = {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| Regex::new(r"\r\n|\r|\n").unwrap())
    };
    re.split(content).map(|s| s.to_string()).collect()
}

/// Remove YAML/TOML front matter; returns (stripped content, front matter lines).
fn remove_front_matter(content: &str) -> (String, Vec<String>) {
    if let Some(m) = front_matter_re().find(content)
        && m.start() == 0
    {
        let matched = &content[..m.end()];
        let stripped = content[m.end()..].to_string();
        let mut fm: Vec<String> = split_lines(matched);
        if fm.last().map(|s| s.is_empty()).unwrap_or(false) {
            fm.pop();
        }
        return (stripped, fm);
    }
    (content.to_string(), Vec::new())
}

/// Mirror of `clearHtmlCommentText`.
pub fn clear_html_comment_text(text: &str) -> String {
    let begin = "<!--";
    let end = "-->";
    let mut bytes = text.to_string();
    let mut search_from = 0usize;
    while let Some(p) = bytes[search_from..].find(begin) {
        let i = search_from + p;
        let j = match bytes[i + 2..].find(end) {
            Some(p) => i + 2 + p,
            None => break, // unterminated => treated as text
        };
        if j > i + begin.len() {
            let content = &bytes[i + begin.len()..j];
            let last_lf = bytes[..i].rfind('\n').map(|p| p + 1).unwrap_or(0);
            let pre_text = &bytes[last_lf..i];
            let is_block = pre_text.trim().is_empty();
            let could_be_table = pipe_re().is_match(pre_text);
            let spans_table_cells = could_be_table && content.contains('\n');
            let is_valid = is_block
                || !(spans_table_cells
                    || content.starts_with('>')
                    || content.starts_with("->")
                    || content.ends_with('-')
                    || content.contains("--"));
            if is_valid {
                let cleared = clear_content(content);
                let new = format!("{}{}{}", &bytes[..i + begin.len()], cleared, &bytes[j..]);
                bytes = new;
            }
        }
        search_from = j + end.len();
        if search_from > bytes.len() {
            break;
        }
    }
    bytes
}

fn pipe_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^ *\|").unwrap())
}

/// Replace non-(space/CR/LF) with "." then spaces-before-newline with ".".
fn clear_content(content: &str) -> String {
    let step1: String = content
        .chars()
        .map(|c| {
            if c == ' ' || c == '\r' || c == '\n' {
                c
            } else {
                '.'
            }
        })
        .collect();
    // trailingSpaceRe: / +[\r\n]/g -> replace spaces with "." keeping the newline
    let re = {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| Regex::new(r" +[\r\n]").unwrap())
    };
    re.replace_all(&step1, |caps: &regex::Captures| {
        let m = caps.get(0).unwrap().as_str();
        m.chars()
            .map(|c| if c == '\r' || c == '\n' { c } else { '.' })
            .collect::<String>()
    })
    .to_string()
}

/// Compute enabled-per-line map. Index = final (front-matter-offset) line
/// number. Returns (per_line, any_enabled_ruleids).
fn enabled_per_line(
    cfg: &ResolvedConfig,
    uncleared_lines: &[String],
    fm_len: usize,
) -> (Vec<HashMap<&'static str, bool>>, Vec<&'static str>) {
    let alias = build_alias_map();
    let all_rule_ids: Vec<&'static str> = RULES.iter().map(|r| r.names[0]).collect();

    let base: HashMap<&'static str, bool> = RULES
        .iter()
        .map(|r| {
            (
                r.names[0],
                cfg.get(r.names[0]).map(|c| c.enabled).unwrap_or(true),
            )
        })
        .collect();

    let mut enabled = base.clone();

    // Pass 1: enable-file / disable-file
    for line in uncleared_lines {
        for (action, param) in scan_inline(line) {
            let au = action.to_uppercase();
            if au == "ENABLE-FILE" || au == "DISABLE-FILE" {
                apply_enable_disable(&mut enabled, &au, &param, &alias, &all_rule_ids);
            }
        }
    }

    // Pass 2: capture/restore/enable/disable with per-line snapshots
    let mut per_line: Vec<HashMap<&'static str, bool>> = Vec::with_capacity(fm_len + 1);
    for _ in 0..(fm_len + 1) {
        per_line.push(HashMap::new()); // holes for front matter + index 0
    }
    let mut captured = enabled.clone();
    for line in uncleared_lines {
        for (action, param) in scan_inline(line) {
            let au = action.to_uppercase();
            match au.as_str() {
                "CAPTURE" => captured = enabled.clone(),
                "RESTORE" => enabled = captured.clone(),
                "ENABLE" | "DISABLE" => {
                    apply_enable_disable(&mut enabled, &au, &param, &alias, &all_rule_ids)
                }
                _ => {}
            }
        }
        per_line.push(enabled.clone());
    }

    // Pass 3: disable-line / disable-next-line
    for (line_index, line) in uncleared_lines.iter().enumerate() {
        for (action, param) in scan_inline(line) {
            let au = action.to_uppercase();
            if au == "DISABLE-LINE" || au == "DISABLE-NEXT-LINE" {
                let next =
                    fm_len + (line_index + 1) + if au == "DISABLE-NEXT-LINE" { 1 } else { 0 };
                if next < per_line.len() {
                    let mut state = per_line[next].clone();
                    apply_enable_disable(&mut state, &au, &param, &alias, &all_rule_ids);
                    per_line[next] = state;
                }
            }
        }
    }

    let mut any: Vec<&'static str> = Vec::new();
    for id in &all_rule_ids {
        if per_line.iter().any(|m| *m.get(id).unwrap_or(&false)) {
            any.push(id);
        }
    }
    (per_line, any)
}

/// Apply `<!-- markdownlint-configure-file <json> -->` directives found
/// anywhere in `content`, shallow-merging each parsed object into `raw`.
fn apply_configure_file(mut raw: serde_json::Value, content: &str) -> serde_json::Value {
    for caps in inline_comment_re().captures_iter(content) {
        let action = caps.get(2).unwrap().as_str();
        if !action.eq_ignore_ascii_case("configure-file") {
            continue;
        }
        let start = caps.get(1).unwrap().end();
        if let Some(rel) = content[start..].find("-->") {
            let param = content[start..start + rel].trim();
            if let Ok(parsed) = crate::config::parse_config_content(param)
                && let (Some(base), Some(over)) = (raw.as_object_mut(), parsed.as_object())
            {
                for (k, v) in over {
                    base.insert(k.clone(), v.clone());
                }
            }
        }
    }
    raw
}

fn scan_inline(line: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for caps in inline_comment_re().captures_iter(line) {
        let m1 = caps.get(1).unwrap();
        let action = caps.get(2).unwrap().as_str().to_string();
        let start_index = m1.end();
        if let Some(rel) = line[start_index..].find("-->") {
            let end_index = start_index + rel;
            let parameter = line[start_index..end_index].to_string();
            out.push((action, parameter));
        }
    }
    out
}

fn apply_enable_disable(
    state: &mut HashMap<&'static str, bool>,
    action: &str,
    parameter: &str,
    alias: &HashMap<String, Vec<&'static str>>,
    all_rule_ids: &[&'static str],
) {
    let enabled = action.starts_with("ENABLE");
    let trimmed = parameter.trim();
    let items: Vec<String> = if trimmed.is_empty() {
        all_rule_ids.iter().map(|s| s.to_uppercase()).collect()
    } else {
        trimmed
            .to_uppercase()
            .split_whitespace()
            .map(String::from)
            .collect()
    };
    for name in items {
        if let Some(ids) = alias.get(&name) {
            for id in ids {
                state.insert(id, enabled);
            }
        }
    }
}

fn build_alias_map() -> HashMap<String, Vec<&'static str>> {
    let mut map: HashMap<String, Vec<&'static str>> = HashMap::new();
    for rule in RULES {
        let id = rule.names[0];
        for name in rule.names {
            map.entry(name.to_uppercase()).or_default().push(id);
        }
        for tag in rule.tags {
            map.entry(tag.to_uppercase()).or_default().push(id);
        }
    }
    map
}

/// Lint markdown `content` with the resolved configuration.
pub fn lint(content: &str, cfg: &ResolvedConfig) -> Vec<LintError> {
    lint_with_raw(content, cfg, None)
}

/// Lint using the raw [`Config`], honouring document-wide
/// `markdownlint-configure-file` directives (which merge into the config).
pub fn lint_config(content: &str, raw: &crate::config::Config) -> Vec<LintError> {
    let resolved = raw.resolve();
    lint_with_raw(content, &resolved, Some(raw))
}

fn lint_with_raw(
    content: &str,
    cfg: &ResolvedConfig,
    raw: Option<&crate::config::Config>,
) -> Vec<LintError> {
    // Strip BOM.
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);
    // Remove front matter.
    let (stripped, fm_lines) = remove_front_matter(content);
    let fm_len = fm_lines.len();

    // Apply configure-file directives (document-wide config merge).
    let owned_resolved;
    let cfg: &ResolvedConfig = match raw {
        Some(raw) => {
            let merged = apply_configure_file(raw.raw.clone(), &stripped);
            owned_resolved = crate::config::Config::from_value(merged).resolve();
            &owned_resolved
        }
        None => cfg,
    };

    let uncleared_lines = split_lines(&stripped);
    let (per_line, enabled_rule_ids) = enabled_per_line(cfg, &uncleared_lines, fm_len);

    // Parse tree from uncleared, front-matter-stripped content — but only if
    // some enabled rule uses the parser. markdownlint skips micromark parsing
    // otherwise, which makes its parser-"none" rules (MD047/MD052/MD053) see
    // an empty token list; an empty tree reproduces that.
    let need_tokens = RULES
        .iter()
        .any(|r| r.micromark && enabled_rule_ids.contains(&r.names[0]));
    let tree = if need_tokens {
        Parser::parse(&stripped)
    } else {
        crate::md::Tree::default()
    };

    // Lines given to rules: HTML-comment cleared.
    let cleared = clear_html_comment_text(&stripped);
    let lines = split_lines(&cleared);

    let mut results: Vec<LintError> = Vec::new();
    for rule in RULES {
        let id = rule.names[0];
        if !enabled_rule_ids.contains(&id) {
            continue;
        }
        let rc = cfg.get(id);
        let options = rc
            .map(|c| c.options.clone())
            .unwrap_or(serde_json::Value::Object(Default::default()));
        let severity = rc.map(|c| c.severity).unwrap_or(Severity::Error);
        let params = Params {
            lines: &lines,
            front_matter_lines: &fm_lines,
            tree: &tree,
            config: &options,
        };
        let mut emit = Emit::new();
        (rule.run)(&params, &mut emit);
        for e in emit.errors {
            // Validate lineNumber in range (rules should be correct).
            if e.line_number == 0 || e.line_number > lines.len() {
                continue;
            }
            let final_line = e.line_number + fm_len;
            if !per_line
                .get(final_line)
                .and_then(|m| m.get(id))
                .copied()
                .unwrap_or(false)
            {
                continue;
            }
            let fix_info = e.fix_info.map(|mut f| {
                if let Some(ln) = f.line_number {
                    f.line_number = Some(ln + fm_len);
                }
                f
            });
            results.push(LintError {
                line_number: final_line,
                rule_names: rule.names,
                rule_description: rule.description,
                rule_information: rule.information(),
                error_detail: e.detail.map(|d| d.replace(['\r', '\n'], " ")),
                error_context: e.context.map(|c| c.replace(['\r', '\n'], " ")),
                error_range: e.range,
                fix_info,
                severity,
            });
        }
    }
    results
}
