//! MD034 — no-bare-urls.

use super::{Emit, FixInfo, Params, RuleMeta};

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD034", "no-bare-urls"],
    description: "Bare URL used",
    tags: &["links", "url"],
    micromark: true,
    run,
};

fn run(params: &Params, emit: &mut Emit) {
    let tree = params.tree;
    for &t in &tree.filter_idx(&["literalAutolink"]) {
        let tok = tree.get(t);
        let range = (tok.start_column, tok.end_column - tok.start_column);
        emit.add_context(
            tok.start_line,
            &tok.text,
            false,
            false,
            Some(range),
            Some(FixInfo {
                edit_column: Some(range.0),
                delete_count: Some(range.1 as i64),
                insert_text: Some(format!("<{}>", tok.text)),
                ..Default::default()
            }),
        );
    }
}
