//! MD039 — no-space-in-links.

use super::{Emit, FixInfo, Params, RuleMeta};
use regex::Regex;
use std::sync::OnceLock;

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD039", "no-space-in-links"],
    description: "Spaces inside link text",
    tags: &["whitespace", "links"],
    micromark: true,
    run,
};

fn start_ws() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^[^\S\r\n]+").unwrap())
}
fn end_ws() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"[^\S\r\n]+$").unwrap())
}

fn add_label_space_error(
    emit: &mut Emit,
    tree: &crate::md::Tree,
    label: usize,
    label_text: usize,
    is_start: bool,
) {
    let lt = tree.get(label_text);
    let m = if is_start {
        start_ws().find(&lt.text)
    } else {
        end_ws().find(&lt.text)
    };
    let range = m.as_ref().map(|mm| {
        let len = mm.as_str().chars().count();
        if is_start {
            (lt.start_column, len)
        } else {
            (lt.end_column - len, len)
        }
    });
    let line = if is_start {
        lt.start_line + if m.is_some() { 0 } else { 1 }
    } else {
        lt.end_line - if m.is_some() { 0 } else { 1 }
    };
    let ctx = collapse_ws(&tree.get(label).text);
    let fix = range.map(|(c, l)| FixInfo {
        edit_column: Some(c),
        delete_count: Some(l as i64),
        ..Default::default()
    });
    emit.add_context(line, &ctx, is_start, !is_start, range, fix);
}

fn collapse_ws(s: &str) -> String {
    let re = {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| Regex::new(r"\s+").unwrap())
    };
    re.replace_all(s, " ").to_string()
}

fn run(params: &Params, emit: &mut Emit) {
    let tree = params.tree;
    for &label in &tree.filter_idx(&["label"]) {
        if tree.get(label).parent.map(|p| tree.get(p).kind) != Some("link") {
            continue;
        }
        let label_texts: Vec<usize> = tree
            .get(label)
            .children
            .iter()
            .copied()
            .filter(|&c| tree.get(c).kind == "labelText")
            .collect();
        for lt in label_texts {
            let text = &tree.get(lt).text;
            if text.trim_start().len() != text.len() {
                add_label_space_error(emit, tree, label, lt, true);
            }
            if text.trim_end().len() != text.len() {
                add_label_space_error(emit, tree, label, lt, false);
            }
        }
    }
}
