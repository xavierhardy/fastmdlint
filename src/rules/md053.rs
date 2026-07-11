//! MD053 — link-image-reference-definitions.
//!
//! Upstream declares this rule `parser: "none"`: it reports nothing when no
//! micromark-parser rule is enabled (empty token list), reproduced here via
//! `micromark: false` and the linter's need-tokens gate.

use std::collections::HashSet;

use super::helpers::{ConfigExt, ellipsify};
use super::refdata;
use super::{Emit, FixInfo, Params, RuleMeta};
use regex::Regex;
use std::sync::OnceLock;

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD053", "link-image-reference-definitions"],
    description: "Link and image reference definitions should be needed",
    tags: &["images", "links"],
    micromark: false,
    run,
};

fn single_line_definition(line: &str) -> bool {
    let re = {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| Regex::new(r"^ {0,3}\[([^\]]*[^\\])\]:").unwrap())
    };
    re.replace(line, "").trim().len() > 0
}

fn run(params: &Params, emit: &mut Emit) {
    let ignored: HashSet<String> = params
        .config
        .opt_array("ignored_definitions")
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str())
                .map(String::from)
                .collect()
        })
        .unwrap_or_else(|| ["//".to_string()].into_iter().collect());
    let tree = params.tree;
    let (defs, dups) = refdata::definition_lines(tree);
    let lines = params.lines;
    // Labels used by any reference: token-derived full/collapsed references,
    // plus a scan of non-definition lines for `[label]` bracket pairs (which
    // catches shortcuts and references the parser leaves untokenized, e.g. on
    // list-continuation or wrapped lines).
    let mut refs: HashSet<String> = refdata::references(tree)
        .into_iter()
        .map(|r| r.label)
        .collect();
    let bracket = {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| Regex::new(r"\[([^\]]+)\]").unwrap())
    };
    let def_line = {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| Regex::new(r"^ {0,3}\[[^\]]+\]:").unwrap())
    };
    for line in lines {
        if def_line.is_match(line) {
            continue;
        }
        for caps in bracket.captures_iter(line) {
            refs.insert(refdata::normalize(&caps[1]));
        }
    }

    let mut defs_sorted: Vec<(&String, &usize)> = defs.iter().collect();
    defs_sorted.sort_by_key(|entry| *entry.1);

    for (label, &line0) in defs_sorted {
        if !ignored.contains(label) && !refs.contains(label) {
            let line = &lines[line0];
            let fix = if single_line_definition(line) {
                Some(FixInfo {
                    delete_count: Some(-1),
                    ..Default::default()
                })
            } else {
                None
            };
            emit.add(
                line0 + 1,
                Some(format!(
                    "Unused link or image reference definition: \"{label}\""
                )),
                Some(ellipsify(line, false, false)),
                Some((1, line.chars().count())),
                fix,
            );
        }
    }
    for (label, line0) in dups {
        if !ignored.contains(&label) {
            let line = &lines[line0];
            let fix = if single_line_definition(line) {
                Some(FixInfo {
                    delete_count: Some(-1),
                    ..Default::default()
                })
            } else {
                None
            };
            emit.add(
                line0 + 1,
                Some(format!(
                    "Duplicate link or image reference definition: \"{label}\""
                )),
                Some(ellipsify(line, false, false)),
                Some((1, line.chars().count())),
                fix,
            );
        }
    }
}
