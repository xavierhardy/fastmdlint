//! MD051 — link-fragments.

use std::collections::HashMap;

use super::helpers::ConfigExt;
use super::{Emit, FixInfo, Params, RuleMeta};
use crate::md::tokens::html_tag_info;
use crate::md::Tree;
use regex::Regex;
use std::sync::OnceLock;

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD051", "link-fragments"],
    description: "Link fragments should be valid",
    tags: &["links"],
    micromark: true,
    run,
};

/// JS `encodeURIComponent`: percent-encode UTF-8 bytes except the unreserved
/// set `A-Za-z0-9 - _ . ! ~ * ' ( )`.
fn encode_uri_component(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        let c = b as char;
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '!' | '~' | '*' | '\'' | '(' | ')') {
            out.push(c);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

const INCLUDE: &[&str] = &["characterEscapeValue", "codeTextData", "data", "mathTextData"];
const EXCLUDE: &[&str] = &["image", "reference", "resource"];

fn inline_text(tree: &Tree, heading_text: usize) -> String {
    let mut out = String::new();
    fn walk(tree: &Tree, nodes: &[usize], out: &mut String) {
        for &n in nodes {
            if INCLUDE.contains(&tree.get(n).kind) {
                out.push_str(&tree.get(n).text);
            }
            let children: Vec<usize> = tree
                .get(n)
                .children
                .iter()
                .copied()
                .filter(|&c| !EXCLUDE.contains(&tree.get(c).kind))
                .collect();
            walk(tree, &children, out);
        }
    }
    walk(tree, &tree.get(heading_text).children, &mut out);
    out
}

fn heading_to_fragment(tree: &Tree, heading_text: usize) -> String {
    let strip = {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| Regex::new(r"[^\p{L}\p{M}\p{N}\p{Pc}\- ]").unwrap())
    };
    let text = inline_text(tree, heading_text).to_lowercase();
    let cleaned = strip.replace_all(&text, "");
    let dashed = cleaned.replace(' ', "-");
    format!("#{}", encode_uri_component(&dashed))
}

fn attr_re(name: &str) -> Regex {
    Regex::new(&format!(r#"(?i)\s{name}\s*=\s*['"]?([^'"\s>]*)"#)).unwrap()
}

fn run(params: &Params, emit: &mut Emit) {
    let ignore_case = params.config.opt_bool("ignore_case", false);
    let ignored_pattern = params.config.opt_str("ignored_pattern").unwrap_or("");
    let ignored_re = Regex::new(if ignored_pattern.is_empty() { "^$" } else { ignored_pattern }).ok();
    let line_fragment_re =
        Regex::new(r"^#(?:L\d+(?:C\d+)?-L\d+(?:C\d+)?|L\d+)$").unwrap();
    let anchor_re = Regex::new(r"\{(#[a-z\d]+(?:[-_][a-z\d]+)*)\}").unwrap();
    let tree = params.tree;

    let mut fragments: HashMap<String, usize> = HashMap::new();
    fragments.insert("#top".to_string(), 0);

    for &ht in &tree.filter_idx(&["atxHeadingText", "setextHeadingText"]) {
        let fragment = heading_to_fragment(tree, ht);
        if fragment != "#" {
            let count = *fragments.get(&fragment).unwrap_or(&0);
            if count > 0 {
                fragments.insert(format!("{fragment}-{count}"), 0);
            }
            fragments.insert(fragment.clone(), count + 1);
            for caps in anchor_re.captures_iter(&tree.get(ht).text) {
                let anchor = caps.get(1).unwrap().as_str().to_string();
                fragments.entry(anchor).or_insert(1);
            }
        }
    }

    let id_re = attr_re("id");
    let name_re = attr_re("name");
    for &t in &tree.filter_idx_html(&["htmlText"]) {
        let tok = tree.get(t);
        if let Some(info) = html_tag_info(&tok.text) {
            if !info.close {
                let m = id_re.captures(&tok.text).or_else(|| {
                    if info.name.to_lowercase() == "a" {
                        name_re.captures(&tok.text)
                    } else {
                        None
                    }
                });
                if let Some(c) = m {
                    if let Some(g) = c.get(1) {
                        fragments.insert(format!("#{}", g.as_str()), 0);
                    }
                }
            }
        }
    }

    for &link in &tree.filter_idx(&["link"]) {
        let dests = tree.descendants_by_type(link, &[&["resource"], &["resourceDestination"], &["resourceDestinationRaw"], &["resourceDestinationString"]]);
        for d in dests {
            let dt = tree.get(d);
            let text = dt.text.clone();
            if text.chars().count() <= 1 || !text.starts_with('#') {
                continue;
            }
            let slice_one: String = text.chars().skip(1).collect();
            let encoded = format!("#{}", encode_uri_component(&slice_one));
            let ignored = ignored_re.as_ref().map(|r| r.is_match(&slice_one)).unwrap_or(false);
            if fragments.contains_key(&encoded)
                || line_fragment_re.is_match(&encoded)
                || ignored
            {
                continue;
            }
            let lt = tree.get(link);
            let (context, range) = if lt.start_line == lt.end_line {
                (Some(lt.text.clone()), Some((lt.start_column, lt.end_column - lt.start_column)))
            } else {
                (None, None)
            };
            let text_lower = text.to_lowercase();
            let mixed = fragments.keys().find(|k| k.to_lowercase() == text_lower).cloned();
            if let Some(mixed_key) = mixed {
                if !ignore_case && mixed_key != text {
                    let fix = range.map(|(_, _)| FixInfo {
                        edit_column: Some(dt.start_column),
                        delete_count: Some((dt.end_column - dt.start_column) as i64),
                        insert_text: Some(mixed_key.clone()),
                        ..Default::default()
                    });
                    emit.add(
                        lt.start_line,
                        Some(format!("Expected: {mixed_key}; Actual: {text}")),
                        context,
                        range,
                        fix,
                    );
                }
            } else {
                emit.add(lt.start_line, None, context, range, None);
            }
        }
    }
}
