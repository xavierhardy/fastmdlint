//! MD045 — no-alt-text.

use super::{Emit, Params, RuleMeta};
use crate::md::tokens::html_tag_info;
use regex::Regex;
use std::sync::OnceLock;

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD045", "no-alt-text"],
    description: "Images should have alternate text (alt text)",
    tags: &["accessibility", "images"],
    micromark: true,
    run,
};

fn alt_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?i)\salt\s*=\s*['"]?([^'"\s>]*)"#).unwrap())
}
fn aria_hidden_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?i)\saria-hidden\s*=\s*['"]?([^'"\s>]*)"#).unwrap())
}

fn run(params: &Params, emit: &mut Emit) {
    let tree = params.tree;
    // Markdown images.
    for &image in &tree.filter_idx(&["image"]) {
        let label_texts = tree.descendants_by_type(image, &[&["label"], &["labelText"]]);
        let has_empty = label_texts.iter().any(|&lt| tree.get(lt).text.is_empty());
        if has_empty {
            let t = tree.get(image);
            let range = if t.start_line == t.end_line {
                Some((t.start_column, t.end_column - t.start_column))
            } else {
                None
            };
            emit.add(t.start_line, None, None, range, None);
        }
    }
    // HTML images.
    for &ht in &tree.filter_idx_html(&["htmlText"]) {
        let tok = tree.get(ht);
        if let Some(info) = html_tag_info(&tok.text) {
            if !info.close && info.name.to_lowercase() == "img" {
                let has_alt = alt_re().is_match(&tok.text);
                let aria_hidden = aria_hidden_re()
                    .captures(&tok.text)
                    .map(|c| c.get(1).map(|m| m.as_str().to_lowercase()).unwrap_or_default())
                    == Some("true".to_string());
                if !has_alt && !aria_hidden {
                    let len = tok.text.split(['\r', '\n']).next().unwrap_or("").chars().count();
                    emit.add(tok.start_line, None, None, Some((tok.start_column, len)), None);
                }
            }
        }
    }
}
