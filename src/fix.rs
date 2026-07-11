//! Auto-fix engine: a faithful port of markdownlint's `applyFixes`, plus a
//! unified-diff renderer for `--dry-run`.

use crate::config::Config;
use crate::linter::{lint_config, split_lines};
use crate::rules::FixInfo;

#[derive(Clone)]
struct Norm {
    line_number: i64,
    edit_column: i64,
    delete_count: i64,
    insert_text: String,
}

fn normalize(fi: &FixInfo, line_number: usize) -> Norm {
    Norm {
        line_number: fi
            .line_number
            .map(|v| v as i64)
            .filter(|v| *v != 0)
            .unwrap_or(line_number as i64),
        edit_column: fi
            .edit_column
            .map(|v| v as i64)
            .filter(|v| *v != 0)
            .unwrap_or(1),
        delete_count: fi.delete_count.unwrap_or(0),
        insert_text: fi.insert_text.clone().unwrap_or_default(),
    }
}

// The no-line-endings branch duplicates the `lf` branch because upstream
// returns `os.EOL` there; fastmdlint always uses "\n". Keep upstream's
// branch order.
#[allow(clippy::if_same_then_else)]
fn preferred_line_ending(input: &str) -> &'static str {
    let (mut cr, mut lf, mut crlf) = (0, 0, 0);
    let bytes: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == '\r' {
            if i + 1 < bytes.len() && bytes[i + 1] == '\n' {
                crlf += 1;
                i += 2;
                continue;
            }
            cr += 1;
        } else if bytes[i] == '\n' {
            lf += 1;
        }
        i += 1;
    }
    if cr == 0 && lf == 0 && crlf == 0 {
        "\n"
    } else if lf >= crlf && lf >= cr {
        "\n"
    } else if crlf >= cr {
        "\r\n"
    } else {
        "\r"
    }
}

fn apply_fix(line: &str, fi: &Norm, line_ending: &str) -> Option<String> {
    if fi.delete_count == -1 {
        return None;
    }
    let chars: Vec<char> = line.chars().collect();
    let edit_index = (fi.edit_column - 1).max(0) as usize;
    let insert = fi.insert_text.replace('\n', line_ending);
    let head: String = chars.iter().take(edit_index.min(chars.len())).collect();
    let tail_start = (edit_index + fi.delete_count.max(0) as usize).min(chars.len());
    let tail: String = chars.iter().skip(tail_start).collect();
    Some(format!("{head}{insert}{tail}"))
}

/// Port of `applyFixes`.
pub fn apply_fixes(input: &str, fixes: &[(usize, FixInfo)]) -> String {
    let line_ending = preferred_line_ending(input);
    let mut lines: Vec<Option<String>> = split_lines(input).into_iter().map(Some).collect();

    let mut fix_infos: Vec<Norm> = fixes.iter().map(|(ln, fi)| normalize(fi, *ln)).collect();

    // Sort bottom-to-top, line-deletes last, right-to-left, long-to-short.
    fix_infos.sort_by(|a, b| {
        let a_del = a.delete_count == -1;
        let b_del = b.delete_count == -1;
        b.line_number
            .cmp(&a.line_number)
            .then_with(|| {
                if a_del {
                    std::cmp::Ordering::Greater
                } else if b_del {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .then(b.edit_column.cmp(&a.edit_column))
            .then(
                b.insert_text
                    .chars()
                    .count()
                    .cmp(&a.insert_text.chars().count()),
            )
    });

    // Remove duplicate entries.
    let mut deduped: Vec<Norm> = Vec::new();
    for fi in fix_infos {
        if let Some(last) = deduped.last()
            && last.line_number == fi.line_number
            && last.edit_column == fi.edit_column
            && last.delete_count == fi.delete_count
            && last.insert_text == fi.insert_text
        {
            continue;
        }
        deduped.push(fi);
    }
    let mut fix_infos = deduped;

    // Collapse insert/no-delete and no-insert/delete for same line/column.
    for i in 1..fix_infos.len() {
        let (prev, cur) = fix_infos.split_at_mut(i);
        let last = &mut prev[i - 1];
        let fi = &mut cur[0];
        if fi.line_number == last.line_number
            && fi.edit_column == last.edit_column
            && fi.insert_text.is_empty()
            && fi.delete_count > 0
            && !last.insert_text.is_empty()
            && last.delete_count == 0
        {
            fi.insert_text = last.insert_text.clone();
            last.line_number = 0;
        }
    }
    fix_infos.retain(|fi| fi.line_number != 0);

    // Apply.
    let mut last_line_index: i64 = -1;
    let mut last_edit_index: i64 = -1;
    for fi in &fix_infos {
        let line_index = fi.line_number - 1;
        let edit_index = fi.edit_column - 1;
        if (line_index != last_line_index
            || fi.delete_count == -1
            || (edit_index + fi.delete_count)
                <= (last_edit_index - if fi.delete_count > 0 { 0 } else { 1 }))
            && line_index >= 0
            && (line_index as usize) < lines.len()
        {
            let idx = line_index as usize;
            if let Some(cur) = &lines[idx] {
                lines[idx] = apply_fix(cur, fi, line_ending);
            }
        }
        last_line_index = line_index;
        last_edit_index = edit_index;
    }

    lines
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(line_ending)
}

/// Result of fixing a document.
pub struct FixResult {
    pub fixed: String,
    pub changed: bool,
}

/// Lint `content`, then apply all reported fixes once (like `markdownlint --fix`).
pub fn fix_content(content: &str, cfg: &Config) -> FixResult {
    let errors = lint_config(content, cfg);
    let fixes: Vec<(usize, FixInfo)> = errors
        .into_iter()
        .filter_map(|e| e.fix_info.map(|fi| (e.line_number, fi)))
        .collect();
    if fixes.is_empty() {
        return FixResult {
            fixed: content.to_string(),
            changed: false,
        };
    }
    let fixed = apply_fixes(content, &fixes);
    let changed = fixed != content;
    FixResult { fixed, changed }
}

/// Render a unified diff (for `--dry-run`).
pub fn unified_diff(path: &str, before: &str, after: &str) -> String {
    let diff = similar::TextDiff::from_lines(before, after);
    let mut out = String::new();
    out.push_str(&format!("--- {path}\n+++ {path}\n"));
    for group in diff.grouped_ops(3) {
        for op in group {
            for change in diff.iter_changes(&op) {
                let sign = match change.tag() {
                    similar::ChangeTag::Delete => "-",
                    similar::ChangeTag::Insert => "+",
                    similar::ChangeTag::Equal => " ",
                };
                out.push_str(sign);
                out.push_str(change.value());
                if !change.value().ends_with('\n') {
                    out.push('\n');
                }
            }
        }
    }
    out
}
