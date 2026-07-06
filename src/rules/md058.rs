//! MD058 — blanks-around-tables.

use super::helpers::is_blank_line;
use super::{Emit, FixInfo, Params, RuleMeta};
use crate::md::Tree;

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD058", "blanks-around-tables"],
    description: "Tables should be surrounded by blank lines",
    tags: &["table"],
    micromark: true,
    run,
};

fn bq_prefix_text(tree: &Tree, prefixes: &[usize], line_number: usize) -> String {
    let joined: String = prefixes
        .iter()
        .filter(|&&p| tree.get(p).start_line == line_number)
        .map(|&p| tree.get(p).text.clone())
        .collect();
    format!("{}\n", joined.trim_end())
}

fn run(params: &Params, emit: &mut Emit) {
    let lines = params.lines;
    let tree = params.tree;
    let prefixes = tree.filter_idx(&["blockQuotePrefix", "linePrefix"]);
    for &table in &tree.filter_idx(&["table"]) {
        let first = tree.get(table).start_line;
        if first >= 2 && !is_blank_line(lines.get(first - 2).map(|s| s.as_str()).unwrap_or("")) {
            emit.add_context(
                first,
                lines[first - 1].trim(),
                false,
                false,
                None,
                Some(FixInfo {
                    insert_text: Some(bq_prefix_text(tree, &prefixes, first)),
                    ..Default::default()
                }),
            );
        }
        let last = tree.get(table).end_line;
        if !is_blank_line(lines.get(last).map(|s| s.as_str()).unwrap_or("")) {
            emit.add_context(
                last,
                lines[last - 1].trim(),
                false,
                false,
                None,
                Some(FixInfo {
                    line_number: Some(last + 1),
                    insert_text: Some(bq_prefix_text(tree, &prefixes, last)),
                    ..Default::default()
                }),
            );
        }
    }
}
