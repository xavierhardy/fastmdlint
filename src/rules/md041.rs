//! MD041 — first-line-heading / first-line-h1.

use super::helpers::{ConfigExt, front_matter_has_title, is_html_flow_comment};
use super::{Emit, Params, RuleMeta};
use crate::md::Tree;
use crate::md::tokens::{NON_CONTENT_TOKENS, html_tag_info};

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD041", "first-line-heading", "first-line-h1"],
    description: "First line in a file should be a top-level heading",
    tags: &["headings"],
    micromark: true,
    run,
};

fn html_flow_tag_name(tree: &Tree, idx: usize) -> Option<String> {
    let t = tree.get(idx);
    if t.kind != "htmlFlow" {
        return None;
    }
    // filterByTypes(children, ["htmlText"], true) — include htmlFlow content.
    let html_text = find_html_text(tree, idx)?;
    let info = html_tag_info(&tree.get(html_text).text)?;
    Some(info.name.to_lowercase())
}

fn find_html_text(tree: &Tree, idx: usize) -> Option<usize> {
    for &c in &tree.get(idx).children {
        if tree.get(c).kind == "htmlText" {
            return Some(c);
        }
        if let Some(found) = find_html_text(tree, c) {
            return Some(found);
        }
    }
    None
}

fn run(params: &Params, emit: &mut Emit) {
    let allow_preamble = params.config.opt_bool("allow_preamble", false);
    let level = params.config.opt_i64("level", 1) as usize;
    let tree = params.tree;
    if front_matter_has_title(
        params.front_matter_lines,
        params.config.opt_str("front_matter_title"),
    ) {
        return;
    }
    let mut error_line: usize = 0;
    for &r in &tree.roots {
        let t = tree.get(r);
        if NON_CONTENT_TOKENS.contains(&t.kind) || is_html_flow_comment(tree, r) {
            continue;
        }
        if t.kind == "atxHeading" || t.kind == "setextHeading" {
            if tree.heading_level(r) != level {
                error_line = t.start_line;
            }
            break;
        }
        if let Some(tag) = html_flow_tag_name(tree, r) {
            if is_heading_tag(&tag) {
                if tag != format!("h{level}") {
                    error_line = t.start_line;
                }
                break;
            }
        }
        if !allow_preamble {
            error_line = t.start_line;
            break;
        }
    }
    if error_line > 0 {
        emit.add_context(
            error_line,
            &params.lines[error_line - 1],
            false,
            false,
            None,
            None,
        );
    }
}

fn is_heading_tag(tag: &str) -> bool {
    let b = tag.as_bytes();
    b.len() == 2 && b[0] == b'h' && (b'1'..=b'6').contains(&b[1])
}
