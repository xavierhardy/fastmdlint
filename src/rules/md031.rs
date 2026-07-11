//! MD031 — blanks-around-fences.

use super::helpers::{ConfigExt, is_blank_line};
use super::{Emit, FixInfo, Params, RuleMeta};
use regex::Regex;
use std::sync::OnceLock;

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD031", "blanks-around-fences"],
    description: "Fenced code blocks should be surrounded by blank lines",
    tags: &["code", "blank_lines"],
    micromark: true,
    run,
};

fn prefix_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(.*?)[`~]").unwrap())
}

fn line_or_empty(lines: &[String], idx: isize) -> &str {
    if idx >= 0 && (idx as usize) < lines.len() {
        &lines[idx as usize]
    } else {
        ""
    }
}

fn add_error(emit: &mut Emit, lines: &[String], line_number: usize, top: bool) {
    let line = &lines[line_number - 1];
    let prefix = prefix_re()
        .captures(line)
        .map(|c| c.get(1).unwrap().as_str().to_string());
    let fix = prefix.map(|p| {
        let insert: String = p.chars().map(|c| if c == '>' { c } else { ' ' }).collect();
        FixInfo {
            line_number: Some(line_number + if top { 0 } else { 1 }),
            insert_text: Some(format!("{}\n", insert.trim_end())),
            ..Default::default()
        }
    });
    emit.add_context(line_number, line.trim(), false, false, None, fix);
}

fn run(params: &Params, emit: &mut Emit) {
    let include_list_items = params.config.opt_bool("list_items", true);
    let lines = params.lines;
    let tree = params.tree;
    for &cb in &tree.filter_idx(&["codeFenced"]) {
        let in_list = tree
            .parent_of_type(cb, &["listOrdered", "listUnordered"])
            .is_some();
        if include_list_items || !in_list {
            let start = tree.get(cb).start_line;
            let end = tree.get(cb).end_line;
            if !is_blank_line(line_or_empty(lines, start as isize - 2)) {
                add_error(emit, lines, start, true);
            }
            if !is_blank_line(line_or_empty(lines, end as isize))
                && !is_blank_line(line_or_empty(lines, end as isize - 1))
            {
                add_error(emit, lines, end, false);
            }
        }
    }
}
