//! MD010 — no-hard-tabs.

use std::collections::HashSet;

use super::helpers::ConfigExt;
use super::{Emit, FixInfo, Params, RuleMeta};

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD010", "no-hard-tabs"],
    description: "Hard tabs",
    tags: &["whitespace", "hard_tab"],
    micromark: true,
    run,
};

struct Range {
    start_line: usize,
    start_col: usize,
    end_line: usize,
    end_col: usize,
}

fn pos_le(al: usize, ac: usize, bl: usize, bc: usize) -> bool {
    al < bl || (al == bl && ac <= bc)
}

fn overlaps(a: &Range, b: &Range) -> bool {
    let lte = pos_le(a.start_line, a.start_col, b.start_line, b.start_col);
    let (first, second) = if lte { (a, b) } else { (b, a) };
    pos_le(
        second.start_line,
        second.start_col,
        first.end_line,
        first.end_col,
    )
}

fn run(params: &Params, emit: &mut Emit) {
    let include_code = params.config.opt_bool("code_blocks", true);
    let ignore_langs: HashSet<String> = params
        .config
        .opt_array("ignore_code_languages")
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_lowercase())
                .collect()
        })
        .unwrap_or_default();
    let spaces_per_tab = params.config.opt("spaces_per_tab");
    let multiplier = match spaces_per_tab {
        Some(v) => (v.as_i64().unwrap_or(1)).max(0) as usize,
        None => 1,
    };
    let tree = params.tree;

    let mut exclusion_types: Vec<&str> = Vec::new();
    if include_code {
        if !ignore_langs.is_empty() {
            exclusion_types.push("codeFenced");
        }
    } else {
        exclusion_types.extend(["codeFenced", "codeIndented", "codeText"]);
    }

    let mut code_ranges: Vec<Range> = Vec::new();
    if !exclusion_types.is_empty() {
        for &tok in &tree.filter_idx(&exclusion_types) {
            let t = tree.get(tok);
            if t.kind == "codeFenced" && !ignore_langs.is_empty() {
                let infos = tree
                    .descendants_by_type(tok, &[&["codeFencedFence"], &["codeFencedFenceInfo"]]);
                let all = !infos.is_empty()
                    && infos
                        .iter()
                        .all(|&i| ignore_langs.contains(&tree.get(i).text.to_lowercase()));
                if !all {
                    continue;
                }
            }
            let code_fenced = t.kind == "codeFenced";
            code_ranges.push(Range {
                start_line: t.start_line + if code_fenced { 1 } else { 0 },
                start_col: if code_fenced { 0 } else { t.start_column },
                end_line: t.end_line - if code_fenced { 1 } else { 0 },
                end_col: if code_fenced {
                    usize::MAX
                } else {
                    t.end_column
                },
            });
        }
    }

    for (line_index, line) in params.lines.iter().enumerate() {
        let line_number = line_index + 1;
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if chars[i] == '\t' {
                let start = i;
                while i < chars.len() && chars[i] == '\t' {
                    i += 1;
                }
                let column = start + 1;
                let length = i - start;
                let range = Range {
                    start_line: line_number,
                    start_col: column,
                    end_line: line_number,
                    end_col: column + length - 1,
                };
                if !code_ranges.iter().any(|c| overlaps(c, &range)) {
                    emit.add(
                        line_number,
                        Some(format!("Column: {column}")),
                        None,
                        Some((column, length)),
                        Some(FixInfo {
                            edit_column: Some(column),
                            delete_count: Some(length as i64),
                            insert_text: Some(" ".repeat(length * multiplier)),
                            ..Default::default()
                        }),
                    );
                }
            } else {
                i += 1;
            }
        }
    }
}
