//! MD035 — hr-style.

use super::helpers::ConfigExt;
use super::{Emit, Params, RuleMeta};

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD035", "hr-style"],
    description: "Horizontal rule style",
    tags: &["hr"],
    micromark: true,
    run,
};

fn run(params: &Params, emit: &mut Emit) {
    let mut style = params
        .config
        .opt_str_or("style", "consistent")
        .trim()
        .to_string();
    for &t in &params.tree.filter_idx(&["thematicBreak"]) {
        let tok = params.tree.get(t);
        if style == "consistent" {
            style = tok.text.clone();
        }
        emit.add_detail_if(tok.start_line, &style, &tok.text, None, None, None, None);
    }
}
