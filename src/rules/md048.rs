//! MD048 — code-fence-style.

use super::helpers::ConfigExt;
use super::{Emit, Params, RuleMeta};

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD048", "code-fence-style"],
    description: "Code fence style",
    tags: &["code"],
    micromark: true,
    run,
};

fn style_for(text: &str) -> &'static str {
    if text.starts_with('~') {
        "tilde"
    } else {
        "backtick"
    }
}

fn run(params: &Params, emit: &mut Emit) {
    let mut expected = params.config.opt_str_or("style", "consistent").to_string();
    let tree = params.tree;
    for &cf in &tree.filter_idx(&["codeFenced"]) {
        let seq = tree
            .descendants_by_type(cf, &[&["codeFencedFence"], &["codeFencedFenceSequence"]])
            .first()
            .copied();
        let Some(seq) = seq else { continue };
        let tok = tree.get(seq);
        if expected == "consistent" {
            expected = style_for(&tok.text).to_string();
        }
        emit.add_detail_if(
            tok.start_line,
            &expected,
            style_for(&tok.text),
            None,
            None,
            None,
            None,
        );
    }
}
