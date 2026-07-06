//! MD018 — no-missing-space-atx.

use std::collections::HashSet;

use super::{Emit, FixInfo, Params, RuleMeta};

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD018", "no-missing-space-atx"],
    description: "No space after hash on atx style heading",
    tags: &["headings", "atx", "spaces"],
    micromark: true,
    run,
};

fn run(params: &Params, emit: &mut Emit) {
    let tree = params.tree;
    let mut ignore_lines: HashSet<usize> = HashSet::new();
    for &b in &tree.filter_idx(&["codeFenced", "codeIndented", "htmlFlow"]) {
        let t = tree.get(b);
        for i in t.start_line..=t.end_line {
            ignore_lines.insert(i);
        }
    }
    for (line_index, line) in params.lines.iter().enumerate() {
        let line_number = line_index + 1;
        if ignore_lines.contains(&line_number) {
            continue;
        }
        let chars: Vec<char> = line.chars().collect();
        // /^#+[^# \t]/
        let mut n = 0;
        while n < chars.len() && chars[n] == '#' {
            n += 1;
        }
        if n == 0 || n >= chars.len() {
            continue;
        }
        let after = chars[n];
        if after == '#' || after == ' ' || after == '\t' {
            continue;
        }
        // !/#\s*$/ : does not end with # then optional whitespace
        if ends_hash_ws(&chars) {
            continue;
        }
        if line.starts_with("#\u{fe0f}\u{20e3}") {
            continue;
        }
        emit.add_context(
            line_number,
            line.trim(),
            false,
            false,
            Some((1, n + 1)),
            Some(FixInfo {
                edit_column: Some(n + 1),
                insert_text: Some(" ".to_string()),
                ..Default::default()
            }),
        );
    }
}

/// Mirror of `/#\s*$/.test(line)`.
fn ends_hash_ws(chars: &[char]) -> bool {
    let mut i = chars.len();
    while i > 0 && chars[i - 1].is_whitespace() {
        i -= 1;
    }
    i > 0 && chars[i - 1] == '#'
}
