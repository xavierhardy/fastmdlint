//! MD014 — commands-show-output.

use super::{Emit, FixInfo, Params, RuleMeta};
use regex::Regex;
use std::sync::OnceLock;

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD014", "commands-show-output"],
    description: "Dollar signs used before commands without showing output",
    tags: &["code"],
    micromark: true,
    run,
};

fn dollar_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(\s*)(\$\s+)").unwrap())
}

fn run(params: &Params, emit: &mut Emit) {
    let tree = params.tree;
    for &cb in &tree.filter_idx(&["codeFenced", "codeIndented"]) {
        let values: Vec<usize> = tree
            .get(cb)
            .children
            .iter()
            .copied()
            .filter(|&c| tree.get(c).kind == "codeFlowValue")
            .collect();
        let matches: Vec<(usize, String, usize)> = values
            .iter()
            .filter_map(|&v| {
                let t = tree.get(v);
                dollar_re().captures(&t.text).map(|c| {
                    (v, c.get(1).unwrap().as_str().to_string(), c.get(2).unwrap().as_str().chars().count())
                })
            })
            .collect();
        if !values.is_empty() && matches.len() == values.len() {
            for (v, g1, g2len) in &matches {
                let t = tree.get(*v);
                let column = t.start_column + g1.chars().count();
                let length = *g2len;
                emit.add_context(
                    t.start_line,
                    &t.text,
                    false,
                    false,
                    Some((column, length)),
                    Some(FixInfo {
                        edit_column: Some(column),
                        delete_count: Some(length as i64),
                        ..Default::default()
                    }),
                );
            }
        }
    }
}
