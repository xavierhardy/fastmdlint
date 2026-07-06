//! MD052 — reference-links-images.
//!
//! Note: shortcut references (`[label]`) are not tokenized, so the
//! non-default `shortcut_syntax` option only checks full/collapsed references.

use std::collections::HashSet;

use super::helpers::ConfigExt;
use super::refdata;
use super::{Emit, Params, RuleMeta};

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD052", "reference-links-images"],
    description: "Reference links and images should use a label that is defined",
    tags: &["images", "links"],
    micromark: true,
    run,
};

fn run(params: &Params, emit: &mut Emit) {
    let ignored: HashSet<String> = params
        .config
        .opt_array("ignored_labels")
        .map(|a| a.iter().filter_map(|v| v.as_str()).map(String::from).collect())
        .unwrap_or_else(|| ["x".to_string()].into_iter().collect());
    let tree = params.tree;
    let defs = refdata::definitions(tree);
    for r in refdata::references(tree) {
        if defs.contains_key(&r.label) || ignored.contains(&r.label) {
            continue;
        }
        let line = &params.lines[r.line0];
        let context: String = line.chars().skip(r.col0).take(r.len).collect();
        emit.add(
            r.line0 + 1,
            Some(format!("Missing link or image reference definition: \"{}\"", r.label)),
            Some(context.clone()),
            Some((r.col0 + 1, context.chars().count())),
            None,
        );
    }
}
