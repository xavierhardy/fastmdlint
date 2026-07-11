//! MD046 — code-block-style.

use super::helpers::ConfigExt;
use super::{Emit, Params, RuleMeta};

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD046", "code-block-style"],
    description: "Code block style",
    tags: &["code"],
    micromark: true,
    run,
};

fn style_for(kind: &str) -> &'static str {
    if kind == "codeFenced" {
        "fenced"
    } else {
        "indented"
    }
}

fn run(params: &Params, emit: &mut Emit) {
    let mut expected = params.config.opt_str_or("style", "consistent").to_string();
    for &t in &params.tree.filter_idx(&["codeFenced", "codeIndented"]) {
        let tok = params.tree.get(t);
        if expected == "consistent" {
            expected = style_for(tok.kind).to_string();
        }
        emit.add_detail_if(
            tok.start_line,
            &expected,
            style_for(tok.kind),
            None,
            None,
            None,
            None,
        );
    }
}
