//! MD055 — table-pipe-style.

use super::helpers::ConfigExt;
use super::{Emit, Params, RuleMeta};
use crate::md::Tree;

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD055", "table-pipe-style"],
    description: "Table pipe style",
    tags: &["table"],
    micromark: true,
    run,
};

fn non_ws_children(tree: &Tree, idx: usize) -> Vec<usize> {
    tree.get(idx)
        .children
        .iter()
        .copied()
        .filter(|&c| !matches!(tree.get(c).kind, "linePrefix" | "whitespace"))
        .collect()
}

fn make_range(start: usize, end: usize) -> (usize, usize) {
    (start, end - start + 1)
}

fn run(params: &Params, emit: &mut Emit) {
    let style = params.config.opt_str_or("style", "consistent").to_string();
    let mut expected_style = style.clone();
    let mut expected_leading = style != "no_leading_or_trailing" && style != "trailing_only";
    let mut expected_trailing = style != "no_leading_or_trailing" && style != "leading_only";
    let tree = params.tree;
    for &row in &tree.filter_idx(&["tableDelimiterRow", "tableRow"]) {
        let cells = &tree.get(row).children;
        if cells.is_empty() {
            continue;
        }
        let first_cell = cells[0];
        let last_cell = *cells.last().unwrap();
        let leading_token = non_ws_children(tree, first_cell);
        let actual_leading = leading_token
            .first()
            .map(|&t| tree.get(t).kind == "tableCellDivider")
            .unwrap_or(false);
        let trailing_token = non_ws_children(tree, last_cell);
        let actual_trailing = trailing_token
            .last()
            .map(|&t| tree.get(t).kind == "tableCellDivider")
            .unwrap_or(false);
        let actual_style = if actual_leading {
            if actual_trailing {
                "leading_and_trailing"
            } else {
                "leading_only"
            }
        } else if actual_trailing {
            "trailing_only"
        } else {
            "no_leading_or_trailing"
        };
        if expected_style == "consistent" {
            expected_style = actual_style.to_string();
            expected_leading = actual_leading;
            expected_trailing = actual_trailing;
        }
        if actual_leading != expected_leading {
            let detail = format!(
                "{} leading pipe",
                if expected_leading {
                    "Missing"
                } else {
                    "Unexpected"
                }
            );
            emit.add_detail_if(
                tree.get(first_cell).start_line,
                &expected_style,
                actual_style,
                Some(&detail),
                None,
                Some(make_range(
                    tree.get(row).start_column,
                    tree.get(first_cell).start_column,
                )),
                None,
            );
        }
        if actual_trailing != expected_trailing {
            let detail = format!(
                "{} trailing pipe",
                if expected_trailing {
                    "Missing"
                } else {
                    "Unexpected"
                }
            );
            emit.add_detail_if(
                tree.get(last_cell).end_line,
                &expected_style,
                actual_style,
                Some(&detail),
                None,
                Some(make_range(
                    tree.get(last_cell).end_column - 1,
                    tree.get(row).end_column - 1,
                )),
                None,
            );
        }
    }
}
