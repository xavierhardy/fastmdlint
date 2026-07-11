# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

fastmdlint is a Rust drop-in replacement for markdownlint-cli. **Byte-for-byte output parity with markdownlint-cli is the design constraint that shapes everything**: messages, positions, rule names, severities, ordering, exit codes, `--fix` results, and JSON output must match the reference tool exactly. When changing rule or parser behavior, the question is never "is this reasonable?" but "is this what markdownlint does?" — upstream markdownlint (DavidAnson/markdownlint) is the reference implementation.

The crate is both a binary (`src/main.rs`) and a library (`src/lib.rs`), and is 100% safe Rust (`#![forbid(unsafe_code)]`), edition 2024.

## Commands

```bash
cargo build --release          # build (release has lto + codegen-units=1)
cargo test                     # unit + integration tests
cargo test md018               # run tests matching a name filter
cargo test --test lint_test    # run only the integration test file

# Parity harness: runs the real markdownlint-cli and fastmdlint side by side
# over examples/ + tests/corpus/, asserting identical stdout, --json output,
# --fix results, and exit codes. Requires the reference CLI:
#   REF_CLI (default ~/tmp/markdownlint-cli/markdownlint.js)
bash tests/parity.sh                    # default corpora
bash tests/parity.sh tests/corpus      # specific corpus dir
VERBOSE=1 bash tests/parity.sh          # show diffs on mismatch
CONFIG=path/to/config.json bash tests/parity.sh   # with a config file

# Benchmarks vs markdownlint-cli (needs REF_CLI and a release build)
bash bench/bench.sh
```

`tests/lint_test.rs` asserts exact output strings captured from the real markdownlint-cli, so it doubles as a parity regression guard that works without the reference CLI installed.

## Architecture

See `docs/ARCHITECTURE.md` for the full picture. The short version:

- **`src/md/`** — the Markdown parser. It does *not* use an off-the-shelf parser; it builds a token tree modelled on micromark's (the parser markdownlint uses), because rules depend on micromark's exact token types and positions. Tokens live in an arena (`Vec<Token>`) in **pre-order, which matches micromark's flattened token list order** — the order rules iterate in. Positions are 1-based; `end_column` is one past the last character.
- **`src/rules/`** — one module per rule (`mdNNN.rs`), plus the registry in `mod.rs`. A few modules host two closely related rules (`md019.rs` has MD019+MD021, `md049.rs` has MD049+MD050).
- **`src/linter.rs`** — the pipeline, mirroring markdownlint's `lintContent`: strip BOM/front matter → scan inline `<!-- markdownlint-* -->` directives for enabled-rules-per-line → parse the token tree from *uncleared* content → clear HTML-comment text to produce the lines rules see → run rules → offset line numbers by the front-matter line count.
- **`src/fix.rs`** — a direct port of markdownlint's `applyFixes` (normalize, sort bottom-to-top/right-to-left, dedupe, collapse, apply, skip overlaps), plus `--dry-run` unified diff.
- **`src/config.rs`** — JSON/JSONC/YAML/TOML config, `extends`, `default`/alias/tag resolution (`getEffectiveConfig`).
- **`src/output.rs`** — text and JSON output with the CLI's exact sort order.
- **`src/runner.rs`** — file discovery (files/dirs/globs) and rayon-parallel linting.
- **`src/pyyaml/`** — internal YAML loader, used only to read YAML config files.

### Anatomy of a rule

Each rule is a `fn(&Params, &mut Emit)` plus a `RuleMeta` const (names, description, tags, `micromark` flag), registered in `RULES` in `src/rules/mod.rs` — **the registry order is canonical and matches upstream**, so don't alphabetize it. `Params` gives rules:

- `lines`: front-matter-stripped, **HTML-comment-cleared** lines (no line endings)
- `tree`: the token tree, parsed from stripped but *uncleared* content
- `config`: that rule's options object as `serde_json::Value`

`Emit`'s `add` / `add_detail_if` / `add_context` mirror upstream's `addError` / `addErrorDetailIf` / `addErrorContext`, including exact message formatting ("Expected: X; Actual: Y") and context ellipsification. Fixes are described with `FixInfo`, mirroring upstream's `fixInfo`.

The `micromark: bool` flag on `RuleMeta` marks parser-based rules. This matters for a deliberately reproduced upstream quirk: when no micromark rule is enabled, MD052/MD053 see an empty token list and report nothing (upstream's `parser: "none"` behavior).

## Conventions

- Rule messages, details, contexts, ranges, and `FixInfo` must match upstream character for character. Rule information URLs are pinned to markdownlint v0.41.0 docs (see `RuleMeta::information`).
- When adding or changing a rule: update/add a fixture in `tests/corpus/`, verify with `tests/parity.sh` against the real CLI, and add an exact-output assertion in `tests/lint_test.rs`.
- `tests/corpus/` holds the parity fixtures: markdownlint's own rule docs, CLI test fixtures, and hand-written violation files (including `kitchensink.md`, which triggers 20+ rules at once).
- Exit codes match markdownlint-cli: 0 = clean, 1 = lint errors, 2 = can't write `--output`, 4 = unexpected problem (e.g. malformed config).
