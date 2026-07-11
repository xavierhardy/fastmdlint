//! Shared link/image reference data derived from the token tree: the map of
//! reference-definition labels to destinations. Used by MD054 (and available
//! to other reference rules).

use std::collections::HashMap;
use std::collections::hash_map::Entry;

use crate::md::Tree;
use regex::Regex;
use std::sync::OnceLock;

/// Normalize a reference label (lowercase, trim, collapse whitespace).
pub fn normalize(s: &str) -> String {
    let ws = {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| Regex::new(r"\s+").unwrap())
    };
    ws.replace_all(s.trim(), " ").to_lowercase()
}

/// A reference use: (normalized label, 0-based line, 0-based column, length).
pub struct RefUse {
    pub label: String,
    pub line0: usize,
    pub col0: usize,
    pub len: usize,
    pub shortcut: bool,
}

/// Full/collapsed reference uses from `link`/`image` tokens. Shortcut
/// references are excluded: the parser only tokenizes them when the label is
/// defined, so they can never be an *undefined* reference.
pub fn references(tree: &Tree) -> Vec<RefUse> {
    let mut out = Vec::new();
    for &link in &tree.filter_idx(&["link", "image"]) {
        let has_resource = !tree.descendants_by_type(link, &[&["resource"]]).is_empty();
        if has_resource {
            continue;
        }
        let reference = tree.descendants_by_type(link, &[&["reference"]]);
        if reference.is_empty() {
            continue; // shortcut (defined by construction) — skip
        }
        let ref_string = tree
            .descendants_by_type(link, &[&["reference"], &["referenceString"]])
            .first()
            .map(|&d| tree.get(d).text.clone());
        let label_text = tree
            .descendants_by_type(link, &[&["label"], &["labelText"]])
            .first()
            .map(|&d| tree.get(d).text.clone())
            .unwrap_or_default();
        let label = normalize(&ref_string.filter(|s| !s.is_empty()).unwrap_or(label_text));
        let t = tree.get(link);
        out.push(RefUse {
            label,
            line0: t.start_line - 1,
            col0: t.start_column - 1,
            len: t.text.chars().count(),
            shortcut: false,
        });
    }
    out
}

/// Undefined shortcut reference uses recorded by the parser (`[label]` with
/// no matching definition), for MD052's `shortcut_syntax` option.
pub fn undefined_shortcut_uses(tree: &Tree) -> Vec<RefUse> {
    tree.undefined_shortcuts
        .iter()
        .map(|u| RefUse {
            label: normalize(&u.label),
            line0: u.line - 1,
            col0: u.column - 1,
            len: u.length,
            shortcut: true,
        })
        .collect()
}

/// Definition label -> 0-based line index (first occurrence) and the list of
/// duplicate (label, line) pairs.
pub fn definition_lines(tree: &Tree) -> (HashMap<String, usize>, Vec<(String, usize)>) {
    let re = {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| Regex::new(r"^\s*\[([^\]]*[^\\\]]|)\]:").unwrap())
    };
    let mut map = HashMap::new();
    let mut dups = Vec::new();
    for &d in &tree.filter_idx(&["definition"]) {
        let t = tree.get(d);
        if let Some(c) = re.captures(&t.text) {
            let label = normalize(c.get(1).map(|m| m.as_str()).unwrap_or(""));
            match map.entry(label) {
                Entry::Occupied(e) => dups.push((e.key().clone(), t.start_line - 1)),
                Entry::Vacant(e) => {
                    e.insert(t.start_line - 1);
                }
            }
        }
    }
    (map, dups)
}

/// Map of normalized definition label -> destination string, parsed from the
/// opaque `definition` tokens.
pub fn definitions(tree: &Tree) -> HashMap<String, String> {
    let re = {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| Regex::new(r"^\s*\[([^\]]*[^\\\]]|)\]:\s*(\S+)").unwrap())
    };
    let mut map = HashMap::new();
    for &d in &tree.filter_idx(&["definition"]) {
        let text = &tree.get(d).text;
        if let Some(c) = re.captures(text) {
            let label = normalize(c.get(1).map(|m| m.as_str()).unwrap_or(""));
            let dest = c.get(2).map(|m| m.as_str().to_string()).unwrap_or_default();
            map.entry(label).or_insert(dest);
        }
    }
    map
}
