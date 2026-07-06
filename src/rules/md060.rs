//! MD060 — table-column-style.

use super::helpers::ConfigExt;
use super::{Emit, FixInfo, Params, RuleMeta};
use crate::md::Tree;
use unicode_width::UnicodeWidthStr;

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD060", "table-column-style"],
    description: "Table column style",
    tags: &["table"],
    micromark: true,
    run,
};

struct Err060 {
    line: usize,
    column: usize,
    detail: String,
    fix: Option<FixInfo>,
}

/// Descendants of `root` of the given kinds, in document order.
fn descendants(tree: &Tree, root: usize, kinds: &[&str]) -> Vec<usize> {
    let mut out = Vec::new();
    fn walk(tree: &Tree, node: usize, kinds: &[&str], out: &mut Vec<usize>) {
        for &c in &tree.get(node).children {
            if kinds.contains(&tree.get(c).kind) {
                out.push(c);
            }
            walk(tree, c, kinds, out);
        }
    }
    walk(tree, root, kinds, &mut out);
    out
}

fn divider_columns(lines: &[String], tree: &Tree, row: usize) -> Vec<(usize, usize)> {
    descendants(tree, row, &["tableCellDivider"])
        .into_iter()
        .map(|d| {
            let dt = tree.get(d);
            let line = &lines[dt.start_line - 1];
            let prefix: String = line.chars().take(dt.start_column - 1).collect();
            (dt.start_column, UnicodeWidthStr::width(prefix.as_str()))
        })
        .collect()
}

fn check_aligned(lines: &[String], tree: &Tree, rows: &[usize], detail: &str) -> Vec<Err060> {
    let mut errs = Vec::new();
    if rows.is_empty() {
        return errs;
    }
    let header_cols = divider_columns(lines, tree, rows[0]);
    for &row in &rows[1..] {
        let mut remaining: Vec<usize> = header_cols.iter().map(|c| c.1).collect();
        for (actual, effective) in divider_columns(lines, tree, row) {
            if !remaining.is_empty() {
                if let Some(pos) = remaining.iter().position(|&e| e == effective) {
                    remaining.remove(pos);
                } else {
                    errs.push(Err060 {
                        line: tree.get(row).start_line,
                        column: actual,
                        detail: detail.to_string(),
                        fix: None,
                    });
                }
            }
        }
    }
    errs
}

fn run(params: &Params, emit: &mut Emit) {
    let style = params.config.opt_str_or("style", "any").to_string();
    let aligned_allowed = style == "any" || style == "aligned";
    let compact_allowed = style == "any" || style == "compact";
    let tight_allowed = style == "any" || style == "tight";
    let aligned_delimiter = params.config.opt_bool("aligned_delimiter", false);
    let lines = params.lines;
    let tree = params.tree;

    for &table in &tree.filter_idx(&["table"]) {
        let rows: Vec<usize> = tree
            .get(table)
            .children
            .iter()
            .flat_map(|&c| {
                // tableHead/tableBody contain the rows
                let k = tree.get(c).kind;
                if k == "tableDelimiterRow" || k == "tableRow" {
                    vec![c]
                } else {
                    descendants(tree, c, &["tableDelimiterRow", "tableRow"])
                }
            })
            .collect();

        let mut errors_aligned = Vec::new();
        if aligned_allowed {
            errors_aligned.extend(check_aligned(
                lines,
                tree,
                &rows,
                "Table pipe does not align with header for style \"aligned\"",
            ));
        }
        let mut errors_compact = Vec::new();
        let mut errors_tight = Vec::new();
        if (compact_allowed || tight_allowed) && !(aligned_allowed && errors_aligned.is_empty()) {
            if aligned_delimiter {
                let sub: Vec<usize> = rows.iter().take(2).copied().collect();
                let e = check_aligned(
                    lines,
                    tree,
                    &sub,
                    "Table pipe does not align with header for option \"aligned_delimiter\"",
                );
                for er in &e {
                    errors_compact.push(Err060 { line: er.line, column: er.column, detail: er.detail.clone(), fix: None });
                    errors_tight.push(Err060 { line: er.line, column: er.column, detail: er.detail.clone(), fix: None });
                }
            }
            for &row in &rows {
                let toks = descendants(tree, row, &["tableCellDivider", "tableContent", "whitespace"]);
                for i in 0..toks.len() {
                    let t = tree.get(toks[i]);
                    if t.kind != "tableCellDivider" {
                        continue;
                    }
                    let (start_line, start_col) = (t.start_line, t.start_column);
                    if i > 0 {
                        let prev = tree.get(toks[i - 1]);
                        if prev.kind == "whitespace" {
                            let plen = prev.text.chars().count();
                            if plen != 1 {
                                errors_compact.push(Err060 {
                                    line: start_line,
                                    column: start_col,
                                    detail: "Table pipe has extra space to the left for style \"compact\"".into(),
                                    fix: Some(FixInfo { edit_column: Some(prev.start_column), delete_count: Some(plen as i64 - 1), ..Default::default() }),
                                });
                            }
                        } else {
                            errors_compact.push(Err060 {
                                line: start_line,
                                column: start_col,
                                detail: "Table pipe is missing space to the left for style \"compact\"".into(),
                                fix: Some(FixInfo { edit_column: Some(prev.end_column), insert_text: Some(" ".into()), ..Default::default() }),
                            });
                        }
                    }
                    if i + 1 < toks.len() {
                        let next = tree.get(toks[i + 1]);
                        let row_end = tree.get(row).end_column;
                        if next.kind == "whitespace" {
                            if next.end_column != row_end {
                                let nlen = next.text.chars().count();
                                if nlen != 1 {
                                    errors_compact.push(Err060 {
                                        line: start_line,
                                        column: start_col,
                                        detail: "Table pipe has extra space to the right for style \"compact\"".into(),
                                        fix: Some(FixInfo { edit_column: Some(next.start_column), delete_count: Some(nlen as i64 - 1), ..Default::default() }),
                                    });
                                }
                                errors_tight.push(Err060 {
                                    line: start_line,
                                    column: start_col,
                                    detail: "Table pipe has space to the right for style \"tight\"".into(),
                                    fix: Some(FixInfo { edit_column: Some(next.start_column), delete_count: Some(nlen as i64), ..Default::default() }),
                                });
                            }
                        } else {
                            errors_compact.push(Err060 {
                                line: start_line,
                                column: start_col,
                                detail: "Table pipe is missing space to the right for style \"compact\"".into(),
                                fix: Some(FixInfo { edit_column: Some(next.start_column), insert_text: Some(" ".into()), ..Default::default() }),
                            });
                        }
                    }
                }
            }
        }

        // Report whichever allowed style has the fewest errors.
        let mut chosen = errors_aligned;
        if compact_allowed && (errors_compact.len() < chosen.len() || !aligned_allowed) {
            chosen = errors_compact;
        }
        if tight_allowed && (errors_tight.len() < chosen.len() || (!aligned_allowed && !compact_allowed)) {
            chosen = errors_tight;
        }
        for e in chosen {
            emit.add(e.line, Some(e.detail), None, Some((e.column, 1)), e.fix);
        }
    }
}
