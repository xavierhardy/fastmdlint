//! MD030 — list-marker-space.

use super::helpers::ConfigExt;
use super::{Emit, FixInfo, Params, RuleMeta};

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD030", "list-marker-space"],
    description: "Spaces after list markers",
    tags: &["ol", "ul", "whitespace"],
    micromark: true,
    run,
};

fn run(params: &Params, emit: &mut Emit) {
    let ul_single = params.config.opt_i64("ul_single", 1);
    let ol_single = params.config.opt_i64("ol_single", 1);
    let ul_multi = params.config.opt_i64("ul_multi", 1);
    let ol_multi = params.config.opt_i64("ol_multi", 1);
    let tree = params.tree;
    for &list in &tree.filter_idx(&["listOrdered", "listUnordered"]) {
        let ordered = tree.get(list).kind == "listOrdered";
        let prefixes: Vec<usize> = tree
            .get(list)
            .children
            .iter()
            .copied()
            .filter(|&c| tree.get(c).kind == "listItemPrefix")
            .collect();
        let all_single = (tree.get(list).end_line - tree.get(list).start_line + 1) == prefixes.len();
        let expected = if ordered {
            if all_single { ol_single } else { ol_multi }
        } else if all_single {
            ul_single
        } else {
            ul_multi
        };
        for &prefix in &prefixes {
            let pt = tree.get(prefix);
            let range = (pt.start_column, pt.end_column - pt.start_column);
            let ws: Vec<usize> = tree
                .get(prefix)
                .children
                .iter()
                .copied()
                .filter(|&c| tree.get(c).kind == "listItemPrefixWhitespace")
                .collect();
            for w in ws {
                let wt = tree.get(w);
                let actual = wt.end_column - wt.start_column;
                emit.add_detail_if(
                    wt.start_line,
                    &expected.to_string(),
                    &actual.to_string(),
                    None,
                    None,
                    Some(range),
                    Some(FixInfo {
                        edit_column: Some(wt.start_column),
                        delete_count: Some(actual as i64),
                        insert_text: Some(" ".repeat(expected.max(0) as usize)),
                        ..Default::default()
                    }),
                );
            }
        }
    }
}
