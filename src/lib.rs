// fastmdlint — a fast, drop-in replacement for markdownlint-cli written in
// Rust, with auto-fix and dry-run support.
//
// The rule set, messages, positions and configuration system are
// reimplementations of markdownlint (Copyright (C) David Anson and
// contributors, MIT), developed with markdownlint as the reference for
// byte-for-byte output parity.

#![forbid(unsafe_code)]

//! fastmdlint — a fast, drop-in replacement for markdownlint-cli.
//!
//! Diagnostics — messages, positions, rule names, severities and output
//! format — are compatible with markdownlint-cli. On top of that, fastmdlint
//! adds parallel file processing and dry-run fixing.

pub mod config;
pub mod decoder;
pub mod fix;
pub mod linter;
pub mod md;
pub mod output;
pub mod pyyaml;
pub mod rules;
pub mod runner;

pub use config::{Config, ResolvedConfig};
pub use linter::{LintError, lint};
pub use rules::Severity;
