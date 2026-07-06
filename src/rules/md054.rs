//! MD054 — link-image-style.
//!
//! Shortcut reference links (`[label]` with a matching definition) are
//! tokenized as links with only a label child, so the `shortcut` option is
//! enforced like every other style.

use super::refdata;
use super::{Emit, FixInfo, Params, RuleMeta};
use regex::Regex;
use std::sync::OnceLock;

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD054", "link-image-style"],
    description: "Link and image style",
    tags: &["images", "links"],
    micromark: true,
    run,
};

fn autolink_able(dest: &str) -> bool {
    let scheme = {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| Regex::new(r"^[A-Za-z][A-Za-z0-9+.\-]*:").unwrap())
    };
    scheme.is_match(dest) && !dest.contains([' ', '<', '>'])
}

fn opt(cfg: &serde_json::Value, key: &str) -> bool {
    match cfg.get(key) {
        None => true,
        Some(v) => !matches!(v, serde_json::Value::Bool(false)),
    }
}

fn run(params: &Params, emit: &mut Emit) {
    let cfg = params.config;
    let autolink = opt(cfg, "autolink");
    let inline = opt(cfg, "inline");
    let full = opt(cfg, "full");
    let collapsed = opt(cfg, "collapsed");
    let shortcut = opt(cfg, "shortcut");
    let url_inline = opt(cfg, "url_inline");
    if autolink && inline && full && collapsed && shortcut && url_inline {
        return;
    }
    let tree = params.tree;
    let defs = refdata::definitions(tree);

    for &link in &tree.filter_idx(&["autolink", "image", "link"]) {
        let lt = tree.get(link);
        let image = lt.kind == "image";
        let is_error;
        #[allow(unused_assignments)]
        let mut destination = String::new();
        let mut label = String::new();
        if lt.kind == "autolink" {
            destination = tree
                .descendants_by_type(link, &[&["autolinkEmail", "autolinkProtocol"]])
                .first()
                .map(|&d| tree.get(d).text.clone())
                .unwrap_or_default();
            is_error = !autolink && !destination.is_empty();
        } else {
            label = tree
                .descendants_by_type(link, &[&["label"], &["labelText"]])
                .first()
                .map(|&d| tree.get(d).text.clone())
                .unwrap_or_default();
            let dest = tree
                .descendants_by_type(
                    link,
                    &[
                        &["resource"],
                        &["resourceDestination"],
                        &["resourceDestinationLiteral", "resourceDestinationRaw"],
                        &["resourceDestinationString"],
                    ],
                )
                .first()
                .map(|&d| tree.get(d).text.clone());
            if let Some(d) = dest {
                destination = d;
                let title = !tree
                    .descendants_by_type(link, &[&["resource"], &["resourceTitle"], &["resourceTitleString"]])
                    .is_empty();
                is_error = !inline
                    || (!url_inline
                        && autolink
                        && !image
                        && !title
                        && label == destination
                        && autolink_able(&destination));
            } else {
                let reference = tree.descendants_by_type(link, &[&["reference"]]);
                let is_shortcut = reference.is_empty();
                let ref_string = tree
                    .descendants_by_type(link, &[&["reference"], &["referenceString"]])
                    .first()
                    .map(|&d| tree.get(d).text.clone());
                let is_collapsed = ref_string.is_none();
                let key = refdata::normalize(&ref_string.unwrap_or_else(|| label.clone()));
                destination = defs.get(&key).cloned().unwrap_or_default();
                is_error = !destination.is_empty()
                    && (if is_shortcut {
                        !shortcut
                    } else if is_collapsed {
                        !collapsed
                    } else {
                        !full
                    });
            }
        }
        if is_error {
            let (range, fix) = if lt.start_line == lt.end_line {
                let r = (lt.start_column, lt.end_column - lt.start_column);
                let insert = build_insert(inline, autolink, url_inline, image, &label, &destination);
                let fix = insert.map(|it| FixInfo {
                    edit_column: Some(r.0),
                    insert_text: Some(it),
                    delete_count: Some(r.1 as i64),
                    ..Default::default()
                });
                (Some(r), fix)
            } else {
                (None, None)
            };
            let ctx = lt.text.split(['\r', '\n']).next().unwrap_or("").to_string();
            emit.add_context(lt.start_line, &ctx, false, false, range, fix);
        }
    }
}

fn build_insert(
    inline: bool,
    autolink: bool,
    url_inline: bool,
    image: bool,
    label: &str,
    destination: &str,
) -> Option<String> {
    let can_inline = inline && !label.is_empty();
    let can_autolink = autolink && !image && autolink_able(destination);
    if can_inline && (url_inline || !can_autolink) {
        let prefix = if image { "!" } else { "" };
        let escaped_label = escape_re(label, r"[\[\]]");
        let escaped_dest = escape_re(destination, r"[()]");
        Some(format!("{prefix}[{escaped_label}]({escaped_dest})"))
    } else if can_autolink {
        Some(format!("<{}>", remove_backslash(destination)))
    } else {
        None
    }
}

fn escape_re(s: &str, class: &str) -> String {
    let re = Regex::new(class).unwrap();
    re.replace_all(s, |c: &regex::Captures| format!("\\{}", &c[0])).to_string()
}

fn remove_backslash(s: &str) -> String {
    let re = {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| Regex::new(r#"\\([!-/:-@\[-`{-~])"#).unwrap())
    };
    re.replace_all(s, "$1").to_string()
}
