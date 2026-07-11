//! MD027 — no-multiple-space-blockquote.

use super::helpers::ConfigExt;
use super::{Emit, FixInfo, Params, RuleMeta};

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD027", "no-multiple-space-blockquote"],
    description: "Multiple spaces after blockquote symbol",
    tags: &["blockquote", "whitespace", "indentation"],
    micromark: true,
    run,
};

const LIST_TYPES: &[&str] = &["listOrdered", "listUnordered"];

fn run(params: &Params, emit: &mut Emit) {
    let include_list_items = params.config.opt_bool("list_items", true);
    let tree = params.tree;
    for &token in &tree.filter_idx(&["linePrefix"]) {
        let parent = tree.get(token).parent;
        let code_indented = parent
            .map(|p| tree.get(p).kind == "codeIndented")
            .unwrap_or(false);
        if code_indented {
            continue;
        }
        let siblings: Vec<usize> = match parent {
            Some(p) => tree.get(p).children.clone(),
            None => tree.roots.clone(),
        };
        let pos = match siblings.iter().position(|&s| s == token) {
            Some(p) => p,
            None => continue,
        };
        let prev_is_bq_prefix = pos > 0 && tree.get(siblings[pos - 1]).kind == "blockQuotePrefix";
        if !prev_is_bq_prefix {
            continue;
        }
        let next_is_list = siblings
            .get(pos + 1)
            .map(|&s| LIST_TYPES.contains(&tree.get(s).kind))
            .unwrap_or(false);
        let in_list = tree.parent_of_type(token, LIST_TYPES).is_some();
        if include_list_items || (!next_is_list && !in_list) {
            let t = tree.get(token);
            let length = t.text.chars().count();
            let line = &params.lines[t.start_line - 1];
            emit.add_context(
                t.start_line,
                line,
                false,
                false,
                Some((t.start_column, length)),
                Some(FixInfo {
                    edit_column: Some(t.start_column),
                    delete_count: Some(length as i64),
                    ..Default::default()
                }),
            );
        }
    }
}
