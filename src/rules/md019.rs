//! MD019 — no-multiple-space-atx, and MD021 — no-multiple-space-closed-atx.

use super::{Emit, FixInfo, Params, RuleMeta};

pub const MD019: RuleMeta = RuleMeta {
    names: &["MD019", "no-multiple-space-atx"],
    description: "Multiple spaces after hash on atx style heading",
    tags: &["headings", "atx", "spaces"],
    micromark: true,
    run: run_md019,
};

pub const MD021: RuleMeta = RuleMeta {
    names: &["MD021", "no-multiple-space-closed-atx"],
    description: "Multiple spaces inside hashes on closed atx style heading",
    tags: &["headings", "atx_closed", "spaces"],
    micromark: true,
    run: run_md021,
};

/// Mirror of validateHeadingSpaces.
fn validate(params: &Params, emit: &mut Emit, heading: usize, delta: isize) {
    let tree = params.tree;
    let children = &tree.get(heading).children;
    let start_line = tree.get(heading).start_line;
    let text = &tree.get(heading).text;
    let len = children.len() as isize;
    let mut index: isize = if delta > 0 { 0 } else { len - 1 };
    while index >= 0
        && index < len
        && tree.get(children[index as usize]).kind != "atxHeadingSequence"
    {
        index += delta;
    }
    if index < 0 || index >= len {
        return;
    }
    let seq_idx = index + delta;
    if seq_idx < 0 || seq_idx >= len {
        return;
    }
    let seq = children[index as usize];
    let ws = children[seq_idx as usize];
    if tree.get(seq).kind == "atxHeadingSequence"
        && tree.get(ws).kind == "whitespace"
        && tree.get(ws).text.chars().count() > 1
    {
        let wtok = tree.get(ws);
        let column = wtok.start_column + 1;
        let length = wtok.end_column - column;
        emit.add_context(
            start_line,
            text.trim(),
            delta > 0,
            delta < 0,
            Some((column, length)),
            Some(FixInfo {
                edit_column: Some(column),
                delete_count: Some(length as i64),
                ..Default::default()
            }),
        );
    }
}

fn run_md019(params: &Params, emit: &mut Emit) {
    for &h in &params.tree.filter_idx(&["atxHeading"]) {
        if params.tree.heading_style(h) == "atx" {
            validate(params, emit, h, 1);
        }
    }
}

fn run_md021(params: &Params, emit: &mut Emit) {
    for &h in &params.tree.filter_idx(&["atxHeading"]) {
        if params.tree.heading_style(h) == "atx_closed" {
            validate(params, emit, h, 1);
            validate(params, emit, h, -1);
        }
    }
}
