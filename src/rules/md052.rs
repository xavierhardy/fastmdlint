//! MD052 — reference-links-images.
//!
//! Full/collapsed references with an undefined label are tokenized as links
//! and reported here; undefined shortcut references (`[label]`) are recorded
//! by the parser and reported only when `shortcut_syntax` is enabled, exactly
//! like the reference implementation.
//!
//! Upstream declares this rule `parser: "none"`, so it sees an empty token
//! list — and reports nothing — when no micromark-parser rule is enabled;
//! `micromark: false` plus the linter's need-tokens gate reproduces that.

use std::collections::HashSet;

use super::helpers::ConfigExt;
use super::refdata;
use super::{Emit, Params, RuleMeta};

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD052", "reference-links-images"],
    description: "Reference links and images should use a label that is defined",
    tags: &["images", "links"],
    micromark: false,
    run,
};

fn run(params: &Params, emit: &mut Emit) {
    let shortcut_syntax = params.config.opt_bool("shortcut_syntax", false);
    let ignored: HashSet<String> = params
        .config
        .opt_array("ignored_labels")
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str())
                .map(String::from)
                .collect()
        })
        .unwrap_or_else(|| ["x".to_string()].into_iter().collect());
    let tree = params.tree;
    let defs = refdata::definitions(tree);
    let mut uses = refdata::references(tree);
    if shortcut_syntax {
        uses.extend(refdata::undefined_shortcut_uses(tree));
    }
    for r in uses {
        if defs.contains_key(&r.label) || ignored.contains(&r.label) {
            continue;
        }
        let line = &params.lines[r.line0];
        let context: String = line.chars().skip(r.col0).take(r.len).collect();
        emit.add(
            r.line0 + 1,
            Some(format!(
                "Missing link or image reference definition: \"{}\"",
                r.label
            )),
            Some(context.clone()),
            Some((r.col0 + 1, context.chars().count())),
            None,
        );
    }
}
