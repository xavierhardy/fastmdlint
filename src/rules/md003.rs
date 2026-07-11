//! MD003 — heading-style.

use super::helpers::ConfigExt;
use super::{Emit, Params, RuleMeta};

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD003", "heading-style"],
    description: "Heading style",
    tags: &["headings"],
    micromark: true,
    run,
};

fn run(params: &Params, emit: &mut Emit) {
    let mut style = params.config.opt_str_or("style", "consistent").to_string();
    for &h in &params.tree.filter_idx(&["atxHeading", "setextHeading"]) {
        let style_for_token = params.tree.heading_style(h);
        if style == "consistent" {
            style = style_for_token.to_string();
        }
        if style_for_token != style {
            let h12 = params.tree.heading_level(h) <= 2;
            let setext_with_atx = style == "setext_with_atx"
                && ((h12 && style_for_token == "setext") || (!h12 && style_for_token == "atx"));
            let setext_with_atx_closed = style == "setext_with_atx_closed"
                && ((h12 && style_for_token == "setext")
                    || (!h12 && style_for_token == "atx_closed"));
            if !setext_with_atx && !setext_with_atx_closed {
                let expected = if style == "setext_with_atx" {
                    if h12 { "setext" } else { "atx" }
                } else if style == "setext_with_atx_closed" {
                    if h12 { "setext" } else { "atx_closed" }
                } else {
                    style.as_str()
                };
                emit.add_detail_if(
                    params.tree.get(h).start_line,
                    expected,
                    style_for_token,
                    None,
                    None,
                    None,
                    None,
                );
            }
        }
    }
}
