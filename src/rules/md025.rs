//! MD025 — single-title / single-h1.

use super::helpers::{ConfigExt, front_matter_has_title, is_html_flow_comment};
use super::{Emit, Params, RuleMeta};
use crate::md::tokens::NON_CONTENT_TOKENS;

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD025", "single-title", "single-h1"],
    description: "Multiple top-level headings in the same document",
    tags: &["headings"],
    micromark: true,
    run,
};

fn run(params: &Params, emit: &mut Emit) {
    let level = params.config.opt_i64("level", 1) as usize;
    let tree = params.tree;
    // is_docfx_tab omitted (rare); treat as false.
    let matching: Vec<usize> = tree
        .filter_idx(&["atxHeading", "setextHeading"])
        .into_iter()
        .filter(|&h| tree.heading_level(h) == level)
        .collect();
    if matching.is_empty() {
        return;
    }
    let found_front_matter = front_matter_has_title(
        params.front_matter_lines,
        params.config.opt_str("front_matter_title"),
    );
    let mut has_top_level = found_front_matter;
    if !has_top_level {
        // Every top-level token before the first matching heading must be a
        // non-content token or an HTML-flow comment.
        let first = matching[0];
        let stop = tree
            .roots
            .iter()
            .position(|&r| r == first)
            .unwrap_or(tree.roots.len().saturating_sub(1));
        has_top_level = tree.roots.iter().take(stop).all(|&r| {
            NON_CONTENT_TOKENS.contains(&tree.get(r).kind) || is_html_flow_comment(tree, r)
        });
    }
    if has_top_level {
        let skip = if found_front_matter { 0 } else { 1 };
        for &h in matching.iter().skip(skip) {
            let text = tree.heading_text(h);
            emit.add_context(tree.get(h).start_line, &text, false, false, None, None);
        }
    }
}
