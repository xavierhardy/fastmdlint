//! Token types.

use super::error::Mark;

#[derive(Debug, Clone, PartialEq)]
pub enum DirectiveValue {
    Yaml(u64, u64),
    Tag(String, String),
    Other,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    StreamStart,
    StreamEnd,
    Directive {
        name: String,
        value: DirectiveValue,
    },
    DocumentStart,
    DocumentEnd,
    BlockSequenceStart,
    BlockMappingStart,
    BlockEnd,
    FlowSequenceStart,
    FlowMappingStart,
    FlowSequenceEnd,
    FlowMappingEnd,
    Key,
    Value,
    BlockEntry,
    FlowEntry,
    Alias(String),
    Anchor(String),
    Tag {
        handle: Option<String>,
        suffix: String,
    },
    Scalar {
        value: String,
        plain: bool,
        /// `None` for plain scalars, otherwise one of `'`, `"`, `|`, `>`.
        style: Option<char>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub start_mark: Mark,
    pub end_mark: Mark,
}

impl Token {
    pub fn new(kind: TokenKind, start_mark: Mark, end_mark: Mark) -> Self {
        Self {
            kind,
            start_mark,
            end_mark,
        }
    }

    /// PyYAML token `id`, used in parser error messages.
    pub fn id(&self) -> &'static str {
        match &self.kind {
            TokenKind::StreamStart => "<stream start>",
            TokenKind::StreamEnd => "<stream end>",
            TokenKind::Directive { .. } => "<directive>",
            TokenKind::DocumentStart => "<document start>",
            TokenKind::DocumentEnd => "<document end>",
            TokenKind::BlockSequenceStart => "<block sequence start>",
            TokenKind::BlockMappingStart => "<block mapping start>",
            TokenKind::BlockEnd => "<block end>",
            TokenKind::FlowSequenceStart => "[",
            TokenKind::FlowMappingStart => "{",
            TokenKind::FlowSequenceEnd => "]",
            TokenKind::FlowMappingEnd => "}",
            TokenKind::Key => "?",
            TokenKind::Value => ":",
            TokenKind::BlockEntry => "-",
            TokenKind::FlowEntry => ",",
            TokenKind::Alias(_) => "<alias>",
            TokenKind::Anchor(_) => "<anchor>",
            TokenKind::Tag { .. } => "<tag>",
            TokenKind::Scalar { .. } => "<scalar>",
        }
    }

    pub fn is_scalar(&self) -> bool {
        matches!(self.kind, TokenKind::Scalar { .. })
    }

    pub fn scalar_value(&self) -> Option<&str> {
        match &self.kind {
            TokenKind::Scalar { value, .. } => Some(value),
            _ => None,
        }
    }

    pub fn scalar_style(&self) -> Option<char> {
        match &self.kind {
            TokenKind::Scalar { style, .. } => *style,
            _ => None,
        }
    }

    pub fn scalar_is_plain(&self) -> bool {
        matches!(self.kind, TokenKind::Scalar { plain: true, .. })
    }
}
