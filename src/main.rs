// fastmdlint — a fast, drop-in replacement for markdownlint-cli written in
// Rust. The rule set and configuration are reimplementations of markdownlint
// (MIT, David Anson) developed for byte-for-byte output parity.

#![forbid(unsafe_code)]

//! fastmdlint CLI — a drop-in replacement for markdownlint-cli.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use serde_json::{Map, Value};

use fastmdlint::Severity;
use fastmdlint::config::{Config, deep_merge};
use fastmdlint::fix::{fix_content, unified_diff};
use fastmdlint::output::{FileReport, OutputFormat};
use fastmdlint::runner::{FileEntry, expand_inputs, lint_files};

// Exit codes (markdownlint-cli).
const EXIT_LINT_ERRORS: u8 = 1;
const EXIT_WRITE_OUTPUT: u8 = 2;
const _EXIT_LOAD_RULES: u8 = 3;
const EXIT_UNEXPECTED: u8 = 4;

#[derive(Parser)]
#[command(
    name = "fastmdlint",
    version,
    about = "A fast, drop-in replacement for markdownlint-cli"
)]
struct Cli {
    /// Files, directories, and/or globs to lint
    files: Vec<String>,

    /// Configuration file (JSON, JSONC, YAML, or TOML)
    #[arg(short = 'c', long = "config")]
    config: Option<PathBuf>,

    /// JSON Pointer to object within configuration file
    #[arg(long = "configPointer", default_value = "")]
    config_pointer: String,

    /// Include files/folders with a dot (for example `.github`)
    #[arg(short = 'd', long = "dot")]
    dot: bool,

    /// Fix basic issues (does not work with STDIN)
    #[arg(short = 'f', long = "fix")]
    fix: bool,

    /// Show what --fix would change without writing files
    #[arg(long = "dry-run")]
    dry_run: bool,

    /// File(s) to ignore/exclude
    #[arg(short = 'i', long = "ignore")]
    ignore: Vec<String>,

    /// Write issues in json format
    #[arg(short = 'j', long = "json")]
    json: bool,

    /// Write issues to file (no console)
    #[arg(short = 'o', long = "output")]
    output: Option<PathBuf>,

    /// Path to file with ignore pattern(s)
    #[arg(short = 'p', long = "ignore-path")]
    ignore_path: Option<PathBuf>,

    /// Do not write issues to STDOUT
    #[arg(short = 'q', long = "quiet")]
    quiet: bool,

    /// Include custom rule files (unsupported; accepted for compatibility)
    #[arg(short = 'r', long = "rules")]
    rules: Vec<String>,

    /// Read from STDIN (does not work with files)
    #[arg(short = 's', long = "stdin")]
    stdin: bool,

    /// Enable certain rules
    #[arg(long = "enable", num_args = 1.., value_delimiter = ' ')]
    enable: Vec<String>,

    /// Disable certain rules
    #[arg(long = "disable", num_args = 1.., value_delimiter = ' ')]
    disable: Vec<String>,

    /// Number of parallel jobs (default: all CPUs)
    #[arg(long = "jobs")]
    jobs: Option<usize>,
}

const PROJECT_CONFIG_FILES: &[&str] = &[
    ".markdownlint.jsonc",
    ".markdownlint.json",
    ".markdownlint.yaml",
    ".markdownlint.yml",
];

fn read_configuration(cli: &Cli) -> Result<Value, String> {
    let mut config = Value::Object(Map::new());
    for name in PROJECT_CONFIG_FILES {
        if Path::new(name).is_file() {
            match Config::from_file(Path::new(name)) {
                Ok(c) => {
                    config = deep_merge(config, c.raw);
                    break;
                }
                Err(_) => break,
            }
        }
    }
    if let Some(user) = &cli.config {
        let c = Config::from_file(user)?;
        config = deep_merge(config, c.raw);
    }
    Ok(config)
}

