//! Marks and errors.

/// A position in the character buffer: all fields are 0-based, `pointer`
/// is the character index into the buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mark {
    /// Character index in the buffer (PyYAML `index`/`pointer`).
    pub pointer: usize,
    /// Line number, 0-based.
    pub line: usize,
    /// Column number, 0-based (in characters).
    pub column: usize,
}

/// A scanner or parser error.
/// Diagnostics consume only `problem` and `problem_mark`.
#[derive(Debug, Clone)]
pub struct YamlError {
    pub context: Option<String>,
    pub context_mark: Option<Mark>,
    pub problem: String,
    pub problem_mark: Option<Mark>,
}

impl YamlError {
    pub fn new(
        context: Option<&str>,
        context_mark: Option<Mark>,
        problem: String,
        problem_mark: Option<Mark>,
    ) -> Self {
        Self {
            context: context.map(str::to_string),
            context_mark,
            problem,
            problem_mark,
        }
    }
}
