//! MD026 — no-trailing-punctuation.

use super::helpers::ConfigExt;
use super::{Emit, FixInfo, Params, RuleMeta};
use regex::Regex;

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD026", "no-trailing-punctuation"],
    description: "Trailing punctuation in heading",
    tags: &["headings"],
    micromark: true,
    run,
};

const ALL_PUNCTUATION_NO_QUESTION: &str = ".,;:!。，；：！";

fn entity_re() -> &'static Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"&(?:#\d+|#[xX][\da-fA-F]+|[a-zA-Z]{2,31}|blk\d{2}|emsp1[34]|frac\d{2}|sup\d|there4);$",
        )
        .unwrap()
    })
}

fn gemoji_re() -> &'static Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?x):(?:[abmovx]|[-+]1|100|1234|(?:1st|2nd|3rd)_place_medal|8ball|clock\d{1,4}|e-mail|non-potable_water|o2|t-rex|u5272|u5408|u55b6|u6307|u6708|u6709|u6e80|u7121|u7533|u7981|u7a7a|[a-z]{2,15}2?|[a-z]{1,14}(?:_[a-z\d]{1,16})+):$",
        )
        .unwrap()
    })
}

fn default_punctuation_re() -> &'static Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        let class = regex::escape(ALL_PUNCTUATION_NO_QUESTION);
        Regex::new(&format!(r"\s*[{class}]+$")).unwrap()
    })
}

fn run(params: &Params, emit: &mut Emit) {
    let punctuation = params.config.opt_str("punctuation");
    // The default punctuation regex is compiled once; a custom option
    // compiles per run.
    let custom_re = punctuation.map(|p| {
        let class = regex::escape(p);
        Regex::new(&format!(r"\s*[{class}]+$")).unwrap()
    });
    let re: &Regex = custom_re
        .as_ref()
        .unwrap_or_else(|| default_punctuation_re());
    let entity_re = entity_re();
    let gemoji_re = gemoji_re();

    let tree = params.tree;
    for &h in &tree.filter_idx(&["atxHeadingText", "setextHeadingText"]) {
        let t = tree.get(h);
        let text = &t.text;
        if let Some(m) = re.find(text) {
            if entity_re.is_match(text) || gemoji_re.is_match(text) {
                continue;
            }
            let full = m.as_str();
            let length = full.chars().count();
            let column = t.end_column - length;
            emit.add(
                t.end_line,
                Some(format!("Punctuation: '{full}'")),
                None,
                Some((column, length)),
                Some(FixInfo {
                    edit_column: Some(column),
                    delete_count: Some(length as i64),
                    ..Default::default()
                }),
            );
        }
    }
}
