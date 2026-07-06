//! MD033 — no-inline-html.

use super::helpers::ConfigExt;
use super::{Emit, Params, RuleMeta};
use crate::md::tokens::html_tag_info;

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD033", "no-inline-html"],
    description: "Inline HTML",
    tags: &["html"],
    micromark: true,
    run,
};

fn lower_list(params: &Params, key: &str) -> Vec<String> {
    params
        .config
        .opt_array(key)
        .map(|a| a.iter().filter_map(|v| v.as_str()).map(|s| s.to_lowercase()).collect())
        .unwrap_or_default()
}

fn run(params: &Params, emit: &mut Emit) {
    let allowed = lower_list(params, "allowed_elements");
    let table_allowed = if params.config.get("table_allowed_elements").is_some() {
        lower_list(params, "table_allowed_elements")
    } else {
        allowed.clone()
    };
    let tree = params.tree;
    for &t in &tree.filter_idx_html(&["htmlText"]) {
        let tok = tree.get(t);
        if let Some(info) = html_tag_info(&tok.text) {
            if info.close {
                continue;
            }
            let name = info.name.to_lowercase();
            let in_table = tree.parent_of_type(t, &["table"]).is_some();
            if (in_table || !allowed.contains(&name)) && (!in_table || !table_allowed.contains(&name))
            {
                let len = tok.text.split(['\r', '\n']).next().unwrap_or("").chars().count();
                emit.add(
                    tok.start_line,
                    Some(format!("Element: {}", info.name)),
                    None,
                    Some((tok.start_column, len)),
                    None,
                );
            }
        }
    }
}
