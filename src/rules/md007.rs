//! MD007 — ul-indent.
//!
//! Note: the blockquote-indent adjustment and gfmFootnoteDefinition base
//! indent from upstream are not modelled (lists inside block quotes/footnotes
//! may diverge); plain nested unordered lists match.

use super::helpers::ConfigExt;
use super::{Emit, FixInfo, Params, RuleMeta};

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD007", "ul-indent"],
    description: "Unordered list indentation",
    tags: &["bullet", "ul", "indentation"],
    micromark: true,
    run,
};

fn run(params: &Params, emit: &mut Emit) {
    let indent = params.config.opt_i64("indent", 2);
    let start_indented = params.config.opt_bool("start_indented", false);
    let start_indent = params.config.opt_i64("start_indent", indent);
    let tree = params.tree;

    for &list in &tree.filter_idx(&["listUnordered"]) {
        // Compute nesting depth by walking unordered-list ancestors.
        let mut nesting: i64 = 0;
        let mut current = list;
        loop {
            let Some(p) = tree.parent_of_type(current, &["blockQuote", "listOrdered", "listUnordered"])
            else {
                break;
            };
            match tree.get(p).kind {
                "listUnordered" => {
                    nesting += 1;
                    current = p;
                    continue;
                }
                "listOrdered" => {
                    nesting = -1;
                    break;
                }
                _ => break, // blockQuote
            }
        }
        if nesting < 0 {
            continue;
        }
        let prefixes: Vec<usize> = tree
            .get(list)
            .children
            .iter()
            .copied()
            .filter(|&c| tree.get(c).kind == "listItemPrefix")
            .collect();
        for &prefix in &prefixes {
            let pt = tree.get(prefix);
            let expected_indent =
                (if start_indented { start_indent } else { 0 }) + nesting * indent;
            let actual_indent = pt.start_column as i64 - 1;
            let range = (1, pt.end_column - 1);
            emit.add_detail_if(
                pt.start_line,
                &expected_indent.to_string(),
                &actual_indent.to_string(),
                None,
                None,
                Some(range),
                Some(FixInfo {
                    edit_column: Some((pt.start_column as i64 - actual_indent) as usize),
                    delete_count: Some((actual_indent - expected_indent).max(0)),
                    insert_text: Some(" ".repeat((expected_indent - actual_indent).max(0) as usize)),
                    ..Default::default()
                }),
            );
        }
    }
}
