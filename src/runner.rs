//! File discovery (files, directories, globs) and parallel linting.

use std::path::{Path, PathBuf};

use rayon::prelude::*;

use crate::config::Config;
use crate::linter::{lint_config, LintError};

/// A discovered input file.
#[derive(Clone)]
pub struct FileEntry {
    /// Path as given / discovered (used for display and sorting).
    pub original: String,
    pub path: PathBuf,
}

const EXTENSIONS: &[&str] = &["md", "markdown"];

fn has_md_ext(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Expand command-line arguments into a list of files. Directories are walked
/// recursively for `*.md`/`*.markdown`; other args are treated as literal
/// files, or as globs if they contain glob metacharacters.
pub fn expand_inputs(args: &[String], dot: bool) -> Vec<FileEntry> {
    let mut out = Vec::new();
    for arg in args {
        let p = Path::new(arg);
        if p.is_dir() {
            walk_dir(arg, dot, &mut out);
        } else if p.is_file() {
            out.push(FileEntry {
                original: arg.clone(),
                path: p.to_path_buf(),
            });
        } else if arg.contains('*') || arg.contains('?') || arg.contains('[') {
            expand_glob(arg, dot, &mut out);
        } else {
            // Non-existent literal path: still record it (markdownlint errors later).
            out.push(FileEntry {
                original: arg.clone(),
                path: p.to_path_buf(),
            });
        }
    }
    // Deduplicate by canonical path, keep first.
    let mut seen = std::collections::HashSet::new();
    out.retain(|e| {
        let key = std::fs::canonicalize(&e.path)
            .unwrap_or_else(|_| e.path.clone());
        seen.insert(key)
    });
    out
}

fn is_hidden(name: &str) -> bool {
    name.starts_with('.') && name != "." && name != ".."
}

fn walk_dir(dir: &str, dot: bool, out: &mut Vec<FileEntry>) {
    for entry in walkdir::WalkDir::new(dir)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| dot || !is_hidden(&e.file_name().to_string_lossy()))
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() && has_md_ext(entry.path()) {
            out.push(FileEntry {
                original: entry.path().to_string_lossy().to_string(),
                path: entry.path().to_path_buf(),
            });
        }
    }
}

fn expand_glob(pattern: &str, _dot: bool, out: &mut Vec<FileEntry>) {
    let glob = match globset::Glob::new(pattern) {
        Ok(g) => g.compile_matcher(),
        Err(_) => return,
    };
    // Walk from current directory.
    for entry in walkdir::WalkDir::new(".")
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() {
            let rel = entry.path().strip_prefix("./").unwrap_or(entry.path());
            let s = rel.to_string_lossy();
            if glob.is_match(s.as_ref()) {
                out.push(FileEntry {
                    original: s.to_string(),
                    path: rel.to_path_buf(),
                });
            }
        }
    }
}

/// One file's lint outcome.
pub struct FileResult {
    pub original: String,
    pub errors: Vec<LintError>,
    pub io_error: Option<String>,
}

fn lint_one(entry: &FileEntry, cfg: &Config) -> FileResult {
    match std::fs::read_to_string(&entry.path) {
        Ok(content) => FileResult {
            original: entry.original.clone(),
            errors: lint_config(&content, cfg),
            io_error: None,
        },
        Err(e) => FileResult {
            original: entry.original.clone(),
            errors: Vec::new(),
            io_error: Some(format!("{}: {e}", entry.path.display())),
        },
    }
}

/// Lint files, in parallel when there is more than one.
pub fn lint_files(files: &[FileEntry], cfg: &Config, jobs: Option<usize>) -> Vec<FileResult> {
    let run = || files.par_iter().map(|f| lint_one(f, cfg)).collect::<Vec<_>>();
    match jobs {
        _ if files.len() <= 1 => files.iter().map(|f| lint_one(f, cfg)).collect(),
        Some(1) => files.iter().map(|f| lint_one(f, cfg)).collect(),
        Some(n) => rayon::ThreadPoolBuilder::new()
            .num_threads(n)
            .build()
            .map(|pool| pool.install(run))
            .unwrap_or_else(|_| run()),
        None => run(),
    }
}
