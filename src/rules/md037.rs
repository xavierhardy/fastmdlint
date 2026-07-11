//! MD037 — no-space-in-emphasis.

use std::collections::HashMap;

use super::{Emit, FixInfo, Params, RuleMeta};
use regex::Regex;
use std::sync::OnceLock;

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD037", "no-space-in-emphasis"],
    description: "Spaces inside emphasis markers",
    tags: &["whitespace", "emphasis"],
    micromark: true,
    run,
};

const MARKERS: &[&str] = &["_", "__", "___", "*", "**", "***"];

fn start_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\s+\S").unwrap())
}
fn end_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\S\s+$").unwrap())
}

fn run(params: &Params, emit: &mut Emit) {
    let tree = params.tree;
    // Tokens that have at least one direct `data` child, in document order.
    let parents: Vec<usize> = (0..tree.tokens.len())
        .filter(|&i| {
            tree.get(i)
                .children
                .iter()
                .any(|&c| tree.get(c).kind == "data")
        })
        .collect();

    for token in parents {
        let mut by_marker: HashMap<&str, Vec<usize>> = HashMap::new();
        for &m in MARKERS {
            by_marker.insert(m, Vec::new());
        }
        for &child in &tree.get(token).children {
            let ct = tree.get(child);
            if ct.kind == "data"
                && ct.text.chars().count() <= 3
                && !ct.in_html_flow
                && let Some(v) = by_marker.get_mut(ct.text.as_str())
            {
                v.push(child);
            }
        }
        for m in MARKERS {
            let toks = &by_marker[m];
            let mut i = 0;
            while i + 1 < toks.len() {
                let start = tree.get(toks[i]);
                let start_line = &params.lines[start.start_line - 1];
                let start_slice: String = start_line.chars().skip(start.end_column - 1).collect();
                if let Some(mm) = start_re().find(&start_slice) {
                    let space_char = mm.as_str();
                    let count = space_char.chars().count() - 1;
                    let column = start.end_column;
                    emit.add(
                        start.start_line,
                        None,
                        Some(format!("{m}{space_char}")),
                        Some((column, count)),
                        Some(FixInfo {
                            edit_column: Some(column),
                            delete_count: Some(count as i64),
                            ..Default::default()
                        }),
                    );
                }
                let end = tree.get(toks[i + 1]);
                let end_line = &params.lines[end.start_line - 1];
                let end_slice: String = end_line.chars().take(end.start_column - 1).collect();
                if let Some(mm) = end_re().find(&end_slice) {
                    let space_char = mm.as_str();
                    let count = space_char.chars().count() - 1;
                    let column = end.start_column - count;
                    emit.add(
                        end.start_line,
                        None,
                        Some(format!("{space_char}{m}")),
                        Some((column, count)),
                        Some(FixInfo {
                            edit_column: Some(column),
                            delete_count: Some(count as i64),
                            ..Default::default()
                        }),
                    );
                }
                i += 2;
            }
        }
    }
}
