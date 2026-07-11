//! MD004 — ul-style.

use super::helpers::ConfigExt;
use super::{Emit, FixInfo, Params, RuleMeta};

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD004", "ul-style"],
    description: "Unordered list style",
    tags: &["bullet", "ul"],
    micromark: true,
    run,
};

fn marker_to_style(marker: &str) -> &'static str {
    match marker {
        "-" => "dash",
        "+" => "plus",
        _ => "asterisk",
    }
}

fn style_to_marker(style: &str) -> &'static str {
    match style {
        "dash" => "-",
        "plus" => "+",
        _ => "*",
    }
}

fn different_item_style(style: &str) -> &'static str {
    match style {
        "dash" => "plus",
        "plus" => "asterisk",
        _ => "dash",
    }
}

fn run(params: &Params, emit: &mut Emit) {
    let style = params.config.opt_str_or("style", "consistent").to_string();
    let valid = ["asterisk", "consistent", "dash", "plus", "sublist"];
    let mut expected_style = if valid.contains(&style.as_str()) {
        style.clone()
    } else {
        "dash".to_string()
    };
    let tree = params.tree;
    let mut nesting_styles: Vec<String> = Vec::new();
    for &list in &tree.filter_idx(&["listUnordered"]) {
        let mut nesting = 0usize;
        if style == "sublist" {
            let mut cur = list;
            while let Some(p) = tree.parent_of_type(cur, &["listOrdered", "listUnordered"]) {
                nesting += 1;
                cur = p;
            }
        }
        let markers = tree.descendants_by_type(list, &[&["listItemPrefix"], &["listItemMarker"]]);
        for m in markers {
            let mtok = tree.get(m);
            let item_style = marker_to_style(&mtok.text);
            if style == "sublist" {
                if nesting_styles
                    .get(nesting)
                    .map(|s| s.is_empty())
                    .unwrap_or(true)
                {
                    let prev = if nesting > 0 {
                        nesting_styles.get(nesting - 1).cloned().unwrap_or_default()
                    } else {
                        String::new()
                    };
                    let val = if item_style == prev {
                        different_item_style(item_style).to_string()
                    } else {
                        item_style.to_string()
                    };
                    if nesting_styles.len() <= nesting {
                        nesting_styles.resize(nesting + 1, String::new());
                    }
                    nesting_styles[nesting] = val;
                }
                expected_style = nesting_styles[nesting].clone();
            } else if expected_style == "consistent" {
                expected_style = item_style.to_string();
            }
            let column = mtok.start_column;
            let length = mtok.end_column - mtok.start_column;
            emit.add_detail_if(
                mtok.start_line,
                &expected_style,
                item_style,
                None,
                None,
                Some((column, length)),
                Some(FixInfo {
                    edit_column: Some(column),
                    delete_count: Some(length as i64),
                    insert_text: Some(style_to_marker(&expected_style).to_string()),
                    ..Default::default()
                }),
            );
        }
    }
}
