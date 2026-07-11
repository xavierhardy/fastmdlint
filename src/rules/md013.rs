//! MD013 — line-length.

use std::collections::HashSet;

use super::helpers::ConfigExt;
use super::{Emit, Params, RuleMeta};
use regex::Regex;
use std::sync::OnceLock;

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD013", "line-length"],
    description: "Line length",
    tags: &["line_length"],
    micromark: true,
    run,
};

fn trailing_word_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\S*$").unwrap())
}

fn not_wrappable_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(?:[#>\s]*\s)?\S*$").unwrap())
}

fn definition_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^ {0,3}\[[^\]]+\]:").unwrap())
}

fn add_range(set: &mut HashSet<usize>, start: usize, end: usize) {
    for i in start..=end {
        set.insert(i);
    }
}

fn run(params: &Params, emit: &mut Emit) {
    let line_length = params.config.opt_i64("line_length", 80);
    let heading_line_length = params.config.opt_i64("heading_line_length", line_length);
    let code_line_length = params.config.opt_i64("code_block_line_length", line_length);
    let strict = params.config.opt_bool("strict", false);
    let stern = params.config.opt_bool("stern", false);
    let include_code = params.config.opt_bool("code_blocks", true);
    let include_tables = params.config.opt_bool("tables", true);
    let include_headings = params.config.opt_bool("headings", true);
    let tree = params.tree;

    let mut heading_lines = HashSet::new();
    for &h in &tree.filter_idx(&["atxHeading", "setextHeading"]) {
        let t = tree.get(h);
        add_range(&mut heading_lines, t.start_line, t.end_line);
    }
    let mut code_lines = HashSet::new();
    for &c in &tree.filter_idx(&["codeFenced", "codeIndented"]) {
        let t = tree.get(c);
        add_range(&mut code_lines, t.start_line, t.end_line);
    }
    let mut table_lines = HashSet::new();
    for &c in &tree.filter_idx(&["table"]) {
        let t = tree.get(c);
        add_range(&mut table_lines, t.start_line, t.end_line);
    }
    // paragraph > data line numbers
    let mut para_data_lines = HashSet::new();
    for &p in &tree.filter_idx(&["paragraph"]) {
        for d in tree.descendants_by_type(p, &[&["data"]]) {
            let t = tree.get(d);
            add_range(&mut para_data_lines, t.start_line, t.end_line);
        }
    }
    // link-only lines: lines with a link/autolink token not covered by
    // paragraph data. (Approximated with the inline tokens we produce.)
    let mut link_lines = HashSet::new();
    for &c in &tree.filter_idx(&["autolink", "image", "link", "literalAutolink"]) {
        let t = tree.get(c);
        add_range(&mut link_lines, t.start_line, t.end_line);
    }
    let mut link_only_lines = HashSet::new();
    for ln in &link_lines {
        if !para_data_lines.contains(ln) {
            link_only_lines.insert(*ln);
        }
    }

    for (line_index, line) in params.lines.iter().enumerate() {
        let line_number = line_index + 1;
        let is_heading = heading_lines.contains(&line_number);
        let in_code = code_lines.contains(&line_number);
        let in_table = table_lines.contains(&line_number);
        let max_length = if in_code {
            code_line_length
        } else if is_heading {
            heading_line_length
        } else {
            line_length
        };
        let is_definition = definition_re().is_match(line);
        // If not strict/stern, the last non-whitespace run may exceed the
        // limit as long as it begins within it.
        let text_len = if strict || stern {
            line.chars().count()
        } else {
            trailing_word_re().replace(line, "#").chars().count()
        };
        let line_len = line.chars().count();
        // Condition kept in upstream's shape rather than clippy's minimal form.
        #[allow(clippy::nonminimal_bool)]
        if max_length > 0
            && (include_code || !in_code)
            && (include_tables || !in_table)
            && (include_headings || !is_heading)
            && !is_definition
            && (strict
                || (!(stern && not_wrappable_re().is_match(line))
                    && !link_only_lines.contains(&line_number)))
            && (text_len as i64 > max_length)
        {
            emit.add_detail_if(
                line_number,
                &max_length.to_string(),
                &line_len.to_string(),
                None,
                None,
                Some((
                    (max_length + 1) as usize,
                    (line_len as i64 - max_length) as usize,
                )),
                None,
            );
        }
    }
}
