//! MD049 — emphasis-style, and MD050 — strong-style.

use super::helpers::ConfigExt;
use super::{Emit, FixInfo, Params, RuleMeta};

pub const MD049: RuleMeta = RuleMeta {
    names: &["MD049", "emphasis-style"],
    description: "Emphasis style",
    tags: &["emphasis"],
    micromark: true,
    run: run_md049,
};

pub const MD050: RuleMeta = RuleMeta {
    names: &["MD050", "strong-style"],
    description: "Strong style",
    tags: &["emphasis"],
    micromark: true,
    run: run_md050,
};

fn style_for(text: &str) -> &'static str {
    if text.starts_with('*') { "asterisk" } else { "underscore" }
}

fn is_word(c: Option<char>) -> bool {
    c.map(|c| c.is_alphanumeric() || c == '_').unwrap_or(false)
}

#[allow(clippy::too_many_arguments)]
fn impl_rule(
    params: &Params,
    emit: &mut Emit,
    kind: &str,
    seq_kind: &str,
    asterisk: &str,
    underline: &str,
    config_style: &str,
) {
    let mut style = config_style.to_string();
    let tree = params.tree;
    for &token in &tree.filter_idx(&[kind]) {
        let sequences: Vec<usize> = tree
            .get(token)
            .children
            .iter()
            .copied()
            .filter(|&c| tree.get(c).kind == seq_kind)
            .collect();
        let (Some(&start), Some(&end)) = (sequences.first(), sequences.last()) else {
            continue;
        };
        let markup_style = style_for(&tree.get(start).text);
        if style == "consistent" {
            style = markup_style.to_string();
        }
        if style != markup_style {
            let intraword = style == "underscore" && {
                let s = tree.get(start);
                let e = tree.get(end);
                let before = params
                    .lines
                    .get(s.start_line - 1)
                    .and_then(|l| l.chars().nth(s.start_column.wrapping_sub(2)));
                let after = params
                    .lines
                    .get(e.end_line - 1)
                    .and_then(|l| l.chars().nth(e.end_column - 1));
                is_word(before) || is_word(after)
            };
            if !intraword {
                for &seq in &[start, end] {
                    let s = tree.get(seq);
                    emit.add(
                        s.start_line,
                        Some(format!("Expected: {style}; Actual: {markup_style}")),
                        None,
                        Some((s.start_column, s.text.chars().count())),
                        Some(FixInfo {
                            edit_column: Some(s.start_column),
                            delete_count: Some(s.text.chars().count() as i64),
                            insert_text: Some(if style == "asterisk" {
                                asterisk.to_string()
                            } else {
                                underline.to_string()
                            }),
                            ..Default::default()
                        }),
                    );
                }
            }
        }
    }
}

fn run_md049(params: &Params, emit: &mut Emit) {
    let style = params.config.opt_str("style").unwrap_or("consistent");
    let style = if style.is_empty() { "consistent" } else { style };
    impl_rule(params, emit, "emphasis", "emphasisSequence", "*", "_", style);
}

fn run_md050(params: &Params, emit: &mut Emit) {
    let style = params.config.opt_str("style").unwrap_or("consistent");
    let style = if style.is_empty() { "consistent" } else { style };
    impl_rule(params, emit, "strong", "strongSequence", "**", "__", style);
}
