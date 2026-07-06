//! MD024 — no-duplicate-heading.

use super::helpers::ConfigExt;
use super::{Emit, Params, RuleMeta};

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD024", "no-duplicate-heading"],
    description: "Multiple headings with the same content",
    tags: &["headings"],
    micromark: true,
    run,
};

fn run(params: &Params, emit: &mut Emit) {
    let siblings_only = params.config.opt_bool("siblings_only", false);
    let tree = params.tree;
    let mut known: Vec<Vec<String>> = vec![Vec::new(), Vec::new()];
    let mut last_level = 1usize;
    for &h in &tree.filter_idx(&["atxHeading", "setextHeading"]) {
        let text = tree.heading_text(h);
        let idx = if siblings_only {
            let new_level = tree.heading_level(h);
            while last_level < new_level {
                last_level += 1;
                if known.len() <= last_level {
                    known.resize(last_level + 1, Vec::new());
                }
                known[last_level] = Vec::new();
            }
            while last_level > new_level {
                known[last_level] = Vec::new();
                last_level -= 1;
            }
            new_level
        } else {
            1
        };
        if known.len() <= idx {
            known.resize(idx + 1, Vec::new());
        }
        if known[idx].contains(&text) {
            emit.add_context(tree.get(h).start_line, text.trim(), false, false, None, None);
        } else {
            known[idx].push(text);
        }
    }
}
