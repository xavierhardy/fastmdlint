//! MD043 — required-headings.

use super::helpers::ConfigExt;
use super::{Emit, Params, RuleMeta};

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD043", "required-headings"],
    description: "Required heading structure",
    tags: &["headings"],
    micromark: true,
    run,
};

fn run(params: &Params, emit: &mut Emit) {
    let required: Vec<String> = match params.config.opt_array("headings") {
        Some(a) => a
            .iter()
            .filter_map(|v| v.as_str())
            .map(String::from)
            .collect(),
        None => return, // nothing to check
    };
    if params
        .config
        .get("headings")
        .map(|v| !v.is_array())
        .unwrap_or(true)
    {
        return;
    }
    let match_case = params.config.opt_bool("match_case", false);
    let tree = params.tree;
    let handle = |s: &str| -> String {
        if match_case {
            s.to_string()
        } else {
            s.to_lowercase()
        }
    };
    let mut i = 0usize;
    let mut match_any = false;
    let mut has_error = false;
    let mut any_headings = false;
    let get_expected = |i: &mut usize| -> String {
        let v = required
            .get(*i)
            .cloned()
            .unwrap_or_else(|| "[None]".to_string());
        *i += 1;
        if v.is_empty() {
            "[None]".to_string()
        } else {
            v
        }
    };

    for &h in &tree.filter_idx(&["atxHeading", "setextHeading"]) {
        if has_error {
            break;
        }
        let heading_text = tree.heading_text(h);
        let level = tree.heading_level(h);
        any_headings = true;
        let actual = format!("{} {}", "#".repeat(level), heading_text);
        let expected = get_expected(&mut i);
        if expected == "*" {
            let next_expected = get_expected(&mut i);
            if handle(&next_expected) != handle(&actual) {
                match_any = true;
                i -= 1;
            }
        } else if expected == "+" {
            match_any = true;
        } else if expected == "?" {
            // allow current, match next
        } else if handle(&expected) == handle(&actual) {
            match_any = false;
        } else if match_any {
            i -= 1;
        } else {
            emit.add_detail_if(
                tree.get(h).start_line,
                &expected,
                &actual,
                None,
                None,
                None,
                None,
            );
            has_error = true;
        }
    }

    let extra = required.len() as isize - i as isize;
    if !has_error
        && (extra > 1 || (extra == 1 && required.get(i).map(|s| s != "*").unwrap_or(false)))
        && (any_headings || !required.iter().all(|h| h == "*"))
    {
        if let Some(ctx) = required.get(i) {
            emit.add_context(params.lines.len(), ctx, false, false, None, None);
        }
    }
}
