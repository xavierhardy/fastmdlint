//! MD005 — list-indent.

use super::{Emit, FixInfo, Params, RuleMeta};

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD005", "list-indent"],
    description: "Inconsistent indentation for list items at the same level",
    tags: &["bullet", "ul", "indentation"],
    micromark: true,
    run,
};

fn run(params: &Params, emit: &mut Emit) {
    let tree = params.tree;
    for &list in &tree.filter_idx(&["listOrdered", "listUnordered"]) {
        let lt = tree.get(list);
        let expected_indent = lt.start_column as i64 - 1;
        let unordered = lt.kind == "listUnordered";
        let mut expected_end: i64 = 0;
        let mut end_matching = false;
        let prefixes: Vec<usize> = tree
            .get(list)
            .children
            .iter()
            .copied()
            .filter(|&c| tree.get(c).kind == "listItemPrefix")
            .collect();
        for &prefix in &prefixes {
            let pt = tree.get(prefix);
            let line_number = pt.start_line;
            let actual_indent = pt.start_column as i64 - 1;
            let range = (1, pt.end_column - 1);
            if unordered {
                emit.add_detail_if(
                    line_number,
                    &expected_indent.to_string(),
                    &actual_indent.to_string(),
                    None,
                    None,
                    Some(range),
                    None,
                );
            } else {
                let marker_len = pt.text.trim().chars().count() as i64;
                let actual_end = pt.start_column as i64 + marker_len - 1;
                if expected_end == 0 {
                    expected_end = actual_end;
                }
                if expected_indent != actual_indent || end_matching {
                    if expected_end == actual_end {
                        end_matching = true;
                    } else {
                        let detail = if end_matching {
                            format!("Expected: ({expected_end}); Actual: ({actual_end})")
                        } else {
                            format!("Expected: {expected_indent}; Actual: {actual_indent}")
                        };
                        let expected = if end_matching {
                            expected_end - marker_len
                        } else {
                            expected_indent
                        };
                        let actual = if end_matching {
                            actual_end - marker_len
                        } else {
                            actual_indent
                        };
                        emit.add(
                            line_number,
                            Some(detail),
                            None,
                            Some(range),
                            Some(FixInfo {
                                edit_column: Some((actual.min(expected) + 1) as usize),
                                delete_count: Some((actual - expected).max(0)),
                                insert_text: Some(" ".repeat((expected - actual).max(0) as usize)),
                                ..Default::default()
                            }),
                        );
                    }
                }
            }
        }
    }
}
