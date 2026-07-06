//! MD047 — single-trailing-newline.

use super::helpers::is_blank_line;
use super::{Emit, FixInfo, Params, RuleMeta};

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD047", "single-trailing-newline"],
    description: "Files should end with a single newline character",
    tags: &["blank_lines"],
    micromark: false,
    run,
};

fn run(params: &Params, emit: &mut Emit) {
    let n = params.lines.len();
    if n == 0 {
        return;
    }
    let last = &params.lines[n - 1];
    if !is_blank_line(last) {
        let len = last.chars().count();
        emit.add(
            n,
            None,
            None,
            Some((len, 1)),
            Some(FixInfo {
                insert_text: Some("\n".to_string()),
                edit_column: Some(len + 1),
                ..Default::default()
            }),
        );
    }
}
