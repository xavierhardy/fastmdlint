# TODOs

Genuine improvement areas identified by a codebase review, implemented one by
one (one commit each).

- [x] **Fix repository URL in Cargo.toml** — points to
  `https://github.com/xhardy/fastmdlint`; the actual repository is
  `https://github.com/xavierhardy/fastmdlint`.
- [x] **Correct the rule count in README.md** — the text says "all 52" rules in
  three places, but the registry (`src/rules/mod.rs`) and README's own rule
  list both contain 53 rules.
- [ ] **Format the codebase with `cargo fmt`** — `cargo fmt --check` currently
  reports diffs in several files.
- [ ] **Fix clippy warnings** — `cargo clippy --all-targets` reports 50
  warnings (collapsible ifs, `is_multiple_of`, `while let` loops, etc.). Apply
  the mechanical fixes; where a warning flags branch structure that
  deliberately mirrors the upstream markdownlint code, keep the structure and
  add a targeted `#[allow]`.
- [ ] **Remove stale `tests/only-implemented.json`** — referenced by no script
  or test, and out of date (missing MD053); all rules are implemented now so
  the "only implemented rules" config no longer serves a purpose.
- [ ] **Add GitHub Actions CI** — run `cargo fmt --check`,
  `cargo clippy --all-targets -- -D warnings`, and `cargo test` on pushes and
  pull requests.
