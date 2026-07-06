//! MD028 — no-blanks-blockquote.

use super::{Emit, Params, RuleMeta};

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD028", "no-blanks-blockquote"],
    description: "Blank line inside blockquote",
    tags: &["blockquote", "whitespace"],
    micromark: true,
    run,
};

const IGNORE: &[&str] = &["lineEnding", "listItemIndent", "linePrefix"];

fn run(params: &Params, emit: &mut Emit) {
    let tree = params.tree;
    for &bq in &tree.filter_idx(&["blockQuote"]) {
        let siblings: Vec<usize> = match tree.get(bq).parent {
            Some(p) => tree.get(p).children.clone(),
            None => tree.roots.clone(),
        };
        let pos = match siblings.iter().position(|&s| s == bq) {
            Some(p) => p,
            None => continue,
        };
        let mut error_lines: Vec<usize> = Vec::new();
        for &sib in siblings.iter().skip(pos + 1) {
            let t = tree.get(sib);
            if t.kind == "lineEndingBlank" {
                error_lines.push(t.start_line);
            } else if IGNORE.contains(&t.kind) {
                // ignore invisible formatting
            } else if t.kind == "blockQuote" {
                for ln in &error_lines {
                    emit.add(*ln, None, None, None, None);
                }
                break;
            } else {
                break;
            }
        }
    }
}
