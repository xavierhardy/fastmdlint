//! MD038 — no-space-in-code.
//!
//! Note: micromark's `codeTextPadding` tokens are not modelled; the common
//! (unpadded) code spans match.

use super::{Emit, FixInfo, Params, RuleMeta};
use regex::Regex;
use std::sync::OnceLock;

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD038", "no-space-in-code"],
    description: "Spaces inside code span elements",
    tags: &["whitespace", "code"],
    micromark: true,
    run,
};

fn start_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(\s+)(\S)").unwrap())
}
fn end_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(\S)(\s+)$").unwrap())
}

fn run(params: &Params, emit: &mut Emit) {
    let tree = params.tree;
    for &ct in &tree.filter_idx(&["codeText"]) {
        let datas = tree.descendants_by_type(ct, &[&["codeTextData"]]);
        if datas.is_empty() {
            continue;
        }
        let start_data = tree.get(datas[0]);
        let end_data = tree.get(datas[datas.len() - 1]);
        let context = tree.get(ct).text.clone();

        let start_match = start_re().captures(&start_data.text);
        let (start_ws, start_backtick) = match &start_match {
            Some(c) => (
                c.get(1).unwrap().as_str().chars().count(),
                c.get(2).unwrap().as_str() == "`",
            ),
            None => (0, false),
        };
        // No padding tokens modelled -> `!startPadding` is always true.
        let start_count = start_ws.saturating_sub(if start_backtick { 1 } else { 0 });
        let start_spaces = start_count > 0;

        let end_match = end_re().captures(&end_data.text);
        let (end_ws, end_backtick) = match &end_match {
            Some(c) => (
                c.get(2).unwrap().as_str().chars().count(),
                c.get(1).unwrap().as_str() == "`",
            ),
            None => (0, false),
        };
        let end_count = end_ws.saturating_sub(if end_backtick { 1 } else { 0 });
        let end_spaces = end_count > 0;

        if start_spaces {
            let start_column = start_data.start_column;
            let length = start_count;
            emit.add_context(
                start_data.start_line,
                &context,
                true,
                false,
                Some((start_column, length)),
                Some(FixInfo {
                    edit_column: Some(start_column),
                    delete_count: Some(length as i64),
                    ..Default::default()
                }),
            );
        }
        if end_spaces {
            let end_column = end_data.end_column;
            let length = end_count;
            emit.add_context(
                end_data.end_line,
                &context,
                false,
                true,
                Some((end_column - length, length)),
                Some(FixInfo {
                    edit_column: Some(end_column - length),
                    delete_count: Some(length as i64),
                    ..Default::default()
                }),
            );
        }
    }
}
