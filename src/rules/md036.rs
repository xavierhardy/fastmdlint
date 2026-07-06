//! MD036 — no-emphasis-as-heading.

use super::helpers::ConfigExt;
use super::{Emit, Params, RuleMeta};
use crate::md::Tree;
use regex::Regex;

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD036", "no-emphasis-as-heading"],
    description: "Emphasis used instead of a heading",
    tags: &["headings", "emphasis"],
    micromark: true,
    run,
};

const ALL_PUNCTUATION: &str = ".,;:!?。，；：！？";

fn meaningful(tree: &Tree, c: usize) -> bool {
    let t = tree.get(c);
    !(t.kind == "htmlText" || (t.kind == "data" && t.text.trim().is_empty()))
}

fn run(params: &Params, emit: &mut Emit) {
    let punctuation = params.config.opt_str("punctuation").unwrap_or(ALL_PUNCTUATION);
    let re = match Regex::new(&format!("[{}]$", regex::escape(punctuation))) {
        Ok(r) => r,
        Err(_) => return,
    };
    let tree = params.tree;
    // paragraphs directly under content, whose parent content is top-level (or
    // inside a plain htmlFlow), with exactly one meaningful child.
    let paragraphs: Vec<usize> = tree
        .filter_idx_html(&["paragraph"])
        .into_iter()
        .filter(|&p| {
            let parent = tree.get(p).parent;
            let is_content = parent.map(|c| tree.get(c).kind == "content").unwrap_or(false);
            if !is_content {
                return false;
            }
            let grand = parent.and_then(|c| tree.get(c).parent);
            let grand_ok = match grand {
                None => true,
                Some(g) => {
                    tree.get(g).kind == "htmlFlow" && tree.get(g).parent.is_none()
                }
            };
            if !grand_ok {
                return false;
            }
            tree.get(p).children.iter().filter(|&&c| meaningful(tree, c)).count() == 1
        })
        .collect();

    for &para in &paragraphs {
        for &pair in &[("emphasis", "emphasisText"), ("strong", "strongText")] {
            let (etype, ttype) = pair;
            // descendants of the paragraph: emphasis/strong then its text token
            let texts = descendants_path(tree, para, etype, ttype);
            for text in texts {
                let t = tree.get(text);
                if t.children.len() == 1
                    && tree.get(t.children[0]).kind == "data"
                    && !re.is_match(&t.text)
                {
                    emit.add_context(t.start_line, &t.text, false, false, None, None);
                }
            }
        }
    }
}

fn descendants_path(tree: &Tree, root: usize, a: &str, b: &str) -> Vec<usize> {
    let mut out = Vec::new();
    fn walk(tree: &Tree, node: usize, a: &str, b: &str, out: &mut Vec<usize>) {
        for &c in &tree.get(node).children {
            if tree.get(c).kind == a {
                for &d in &tree.get(c).children {
                    if tree.get(d).kind == b {
                        out.push(d);
                    }
                }
            }
            walk(tree, c, a, b, out);
        }
    }
    walk(tree, root, a, b, &mut out);
    out
}
