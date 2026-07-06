//! MD020 — no-missing-space-closed-atx.

use std::collections::HashSet;

use super::{Emit, FixInfo, Params, RuleMeta};
use regex::Regex;
use std::sync::OnceLock;

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD020", "no-missing-space-closed-atx"],
    description: "No space inside hashes on closed atx style heading",
    tags: &["headings", "atx_closed", "spaces"],
    micromark: true,
    run,
};

fn closed_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^(#+)([ \t]*)([^# \t\\]|[^# \t][^#]*?[^# \t\\])([ \t]*)((?:\\#)?)(#+)(\s*)$")
            .unwrap()
    })
}

fn run(params: &Params, emit: &mut Emit) {
    let tree = params.tree;
    let mut ignore: HashSet<usize> = HashSet::new();
    for &b in &tree.filter_idx(&["codeFenced", "codeIndented", "htmlFlow"]) {
        let t = tree.get(b);
        for i in t.start_line..=t.end_line {
            ignore.insert(i);
        }
    }
    for (line_index, line) in params.lines.iter().enumerate() {
        let line_number = line_index + 1;
        if ignore.contains(&line_number) {
            continue;
        }
        if let Some(m) = closed_re().captures(line) {
            let left_hash = m.get(1).unwrap().as_str();
            let left_space_len = m.get(2).unwrap().as_str().chars().count();
            let content = m.get(3).unwrap().as_str();
            let right_space_len = m.get(4).unwrap().as_str().chars().count();
            let right_escape = m.get(5).unwrap().as_str();
            let right_hash = m.get(6).unwrap().as_str();
            let trail_space_len = m.get(7).unwrap().as_str().chars().count();
            let left_hash_len = left_hash.chars().count();
            let right_hash_len = right_hash.chars().count();
            let left = left_space_len == 0;
            let right = right_space_len == 0 || !right_escape.is_empty();
            let right_escape_replacement = if right_escape.is_empty() {
                String::new()
            } else {
                format!("{right_escape} ")
            };
            if left || right {
                let line_len = line.chars().count();
                let range = if left {
                    (1, left_hash_len + 1)
                } else {
                    (line_len - trail_space_len - right_hash_len, right_hash_len + 1)
                };
                emit.add_context(
                    line_number,
                    line.trim(),
                    left,
                    right,
                    Some(range),
                    Some(FixInfo {
                        edit_column: Some(1),
                        delete_count: Some(line_len as i64),
                        insert_text: Some(format!(
                            "{left_hash} {content} {right_escape_replacement}{right_hash}"
                        )),
                        ..Default::default()
                    }),
                );
            }
        }
    }
}