/// Resolve a JSON Pointer (RFC 6901) into `value`.
fn json_pointer<'a>(value: &'a Value, pointer: &str) -> Option<&'a Value> {
    if pointer.is_empty() {
        return Some(value);
    }
    value.pointer(pointer)
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    if !cli.rules.is_empty() {
        eprintln!("fastmdlint: custom rules (-r/--rules) are not supported");
    }

    let configuration = match read_configuration(&cli) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(EXIT_UNEXPECTED);
        }
    };

    let mut config_value = json_pointer(&configuration, &cli.config_pointer)
        .cloned()
        .unwrap_or(Value::Object(Map::new()));
    if !config_value.is_object() {
        config_value = Value::Object(Map::new());
    }
    let mut config = Config::from_value(config_value);
    config.apply_enable_disable(&cli.enable, &cli.disable);

    let format = if cli.json {
        OutputFormat::Json
    } else {
        OutputFormat::Text
    };

    // STDIN mode.
    if cli.stdin && cli.files.is_empty() && !cli.fix {
        let mut buf = String::new();
        if std::io::stdin().read_to_string(&mut buf).is_err() {
            return ExitCode::from(EXIT_UNEXPECTED);
        }
        let errors = fastmdlint::linter::lint_config(&buf, &config);
        let reports = vec![FileReport {
            file: "stdin",
            errors: &errors,
        }];
        return finish(&cli, &reports, format);
    }

    if cli.files.is_empty() {
        // Nothing to do; markdownlint prints help. Exit 0.
        return ExitCode::SUCCESS;
    }

    // Discover files.
    let mut entries = expand_inputs(&cli.files, cli.dot);
    apply_ignores(&cli, &mut entries);

    // Fix mode.
    if cli.fix {
        let mut wrote = false;
        for entry in &entries {
            let content = match std::fs::read_to_string(&entry.path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let result = fix_content(&content, &config);
            if result.changed {
                if cli.dry_run {
                    print!("{}", unified_diff(&entry.original, &content, &result.fixed));
                } else if std::fs::write(&entry.path, &result.fixed).is_err() {
                    eprintln!("Cannot write file {}", entry.path.display());
                    return ExitCode::from(EXIT_UNEXPECTED);
                }
                wrote = true;
            }
        }
        let _ = wrote;
    }

    // Lint and report.
    let results = lint_files(&entries, &config, cli.jobs);
    for r in &results {
        if let Some(err) = &r.io_error {
            eprintln!("{err}");
        }
    }
    let reports: Vec<FileReport> = results
        .iter()
        .map(|r| FileReport {
            file: &r.original,
            errors: &r.errors,
        })
        .collect();
    finish(&cli, &reports, format)
}

fn finish(cli: &Cli, reports: &[FileReport], format: OutputFormat) -> ExitCode {
    let out = fastmdlint::output::render(reports, format);
    let has_errors = reports
        .iter()
        .flat_map(|r| r.errors.iter())
        .any(|e| e.severity == Severity::Error);

    if let Some(path) = &cli.output {
        let content = if out.is_empty() {
            String::new()
        } else {
            format!("{out}\n")
        };
        if std::fs::write(path, content).is_err() {
            eprintln!("Cannot write to output file {}", path.display());
            return ExitCode::from(EXIT_WRITE_OUTPUT);
        }
    } else if !out.is_empty() && !cli.quiet {
        eprintln!("{out}");
    }

    if has_errors {
        ExitCode::from(EXIT_LINT_ERRORS)
    } else {
        ExitCode::SUCCESS
    }
}

fn apply_ignores(cli: &Cli, entries: &mut Vec<FileEntry>) {
    // .markdownlintignore (or -p file) via gitignore semantics.
    let ignore_file = cli.ignore_path.clone().or_else(|| {
        let def = PathBuf::from(".markdownlintignore");
        if def.is_file() { Some(def) } else { None }
    });
    if let Some(path) = ignore_file {
        if let Ok(text) = std::fs::read_to_string(&path) {
            let mut builder = ignore::gitignore::GitignoreBuilder::new(".");
            for line in text.lines() {
                let _ = builder.add_line(None, line);
            }
            if let Ok(gi) = builder.build() {
                entries.retain(|e| {
                    let rel = e.path.strip_prefix("./").unwrap_or(&e.path);
                    !gi.matched(rel, rel.is_dir()).is_ignore()
                });
            }
        }
    }
    // -i ignore globs / paths.
    if !cli.ignore.is_empty() {
        let matchers: Vec<globset::GlobMatcher> = cli
            .ignore
            .iter()
            .filter_map(|p| globset::Glob::new(p).ok().map(|g| g.compile_matcher()))
            .collect();
        let literal: Vec<PathBuf> = cli
            .ignore
            .iter()
            .map(|p| std::fs::canonicalize(p).unwrap_or_else(|_| PathBuf::from(p)))
            .collect();
        entries.retain(|e| {
            let canon = std::fs::canonicalize(&e.path).unwrap_or_else(|_| e.path.clone());
            if literal.iter().any(|l| *l == canon) {
                return false;
            }
            let s = e.original.clone();
            !matchers.iter().any(|m| m.is_match(&s))
        });
    }
}
