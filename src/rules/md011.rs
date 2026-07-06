//! MD011 — no-reversed-links.

use std::collections::HashSet;

use super::{Emit, FixInfo, Params, RuleMeta};
use regex::Regex;
use std::sync::OnceLock;

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD011", "no-reversed-links"],
    description: "Reversed link syntax",
    tags: &["links"],
    micromark: true,
    run,
};

fn reversed_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // (^|[^\\])\(([^()]+)\)\[([^\]^][^\]]*)\](?!\()
    RE.get_or_init(|| Regex::new(r"(^|[^\\])\(([^()]+)\)\[([^\]^][^\]]*)\]").unwrap())
}

struct Range {
    line: usize,
    scol: usize,
    ecol: usize,
}

fn overlaps(a: &Range, b: &Range) -> bool {
    if a.line != b.line {
        return false;
    }
    a.scol <= b.ecol && b.scol <= a.ecol
}

fn run(params: &Params, emit: &mut Emit) {
    let tree = params.tree;
    let mut ignore_lines: HashSet<usize> = HashSet::new();
    for &b in &tree.filter_idx(&["codeFenced", "codeIndented", "mathFlow"]) {
        let t = tree.get(b);
        for i in t.start_line..=t.end_line {
            ignore_lines.insert(i);
        }
    }
    let ignore_texts: Vec<Range> = tree
        .filter_idx(&["codeText", "mathText"])
        .into_iter()
        .map(|c| {
            let t = tree.get(c);
            Range {
                line: t.start_line,
                scol: t.start_column,
                ecol: t.end_column,
            }
        })
        .collect();

    for (line_index, line) in params.lines.iter().enumerate() {
        let line_number = line_index + 1;
        if ignore_lines.contains(&line_number) {
            continue;
        }
        // Manual scan to emulate the JS global regex with lookahead `(?!\()`.
        let chars: Vec<char> = line.chars().collect();
        let mut search = 0usize;
        while let Some(caps) = reversed_re().captures(&line[byte_index(&chars, search)..]) {
            let m = caps.get(0).unwrap();
            let base = byte_index(&chars, search);
            let match_char_start = char_len(&line[..base + m.start()]);
            let pre = caps.get(1).map(|x| x.as_str()).unwrap_or("");
            let link_text = caps.get(2).unwrap().as_str();
            let link_dest = caps.get(3).unwrap().as_str();
            let full = m.as_str();
            // lookahead (?!\() — the char after full match must not be '('
            let after_idx = base + m.end();
            let next_char = line[after_idx..].chars().next();
            let lookahead_ok = next_char != Some('(');

            let pre_len = pre.chars().count();
            if lookahead_ok && !link_text.ends_with('\\') && !link_dest.ends_with('\\') {
                let column = match_char_start + pre_len + 1;
                let length = full.chars().count() - pre_len;
                let range = Range {
                    line: line_number,
                    scol: column,
                    ecol: column + length - 1,
                };
                if !ignore_texts.iter().any(|it| overlaps(it, &range)) {
                    let reversed: String = full.chars().skip(pre_len).collect();
                    emit.add(
                        line_number,
                        Some(reversed),
                        None,
                        Some((column, length)),
                        Some(FixInfo {
                            edit_column: Some(column),
                            delete_count: Some(length as i64),
                            insert_text: Some(format!("[{link_text}]({link_dest})")),
                            ..Default::default()
                        }),
                    );
                }
            }
            // advance past this match (JS lastIndex = match end)
            let new_search = char_len(&line[..after_idx]);
            if new_search <= search {
                search += 1;
            } else {
                search = new_search;
            }
            if search >= chars.len() {
                break;
            }
        }
    }
}

fn byte_index(chars: &[char], char_idx: usize) -> usize {
    chars.iter().take(char_idx).map(|c| c.len_utf8()).sum()
}

fn char_len(s: &str) -> usize {
    s.chars().count()
}
