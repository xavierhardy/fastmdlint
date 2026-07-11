//! MD059 — descriptive-link-text.

use std::collections::HashSet;

use super::helpers::ConfigExt;
use super::{Emit, Params, RuleMeta};
use regex::Regex;
use std::sync::OnceLock;

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD059", "descriptive-link-text"],
    description: "Link text should be descriptive",
    tags: &["accessibility", "links"],
    micromark: true,
    run,
};

const DEFAULT_PROHIBITED: &[&str] = &["click here", "here", "link", "more"];

fn normalize(s: &str) -> String {
    let non_word = {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| Regex::new(r"[\W_]+").unwrap())
    };
    let ws = {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| Regex::new(r"\s+").unwrap())
    };
    let step1 = non_word.replace_all(s, " ");
    let step2 = ws.replace_all(&step1, " ");
    step2.to_lowercase().trim().to_string()
}

fn run(params: &Params, emit: &mut Emit) {
    let prohibited: HashSet<String> = match params.config.opt_array("prohibited_texts") {
        Some(a) => a.iter().filter_map(|v| v.as_str()).map(normalize).collect(),
        None => DEFAULT_PROHIBITED.iter().map(|s| normalize(s)).collect(),
    };
    if prohibited.is_empty() {
        return;
    }
    let tree = params.tree;
    for &link in &tree.filter_idx(&["link"]) {
        for lt in tree.descendants_by_type(link, &[&["label"], &["labelText"]]) {
            let t = tree.get(lt);
            let has_allowed_child = t
                .children
                .iter()
                .any(|&c| matches!(tree.get(c).kind, "codeText" | "htmlText"));
            if !has_allowed_child && prohibited.contains(&normalize(&t.text)) {
                let parent_text = t
                    .parent
                    .map(|p| tree.get(p).text.clone())
                    .unwrap_or_default();
                let range = if t.start_line == t.end_line {
                    Some((t.start_column, t.end_column - t.start_column))
                } else {
                    None
                };
                emit.add_context(t.start_line, &parent_text, false, false, range, None);
            }
        }
    }
}
