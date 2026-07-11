//! MD007 — ul-indent.

use std::collections::HashMap;

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

    // Mirror the upstream document-order traversal over blockQuotePrefix /
    // listItemPrefix / listUnordered, tracking the last blockquote prefix so
    // its width can be subtracted from a list item's indentation.
    let mut unordered_list_nesting: HashMap<usize, i64> = HashMap::new();
    let mut last_bq_prefix: Option<usize> = None;
    for &token in &tree.filter_idx(&["blockQuotePrefix", "listItemPrefix", "listUnordered"]) {
        let t = tree.get(token);
        match t.kind {
            "blockQuotePrefix" => last_bq_prefix = Some(token),
            "listUnordered" => {
                let mut nesting: i64 = 0;
                let mut current = token;
                while let Some(p) =
                    tree.parent_of_type(current, &["blockQuote", "listOrdered", "listUnordered"])
                {
                    match tree.get(p).kind {
                        "listUnordered" => {
                            nesting += 1;
                            current = p;
                        }
                        "listOrdered" => {
                            nesting = -1;
                            break;
                        }
                        _ => break, // blockQuote
                    }
                }
                if nesting >= 0 {
                    unordered_list_nesting.insert(token, nesting);
                }
            }
            _ => {
                // listItemPrefix
                let Some(parent) = t.parent else { continue };
                let Some(&nesting) = unordered_list_nesting.get(&parent) else {
                    continue;
                };
                let expected_indent =
                    (if start_indented { start_indent } else { 0 }) + nesting * indent;
                let bq_adjustment = match last_bq_prefix {
                    Some(bq) => {
                        let b = tree.get(bq);
                        if b.end_line == t.start_line {
                            b.end_column as i64 - 1
                        } else {
                            0
                        }
                    }
                    None => 0,
                };
                let actual_indent = t.start_column as i64 - 1 - bq_adjustment;
                let range = (1, t.end_column - 1);
                emit.add_detail_if(
                    t.start_line,
                    &expected_indent.to_string(),
                    &actual_indent.to_string(),
                    None,
                    None,
                    Some(range),
                    Some(FixInfo {
                        edit_column: Some((t.start_column as i64 - actual_indent) as usize),
                        delete_count: Some((actual_indent - expected_indent).max(0)),
                        insert_text: Some(
                            " ".repeat((expected_indent - actual_indent).max(0) as usize),
                        ),
                        ..Default::default()
                    }),
                );
            }
        }
    }
}
