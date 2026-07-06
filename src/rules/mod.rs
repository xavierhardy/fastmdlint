//! Rule registry and shared rule infrastructure.
//!
//! Each rule is a function that inspects [`Params`] (the front-matter-stripped
//! lines and the parsed token [`Tree`]) and reports [`RawError`]s. The linter
//! attaches rule metadata (names, description, information URL, severity) to
//! produce the final [`crate::linter::LintError`].

use serde_json::Value;

use crate::md::Tree;

pub mod helpers;
pub mod refdata;

// Rule implementation modules.
mod md001;
mod md003;
mod md004;
mod md005;
mod md007;
mod md009;
mod md010;
mod md011;
mod md012;
mod md013;
mod md014;
mod md018;
mod md019;
mod md020;
mod md022;
mod md023;
mod md024;
mod md025;
mod md026;
mod md027;
mod md028;
mod md029;
mod md030;
mod md031;
mod md032;
mod md033;
mod md034;
mod md035;
mod md036;
mod md037;
mod md038;
mod md039;
mod md049;
mod md051;
mod md052;
mod md053;
mod md054;
mod md042;
mod md043;
mod md044;
mod md045;
mod md055;
mod md056;
mod md058;
mod md059;
mod md060;
mod md040;
mod md041;
mod md046;
mod md047;
mod md048;

/// Severity of a reported problem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
}

/// A fix descriptor, mirroring markdownlint's `fixInfo`.
#[derive(Debug, Clone, Default)]
pub struct FixInfo {
    pub line_number: Option<usize>,
    pub edit_column: Option<usize>,
    pub delete_count: Option<i64>,
    pub insert_text: Option<String>,
}

/// A problem as reported by a rule (before rule metadata is attached).
#[derive(Debug, Clone)]
pub struct RawError {
    pub line_number: usize,
    pub detail: Option<String>,
    pub context: Option<String>,
    pub range: Option<(usize, usize)>,
    pub fix_info: Option<FixInfo>,
}

/// Parameters passed to a rule.
pub struct Params<'a> {
    /// Front-matter-stripped, HTML-comment-cleared lines (no line endings).
    pub lines: &'a [String],
    /// Front matter lines (stripped from `lines`).
    pub front_matter_lines: &'a [String],
    /// Parsed token tree (from stripped, un-cleared content).
    pub tree: &'a Tree,
    /// This rule's options object.
    pub config: &'a Value,
}

/// Sink for rule errors, mirroring markdownlint's `onError` helpers.
pub struct Emit {
    pub errors: Vec<RawError>,
}

impl Emit {
    pub fn new() -> Self {
        Emit { errors: Vec::new() }
    }

    /// `addError(onError, lineNumber, detail, context, range, fixInfo)`
    pub fn add(
        &mut self,
        line_number: usize,
        detail: Option<String>,
        context: Option<String>,
        range: Option<(usize, usize)>,
        fix_info: Option<FixInfo>,
    ) {
        self.errors.push(RawError {
            line_number,
            detail,
            context,
            range,
            fix_info,
        });
    }

    /// `addErrorDetailIf(onError, lineNumber, expected, actual, detail, ...)`
    #[allow(clippy::too_many_arguments)]
    pub fn add_detail_if(
        &mut self,
        line_number: usize,
        expected: &str,
        actual: &str,
        detail: Option<&str>,
        context: Option<String>,
        range: Option<(usize, usize)>,
        fix_info: Option<FixInfo>,
    ) {
        if expected != actual {
            let mut d = format!("Expected: {expected}; Actual: {actual}");
            if let Some(extra) = detail {
                if !extra.is_empty() {
                    d.push_str("; ");
                    d.push_str(extra);
                }
            }
            self.add(line_number, Some(d), context, range, fix_info);
        }
    }

    /// `addErrorContext(onError, lineNumber, context, start, end, range, fix)`
    #[allow(clippy::too_many_arguments)]
    pub fn add_context(
        &mut self,
        line_number: usize,
        context: &str,
        start: bool,
        end: bool,
        range: Option<(usize, usize)>,
        fix_info: Option<FixInfo>,
    ) {
        let normalized = context.replace(['\r', '\n'], "\n");
        let ellipsified = helpers::ellipsify(&normalized, start, end);
        self.add(line_number, None, Some(ellipsified), range, fix_info);
    }
}

impl Default for Emit {
    fn default() -> Self {
        Self::new()
    }
}

/// Static metadata + implementation for a rule.
pub struct RuleMeta {
    pub names: &'static [&'static str],
    pub description: &'static str,
    pub tags: &'static [&'static str],
    /// True for `parser: "micromark"` rules (need the token tree).
    pub micromark: bool,
    pub run: fn(&Params, &mut Emit),
}

impl RuleMeta {
    /// Primary rule id, e.g. "MD013".
    pub fn id(&self) -> &'static str {
        self.names[0]
    }

    /// Information URL (`ruleInformation`).
    pub fn information(&self) -> String {
        format!(
            "https://github.com/DavidAnson/markdownlint/blob/v0.41.0/doc/{}.md",
            self.names[0].to_lowercase()
        )
    }
}

/// The full rule registry, in canonical order.
pub const RULES: &[RuleMeta] = &[
    md001::RULE,
    md003::RULE,
    md004::RULE,
    md005::RULE,
    md007::RULE,
    md009::RULE,
    md010::RULE,
    md011::RULE,
    md012::RULE,
    md013::RULE,
    md014::RULE,
    md018::RULE,
    md019::MD019,
    md020::RULE,
    md019::MD021,
    md022::RULE,
    md023::RULE,
    md024::RULE,
    md025::RULE,
    md026::RULE,
    md027::RULE,
    md028::RULE,
    md029::RULE,
    md030::RULE,
    md031::RULE,
    md032::RULE,
    md033::RULE,
    md034::RULE,
    md035::RULE,
    md036::RULE,
    md037::RULE,
    md038::RULE,
    md039::RULE,
    md049::MD049,
    md049::MD050,
    md051::RULE,
    md052::RULE,
    md053::RULE,
    md054::RULE,
    md042::RULE,
    md043::RULE,
    md044::RULE,
    md045::RULE,
    md055::RULE,
    md056::RULE,
    md058::RULE,
    md059::RULE,
    md060::RULE,
    md040::RULE,
    md041::RULE,
    md046::RULE,
    md047::RULE,
    md048::RULE,
];

/// Find a rule by (case-insensitive) primary id.
pub fn rule_by_id(id: &str) -> Option<&'static RuleMeta> {
    let up = id.to_uppercase();
    RULES.iter().find(|r| r.names[0].eq_ignore_ascii_case(&up))
}
