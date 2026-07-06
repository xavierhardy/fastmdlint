//! MD012 — no-multiple-blanks.

use std::collections::HashSet;

use super::helpers::ConfigExt;
use super::{Emit, FixInfo, Params, RuleMeta};

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD012", "no-multiple-blanks"],
    description: "Multiple consecutive blank lines",
    tags: &["whitespace", "blank_lines"],
    micromark: true,
    run,
};

fn run(params: &Params, emit: &mut Emit) {
    let maximum = params.config.opt_i64("maximum", 1);
    let tree = params.tree;
    let mut code_lines: HashSet<usize> = HashSet::new();
    for &c in &tree.filter_idx(&["codeFenced", "codeIndented"]) {
        let t = tree.get(c);
        for i in t.start_line..=t.end_line {
            code_lines.insert(i);
        }
    }
    let mut count: i64 = 0;
    for (line_index, line) in params.lines.iter().enumerate() {
        let in_code = code_lines.contains(&(line_index + 1));
        count = if in_code || !line.trim().is_empty() {
            0
        } else {
            count + 1
        };
        if maximum < count {
            emit.add_detail_if(
                line_index + 1,
                &maximum.to_string(),
                &count.to_string(),
                None,
                None,
                None,
                Some(FixInfo {
                    delete_count: Some(-1),
                    ..Default::default()
                }),
            );
        }
    }
}
