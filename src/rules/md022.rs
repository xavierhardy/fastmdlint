//! MD022 — blanks-around-headings.

use super::helpers::{ConfigExt, is_blank_line};
use super::{Emit, FixInfo, Params, RuleMeta};
use crate::md::Tree;
use serde_json::Value;

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD022", "blanks-around-headings"],
    description: "Headings should be surrounded by blank lines",
    tags: &["headings", "blank_lines"],
    micromark: true,
    run,
};

/// getLinesFunction — returns a closure that maps a heading level to the
/// required number of blank lines.
fn lines_for(config_val: Option<&Value>, level: usize) -> i64 {
    match config_val {
        Some(Value::Array(arr)) => {
            let mut lines = [1i64; 6];
            for (i, v) in arr.iter().take(6).enumerate() {
                lines[i] = v.as_i64().unwrap_or(1);
            }
            lines[level - 1]
        }
        Some(v) => v
            .as_i64()
            .or_else(|| v.as_f64().map(|f| f as i64))
            .unwrap_or(1),
        None => 1,
    }
}

fn block_quote_prefix_text(
    tree: &Tree,
    prefixes: &[usize],
    line_number: usize,
    count: i64,
) -> String {
    if count <= 0 {
        return String::new();
    }
    let joined: String = prefixes
        .iter()
        .filter(|&&p| tree.get(p).start_line == line_number)
        .map(|&p| tree.get(p).text.clone())
        .collect();
    let base = format!("{}\n", joined.trim_end());
    base.repeat(count as usize)
}

fn get_line<'a>(
    lines: &'a [String],
    index: isize,
    front_matter: &'a [String],
    include_front_matter: bool,
) -> &'a str {
    if index >= 0 && (index as usize) < lines.len() {
        return &lines[index as usize];
    }
    if include_front_matter
        && !front_matter.is_empty()
        && index < 0
        && index >= -(front_matter.len() as isize)
    {
        return &front_matter[(front_matter.len() as isize + index) as usize];
    }
    ""
}

fn run(params: &Params, emit: &mut Emit) {
    let above_cfg = params.config.opt("lines_above");
    let below_cfg = params.config.opt("lines_below");
    let include_front_matter = params.config.opt_bool("include_front_matter", false);
    let lines = params.lines;
    let front_matter = params.front_matter_lines;
    let tree = params.tree;
    let prefixes = tree.filter_idx(&["blockQuotePrefix", "linePrefix"]);

    for &h in &tree.filter_idx(&["atxHeading", "setextHeading"]) {
        let start_line = tree.get(h).start_line;
        let end_line = tree.get(h).end_line;
        let level = tree.heading_level(h);
        let line = lines[start_line - 1].trim().to_string();

        let lines_above = lines_for(above_cfg, level);
        if lines_above >= 0 {
            let mut actual_above = 0i64;
            let mut i = 0i64;
            while i < lines_above
                && is_blank_line(get_line(
                    lines,
                    start_line as isize - 2 - i as isize,
                    front_matter,
                    include_front_matter,
                ))
            {
                actual_above += 1;
                i += 1;
            }
            emit.add_detail_if(
                start_line,
                &lines_above.to_string(),
                &actual_above.to_string(),
                Some("Above"),
                Some(line.clone()),
                None,
                Some(FixInfo {
                    insert_text: Some(block_quote_prefix_text(
                        tree,
                        &prefixes,
                        start_line - 1,
                        lines_above - actual_above,
                    )),
                    ..Default::default()
                }),
            );
        }

        let lines_below = lines_for(below_cfg, level);
        if lines_below >= 0 {
            let mut actual_below = 0i64;
            let mut i = 0i64;
            while i < lines_below
                && is_blank_line(get_line(
                    lines,
                    end_line as isize + i as isize,
                    front_matter,
                    false,
                ))
            {
                actual_below += 1;
                i += 1;
            }
            emit.add_detail_if(
                start_line,
                &lines_below.to_string(),
                &actual_below.to_string(),
                Some("Below"),
                Some(line.clone()),
                None,
                Some(FixInfo {
                    line_number: Some(end_line + 1),
                    insert_text: Some(block_quote_prefix_text(
                        tree,
                        &prefixes,
                        end_line + 1,
                        lines_below - actual_below,
                    )),
                    ..Default::default()
                }),
            );
        }
    }
}
