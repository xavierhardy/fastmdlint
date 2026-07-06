//! Integration tests for fastmdlint's library API. The expected strings were
//! captured from the real markdownlint-cli and are asserted verbatim, so these
//! double as regression guards for output parity.

use fastmdlint::config::Config;
use fastmdlint::linter::lint;
use fastmdlint::output::{render, FileReport, OutputFormat};
use serde_json::json;

/// Lint `content` with the given config JSON and render text output as the CLI
/// would (file name `f.md`).
fn run_text(content: &str, config: serde_json::Value) -> String {
    let cfg = Config::from_value(config).resolve();
    let errors = lint(content, &cfg);
    let reports = vec![FileReport {
        file: "f.md",
        errors: &errors,
    }];
    render(&reports, OutputFormat::Text)
}

fn default_cfg() -> serde_json::Value {
    json!({})
}

#[test]
fn clean_document_has_no_problems() {
    let md = "# Title\n\nSome text.\n";
    assert_eq!(run_text(md, default_cfg()), "");
}

#[test]
fn md047_missing_final_newline() {
    let md = "# Title\n\ntext";
    let out = run_text(md, default_cfg());
    assert!(out.contains("MD047/single-trailing-newline"), "{out}");
    assert!(out.contains("f.md:3"), "{out}");
}

#[test]
fn md018_no_space_after_hash() {
    let md = "#Heading\n";
    let out = run_text(md, json!({ "default": false, "MD018": true }));
    assert_eq!(
        out,
        "f.md:1:1 error MD018/no-missing-space-atx No space after hash on atx style heading [Context: \"#Heading\"]"
    );
}

#[test]
fn md019_multiple_spaces_after_hash() {
    let md = "#  Heading\n";
    let out = run_text(md, json!({ "default": false, "MD019": true }));
    assert_eq!(
        out,
        "f.md:1:3 error MD019/no-multiple-space-atx Multiple spaces after hash on atx style heading [Context: \"#  Heading\"]"
    );
}

#[test]
fn md012_multiple_blank_lines() {
    let md = "a\n\n\nb\n";
    let out = run_text(md, json!({ "default": false, "MD012": true }));
    assert_eq!(
        out,
        "f.md:3 error MD012/no-multiple-blanks Multiple consecutive blank lines [Expected: 1; Actual: 2]"
    );
}

#[test]
fn md003_heading_style_consistent() {
    let md = "# Atx\n\nSetext\n======\n";
    let out = run_text(md, json!({ "default": false, "MD003": true }));
    assert_eq!(
        out,
        "f.md:3 error MD003/heading-style Heading style [Expected: atx; Actual: setext]"
    );
}

#[test]
fn md040_fenced_code_without_language() {
    let md = "```\ncode\n```\n";
    let out = run_text(md, json!({ "default": false, "MD040": true }));
    assert!(out.contains("MD040/fenced-code-language"), "{out}");
}

#[test]
fn config_disable_via_default_false() {
    // Everything off, so a document full of issues yields nothing.
    let md = "#bad\n\n\n\n";
    let out = run_text(md, json!({ "default": false }));
    assert_eq!(out, "");
}

#[test]
fn inline_disable_next_line_suppresses() {
    let md = "<!-- markdownlint-disable-next-line MD018 -->\n#Heading\n";
    let out = run_text(md, json!({ "default": false, "MD018": true }));
    assert_eq!(out, "");
}

#[test]
fn json_output_shape() {
    let md = "#x\n";
    let cfg = Config::from_value(json!({ "default": false, "MD018": true })).resolve();
    let errors = lint(md, &cfg);
    let reports = vec![FileReport {
        file: "f.md",
        errors: &errors,
    }];
    let out = render(&reports, OutputFormat::Json);
    assert!(out.contains("\"ruleNames\": [\n      \"MD018\","), "{out}");
    assert!(out.contains("\"ruleInformation\": \"https://github.com/DavidAnson/markdownlint/blob/v0.41.0/doc/md018.md\""), "{out}");
    assert!(out.contains("\"severity\": \"error\""), "{out}");
}

#[test]
fn severity_warning_from_config() {
    let md = "#x\n";
    let out = run_text(md, json!({ "default": false, "MD018": { "severity": "warning" } }));
    assert!(out.contains(" warning MD018/"), "{out}");
}

#[test]
fn md049_emphasis_style_consistent() {
    let md = "# T\n\nUse *a* and _b_ here.\n";
    let out = run_text(md, json!({ "default": false, "MD049": true }));
    assert!(out.contains("MD049/emphasis-style"), "{out}");
    assert!(out.contains("[Expected: asterisk; Actual: underscore]"), "{out}");
}

#[test]
fn md055_table_pipe_style() {
    let md = "# T\n\n| a | b |\n| - | - |\n| 1 | 2 |\nno pipe | here\n";
    let out = run_text(md, json!({ "default": false, "MD055": true }));
    assert!(out.contains("MD055/table-pipe-style"), "{out}");
}

#[test]
fn md033_inline_html() {
    let md = "# T\n\nA <span>tag</span> here.\n";
    let out = run_text(md, json!({ "default": false, "MD033": true }));
    assert!(out.contains("MD033/no-inline-html"), "{out}");
    assert!(out.contains("[Element: span]"), "{out}");
}

#[test]
fn md052_undefined_reference() {
    let md = "# T\n\nA [text][missing] link.\n";
    let out = run_text(md, json!({ "default": false, "MD052": true }));
    assert!(out.contains("MD052/reference-links-images"), "{out}");
    assert!(out.contains("\"missing\""), "{out}");
}

#[test]
fn md027_blockquote_extra_space() {
    let md = "# T\n\n> ok\n>   extra\n";
    let out = run_text(md, json!({ "default": false, "MD027": true }));
    assert!(out.contains("MD027/no-multiple-space-blockquote"), "{out}");
}

#[test]
fn front_matter_offsets_line_numbers() {
    let md = "---\ntitle: Test\n---\n\n#Heading\n";
    let out = run_text(md, json!({ "default": false, "MD018": true }));
    // The heading is on source line 5 (after 3 front-matter lines + blank).
    assert!(out.contains("f.md:5:1 "), "{out}");
}
