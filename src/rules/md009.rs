//! MD009 — no-trailing-spaces.

use std::collections::HashSet;

use super::helpers::ConfigExt;
use super::{Emit, FixInfo, Params, RuleMeta};

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD009", "no-trailing-spaces"],
    description: "Trailing spaces",
    tags: &["whitespace"],
    micromark: true,
    run,
};

fn add_range(set: &mut HashSet<usize>, start: usize, end: usize) {
    for i in start..=end {
        set.insert(i);
    }
}

fn run(params: &Params, emit: &mut Emit) {
    let br_spaces = params.config.opt_i64("br_spaces", 2);
    let include_code = params.config.opt_bool("code_blocks", false);
    let list_item_empty_lines = params.config.opt_bool("list_item_empty_lines", false);
    let strict = params.config.opt_bool("strict", false);
    let tree = params.tree;

    let mut code_block_lines: HashSet<usize> = HashSet::new();
    if !include_code {
        for &c in &tree.filter_idx(&["codeFenced"]) {
            let t = tree.get(c);
            if t.end_line >= t.start_line + 2 {
                add_range(&mut code_block_lines, t.start_line + 1, t.end_line - 1);
            }
        }
        for &c in &tree.filter_idx(&["codeIndented"]) {
            let t = tree.get(c);
            add_range(&mut code_block_lines, t.start_line, t.end_line);
        }
    }

    let mut list_item_lines: HashSet<usize> = HashSet::new();
    if list_item_empty_lines {
        for &list in &tree.filter_idx(&["listOrdered", "listUnordered"]) {
            let lt = tree.get(list);
            add_range(&mut list_item_lines, lt.start_line, lt.end_line);
            let mut trailing_indent = true;
            for &child in lt.children.iter().rev() {
                match tree.get(child).kind {
                    "content" => trailing_indent = false,
                    "listItemIndent" => {
                        if trailing_indent {
                            list_item_lines.remove(&tree.get(child).start_line);
                        }
                    }
                    "listItemPrefix" => trailing_indent = true,
                    _ => {}
                }
            }
        }
    }

    let mut paragraph_lines: HashSet<usize> = HashSet::new();
    let mut code_inline_lines: HashSet<usize> = HashSet::new();
    if strict {
        for &p in &tree.filter_idx(&["paragraph"]) {
            let t = tree.get(p);
            add_range(
                &mut paragraph_lines,
                t.start_line,
                t.end_line.saturating_sub(1).max(t.start_line),
            );
        }
        for &c in &tree.filter_idx(&["codeText"]) {
            let t = tree.get(c);
            add_range(
                &mut code_inline_lines,
                t.start_line,
                t.end_line.saturating_sub(1).max(t.start_line),
            );
        }
    }

    let expected = if br_spaces < 2 { 0 } else { br_spaces as usize };
    for (line_index, line) in params.lines.iter().enumerate() {
        let line_number = line_index + 1;
        let trailing = line.chars().count() - line.trim_end().chars().count();
        if trailing != 0
            && !code_block_lines.contains(&line_number)
            && !list_item_lines.contains(&line_number)
            && ((expected != trailing)
                || (strict
                    && (!paragraph_lines.contains(&line_number)
                        || code_inline_lines.contains(&line_number))))
        {
            let column = line.chars().count() - trailing + 1;
            let detail = format!(
                "Expected: {}; Actual: {}",
                if expected == 0 {
                    "0".to_string()
                } else {
                    format!("0 or {expected}")
                },
                trailing
            );
            emit.add(
                line_number,
                Some(detail),
                None,
                Some((column, trailing)),
                Some(FixInfo {
                    edit_column: Some(column),
                    delete_count: Some(trailing as i64),
                    ..Default::default()
                }),
            );
        }
    }
}
