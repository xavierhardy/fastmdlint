//! MD040 — fenced-code-language.

use super::helpers::ConfigExt;
use super::{Emit, Params, RuleMeta};

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD040", "fenced-code-language"],
    description: "Fenced code blocks should have a language specified",
    tags: &["code", "language"],
    micromark: true,
    run,
};

fn run(params: &Params, emit: &mut Emit) {
    let allowed: Vec<String> = params
        .config
        .opt_array("allowed_languages")
        .map(|a| a.iter().filter_map(|v| v.as_str()).map(String::from).collect())
        .unwrap_or_default();
    let language_only = params.config.opt_bool("language_only", false);
    let tree = params.tree;
    for &fc in &tree.filter_idx(&["codeFenced"]) {
        let fence = match tree.descendants_by_type(fc, &[&["codeFencedFence"]]).first().copied() {
            Some(f) => f,
            None => continue,
        };
        let ftok = tree.get(fence);
        let start_line = ftok.start_line;
        let text = ftok.text.clone();
        let info = tree
            .descendants_by_type(fence, &[&["codeFencedFenceInfo"]])
            .first()
            .map(|&i| tree.get(i).text.clone());
        match info {
            None => {
                emit.add_context(start_line, &text, false, false, None, None);
            }
            Some(info) => {
                if !allowed.is_empty() && !allowed.contains(&info) {
                    emit.add(
                        start_line,
                        Some(format!("\"{info}\" is not allowed")),
                        None,
                        None,
                        None,
                    );
                }
            }
        }
        if language_only
            && !tree
                .descendants_by_type(fence, &[&["codeFencedFenceMeta"]])
                .is_empty()
        {
            emit.add(
                start_line,
                Some(format!("Info string contains more than language: \"{text}\"")),
                None,
                None,
                None,
            );
        }
    }
}
