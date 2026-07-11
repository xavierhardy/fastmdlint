//! MD056 — table-column-count.

use super::{Emit, Params, RuleMeta};

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD056", "table-column-count"],
    description: "Table column count",
    tags: &["table"],
    micromark: true,
    run,
};

fn make_range(start: usize, end: usize) -> (usize, usize) {
    (start, end - start + 1)
}

fn run(params: &Params, emit: &mut Emit) {
    let tree = params.tree;
    let mut expected: usize = 0;
    let mut current_table: Option<usize> = None;
    for &row in &tree.filter_idx(&["tableDelimiterRow", "tableRow"]) {
        let table = tree.parent_of_type(row, &["table"]);
        if current_table != table {
            expected = 0;
            current_table = table;
        }
        let cells: Vec<usize> = tree
            .get(row)
            .children
            .iter()
            .copied()
            .filter(|&c| {
                matches!(
                    tree.get(c).kind,
                    "tableData" | "tableDelimiter" | "tableHeader"
                )
            })
            .collect();
        let actual = cells.len();
        if expected == 0 {
            expected = actual;
        }
        let (detail, range) = if actual < expected {
            (
                Some("Too few cells, row will be missing data".to_string()),
                Some((tree.get(row).end_column - 1, 1)),
            )
        } else if expected < actual {
            (
                Some("Too many cells, extra data will be missing".to_string()),
                Some(make_range(
                    tree.get(cells[expected]).start_column,
                    tree.get(row).end_column - 1,
                )),
            )
        } else {
            (None, None)
        };
        emit.add_detail_if(
            tree.get(row).end_line,
            &expected.to_string(),
            &actual.to_string(),
            detail.as_deref(),
            None,
            range,
            None,
        );
    }
}
