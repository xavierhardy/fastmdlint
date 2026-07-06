//! Markdown parsing: a token tree modelled on markdownlint's micromark tree.

pub mod parser;
pub mod tokens;

pub use parser::Parser;
pub use tokens::{Token, Tree};
