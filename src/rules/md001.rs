//! MD001 — heading-increment.

use super::helpers::{ConfigExt, front_matter_has_title};
use super::{Emit, Params, RuleMeta};

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD001", "heading-increment"],
    description: "Heading levels should only increment by one level at a time",
    tags: &["headings"],
    micromark: true,
    run,
};

fn run(params: &Params, emit: &mut Emit) {
    let has_title = front_matter_has_title(
        params.front_matter_lines,
        params.config.opt_str("front_matter_title"),
    );
    let mut prev_level: i64 = if has_title { 1 } else { i64::MAX };
    for &h in &params.tree.filter_idx(&["atxHeading", "setextHeading"]) {
        let level = params.tree.heading_level(h) as i64;
        if level > prev_level {
            emit.add_detail_if(
                params.tree.get(h).start_line,
                &format!("h{}", prev_level + 1),
                &format!("h{level}"),
                None,
                None,
                None,
                None,
            );
        }
        prev_level = level;
    }
}
