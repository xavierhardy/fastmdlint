//! MD023 — heading-start-left.

use super::{Emit, FixInfo, Params, RuleMeta};

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD023", "heading-start-left"],
    description: "Headings must start at the beginning of the line",
    tags: &["headings", "spaces"],
    micromark: true,
    run,
};

fn run(params: &Params, emit: &mut Emit) {
    let tree = params.tree;
    let headings = tree.filter_idx(&["atxHeading", "linePrefix", "setextHeading"]);
    for i in 0..headings.len().saturating_sub(1) {
        let a = tree.get(headings[i]);
        let b = tree.get(headings[i + 1]);
        if a.kind == "linePrefix" && b.kind != "linePrefix" && a.start_line == b.start_line {
            let length = a.end_column - a.start_column;
            emit.add_context(
                a.start_line,
                &params.lines[a.start_line - 1],
                true,
                false,
                Some((a.start_column, length)),
                Some(FixInfo {
                    edit_column: Some(a.start_column),
                    delete_count: Some(length as i64),
                    ..Default::default()
                }),
            );
        }
    }
}
