//! MD029 — ol-prefix.

use super::helpers::ConfigExt;
use super::{Emit, FixInfo, Params, RuleMeta};

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD029", "ol-prefix"],
    description: "Ordered list item prefix",
    tags: &["ol"],
    micromark: true,
    run,
};

fn style_example(style: &str) -> &'static str {
    match style {
        "one" => "1/1/1",
        "ordered" => "1/2/3",
        "zero" => "0/0/0",
        _ => "",
    }
}

fn item_value(tree: &crate::md::Tree, prefix: usize) -> (usize, i64) {
    let v = tree.descendants_by_type(prefix, &[&["listItemValue"]]);
    match v.first() {
        Some(&i) => {
            let vt = tree.get(i);
            (vt.start_column, vt.text.parse::<i64>().unwrap_or(0))
        }
        None => (tree.get(prefix).start_column, 0),
    }
}

fn run(params: &Params, emit: &mut Emit) {
    let style = params.config.opt_str("style").unwrap_or("").to_string();
    let valid = ["one", "ordered", "zero"];
    let tree = params.tree;
    for &list in &tree.filter_idx(&["listOrdered"]) {
        let prefixes: Vec<usize> = tree
            .get(list)
            .children
            .iter()
            .copied()
            .filter(|&c| tree.get(c).kind == "listItemPrefix")
            .collect();
        let mut expected: i64 = 1;
        let mut incrementing = false;
        if prefixes.len() >= 2 {
            let (_, first) = item_value(tree, prefixes[0]);
            let (_, second) = item_value(tree, prefixes[1]);
            if second != 1 || first == 0 {
                incrementing = true;
                if first == 0 {
                    expected = 0;
                }
            }
        }
        let list_style = if valid.contains(&style.as_str()) {
            style.clone()
        } else if incrementing {
            "ordered".to_string()
        } else {
            "one".to_string()
        };
        if list_style == "zero" {
            expected = 0;
        } else if list_style == "one" {
            expected = 1;
        }
        for &prefix in &prefixes {
            let (column, actual) = item_value(tree, prefix);
            let pt = tree.get(prefix);
            emit.add_detail_if(
                pt.start_line,
                &expected.to_string(),
                &actual.to_string(),
                Some(&format!("Style: {}", style_example(&list_style))),
                None,
                Some((pt.start_column, pt.end_column - pt.start_column)),
                Some(FixInfo {
                    edit_column: Some(column),
                    delete_count: Some(actual.to_string().len() as i64),
                    insert_text: Some(expected.to_string()),
                    ..Default::default()
                }),
            );
            if list_style == "ordered" {
                expected += 1;
            }
        }
    }
}
