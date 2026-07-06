//! MD032 — blanks-around-lists.

use super::helpers::is_blank_line;
use super::{Emit, FixInfo, Params, RuleMeta};
use crate::md::tokens::NON_CONTENT_TOKENS;
use crate::md::Tree;

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD032", "blanks-around-lists"],
    description: "Lists should be surrounded by blank lines",
    tags: &["bullet", "ul", "ol", "blank_lines"],
    micromark: true,
    run,
};

fn block_quote_prefix_text(tree: &Tree, prefixes: &[usize], line_number: usize) -> String {
    let joined: String = prefixes
        .iter()
        .filter(|&&p| tree.get(p).start_line == line_number)
        .map(|&p| tree.get(p).text.clone())
        .collect();
    format!("{}\n", joined.trim_end())
}

/// The "visual" end line of a list: the last content line, ignoring trailing
/// blank lines. Our parser already excludes trailing blank lines from a
/// list's `end_line`, and item continuation lines are covered by it, so the
/// list's own `end_line` is the visual end. We still descend to pick up the
/// deepest content end line in case a nested block extends further.
fn visual_end(tree: &Tree, list: usize) -> usize {
    let mut last = tree.get(list).end_line;
    fn walk(tree: &Tree, node: usize, last: &mut usize) {
        for &c in &tree.get(node).children {
            if !NON_CONTENT_TOKENS.contains(&tree.get(c).kind) {
                if tree.get(c).end_line > *last {
                    *last = tree.get(c).end_line;
                }
                walk(tree, c, last);
            }
        }
    }
    walk(tree, list, &mut last);
    last
}

fn run(params: &Params, emit: &mut Emit) {
    let lines = params.lines;
    let tree = params.tree;
    let prefixes = tree.filter_idx(&["blockQuotePrefix", "linePrefix"]);

    let top_level: Vec<usize> = tree
        .filter_idx(&["listOrdered", "listUnordered"])
        .into_iter()
        .filter(|&l| tree.parent_of_type(l, &["listOrdered", "listUnordered"]).is_none())
        .collect();

    for list in top_level {
        let first = tree.get(list).start_line;
        if first >= 2 && !is_blank_line(lines.get(first - 2).map(|s| s.as_str()).unwrap_or("")) {
            emit.add_context(
                first,
                lines[first - 1].trim(),
                false,
                false,
                None,
                Some(FixInfo {
                    insert_text: Some(block_quote_prefix_text(tree, &prefixes, first)),
                    ..Default::default()
                }),
            );
        } else if first == 1 {
            // line above is "" (out of range) -> isBlankLine("") == true -> no error
        }

        let end = visual_end(tree, list);
        let below = lines.get(end).map(|s| s.as_str()).unwrap_or("");
        if !is_blank_line(below) {
            emit.add_context(
                end,
                lines[end - 1].trim(),
                false,
                false,
                None,
                Some(FixInfo {
                    line_number: Some(end + 1),
                    insert_text: Some(block_quote_prefix_text(tree, &prefixes, end)),
                    ..Default::default()
                }),
            );
        }
    }
}
