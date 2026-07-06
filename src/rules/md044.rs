//! MD044 — proper-names.
//!
//! Scans `data`, `codeFlowValue` and `codeTextData` tokens (and, when
//! configured, is a no-op if no names are given). `htmlFlowData`/`htmlTextData`
//! scanning is approximated via `data`.

use super::helpers::ConfigExt;
use super::{Emit, FixInfo, Params, RuleMeta};
use crate::md::Tree;
use regex::Regex;

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD044", "proper-names"],
    description: "Proper names should have the correct capitalization",
    tags: &["spelling"],
    micromark: true,
    run,
};

const IGNORED_CHILD: &[&str] = &["codeFencedFence", "definition", "reference", "resource"];

fn collect(tree: &Tree, kinds: &[&str]) -> Vec<usize> {
    // filterByPredicate over the whole tree, not descending into ignored types.
    let mut out = Vec::new();
    fn walk(tree: &Tree, nodes: &[usize], kinds: &[&str], out: &mut Vec<usize>) {
        for &n in nodes {
            if kinds.contains(&tree.get(n).kind) {
                out.push(n);
            }
            let children: Vec<usize> = tree
                .get(n)
                .children
                .iter()
                .copied()
                .filter(|&c| !IGNORED_CHILD.contains(&tree.get(c).kind))
                .collect();
            walk(tree, &children, kinds, out);
        }
    }
    walk(tree, &tree.roots, kinds, &mut out);
    out
}

fn run(params: &Params, emit: &mut Emit) {
    let mut names: Vec<String> = params
        .config
        .opt_array("names")
        .map(|a| a.iter().filter_map(|v| v.as_str()).map(String::from).collect())
        .unwrap_or_default();
    names.sort_by(|a, b| b.len().cmp(&a.len()).then(a.cmp(b)));
    if names.is_empty() {
        return;
    }
    let include_code = params.config.opt_bool("code_blocks", true);
    let mut scanned = vec!["data"];
    if include_code {
        scanned.push("codeFlowValue");
        scanned.push("codeTextData");
    }
    let tree = params.tree;
    let tokens = collect(tree, &scanned);

    let mut exclusions: Vec<(usize, usize, usize)> = Vec::new(); // line, scol, ecol
    for name in &names {
        let escaped = regex::escape(name);
        let start_pat = if name.chars().next().map(|c| !c.is_alphanumeric() && c != '_').unwrap_or(false) {
            ""
        } else {
            r"\b_*"
        };
        let end_pat = if name.chars().last().map(|c| !c.is_alphanumeric() && c != '_').unwrap_or(false) {
            ""
        } else {
            r"_*\b"
        };
        let re = match Regex::new(&format!("(?i)({start_pat})({escaped}){end_pat}")) {
            Ok(r) => r,
            Err(_) => continue,
        };
        for &tok in &tokens {
            let t = tree.get(tok);
            for caps in re.captures_iter(&t.text) {
                let m = caps.get(0).unwrap();
                let left = caps.get(1).map(|x| x.as_str().chars().count()).unwrap_or(0);
                let name_match = caps.get(2).unwrap().as_str();
                let char_idx = t.text[..m.start()].chars().count();
                let column = t.start_column + char_idx + left;
                let length = name_match.chars().count();
                let line = t.start_line;
                if names.iter().any(|n| n == name_match) {
                    continue;
                }
                let overlaps = exclusions.iter().any(|&(l, s, e)| {
                    l == line && s <= column + length - 1 && column <= e
                });
                if !overlaps {
                    emit.add_detail_if(
                        line,
                        name,
                        name_match,
                        None,
                        None,
                        Some((column, length)),
                        Some(FixInfo {
                            edit_column: Some(column),
                            delete_count: Some(length as i64),
                            insert_text: Some(name.clone()),
                            ..Default::default()
                        }),
                    );
                }
                exclusions.push((line, column, column + length - 1));
            }
        }
    }
}
