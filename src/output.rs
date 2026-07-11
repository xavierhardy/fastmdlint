//! Output formatting: markdownlint-cli's default text format and `--json`.

use serde_json::{Map, Value};

use crate::linter::LintError;

/// One file's problems.
pub struct FileReport<'a> {
    pub file: &'a str,
    pub errors: &'a [LintError],
}

#[derive(Clone, Copy, PartialEq)]
pub enum OutputFormat {
    Text,
    Json,
}

struct Flat<'a> {
    file: &'a str,
    err: &'a LintError,
}

fn collect<'a>(reports: &'a [FileReport<'a>]) -> Vec<Flat<'a>> {
    let mut v = Vec::new();
    for r in reports {
        for e in r.errors {
            v.push(Flat {
                file: r.file,
                err: e,
            });
        }
    }
    v
}

fn names(err: &LintError) -> String {
    err.rule_names.join("/")
}

fn description(err: &LintError) -> String {
    let mut d = err.rule_description.to_string();
    if let Some(detail) = &err.error_detail {
        d.push_str(&format!(" [{detail}]"));
    }
    if let Some(context) = &err.error_context {
        d.push_str(&format!(" [Context: \"{context}\"]"));
    }
    d
}

/// Render the problems in the requested format. Returns the string exactly as
/// markdownlint-cli would emit it (without the trailing newline that
/// `console.error` adds).
pub fn render(reports: &[FileReport], format: OutputFormat) -> String {
    let mut flat = collect(reports);
    if flat.is_empty() {
        return String::new();
    }
    match format {
        OutputFormat::Text => {
            flat.sort_by(|a, b| {
                a.file
                    .cmp(b.file)
                    .then(a.err.line_number.cmp(&b.err.line_number))
                    .then(names(a.err).cmp(&names(b.err)))
                    .then(description(a.err).cmp(&description(b.err)))
            });
            flat.iter()
                .map(|f| {
                    let column = f.err.error_range.map(|r| r.0).unwrap_or(0);
                    let column_text = if column != 0 {
                        format!(":{column}")
                    } else {
                        String::new()
                    };
                    format!(
                        "{}:{}{} {} {} {}",
                        f.file,
                        f.err.line_number,
                        column_text,
                        f.err.severity.as_str(),
                        names(f.err),
                        description(f.err)
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        }
        OutputFormat::Json => {
            flat.sort_by(|a, b| {
                a.file
                    .cmp(b.file)
                    .then(a.err.line_number.cmp(&b.err.line_number))
                    .then(a.err.rule_description.cmp(b.err.rule_description))
            });
            let arr: Vec<Value> = flat.iter().map(|f| json_entry(f.file, f.err)).collect();
            serde_json::to_string_pretty(&Value::Array(arr)).unwrap()
        }
    }
}

fn json_entry(file: &str, err: &LintError) -> Value {
    let mut m = Map::new();
    m.insert("fileName".into(), Value::String(file.to_string()));
    m.insert("lineNumber".into(), Value::from(err.line_number));
    m.insert(
        "ruleNames".into(),
        Value::Array(
            err.rule_names
                .iter()
                .map(|s| Value::String(s.to_string()))
                .collect(),
        ),
    );
    m.insert(
        "ruleDescription".into(),
        Value::String(err.rule_description.to_string()),
    );
    m.insert(
        "ruleInformation".into(),
        Value::String(err.rule_information.clone()),
    );
    m.insert(
        "errorDetail".into(),
        err.error_detail
            .clone()
            .map(Value::String)
            .unwrap_or(Value::Null),
    );
    m.insert(
        "errorContext".into(),
        err.error_context
            .clone()
            .map(Value::String)
            .unwrap_or(Value::Null),
    );
    m.insert(
        "errorRange".into(),
        match err.error_range {
            Some((a, b)) => Value::Array(vec![Value::from(a), Value::from(b)]),
            None => Value::Null,
        },
    );
    m.insert("fixInfo".into(), json_fix_info(err));
    m.insert(
        "severity".into(),
        Value::String(err.severity.as_str().to_string()),
    );
    Value::Object(m)
}

fn json_fix_info(err: &LintError) -> Value {
    match &err.fix_info {
        None => Value::Null,
        Some(fi) => {
            let mut m = Map::new();
            if let Some(ln) = fi.line_number {
                m.insert("lineNumber".into(), Value::from(ln));
            }
            if let Some(ec) = fi.edit_column {
                m.insert("editColumn".into(), Value::from(ec));
            }
            if let Some(dc) = fi.delete_count {
                m.insert("deleteCount".into(), Value::from(dc));
            }
            if let Some(it) = &fi.insert_text {
                m.insert("insertText".into(), Value::String(it.clone()));
            }
            Value::Object(m)
        }
    }
}
