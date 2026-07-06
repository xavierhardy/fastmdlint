# fastmdlint

A fast, drop-in replacement for
[markdownlint-cli](https://github.com/igorshubovych/markdownlint-cli) written in
Rust — plus dry-run fixing.

fastmdlint reimplements markdownlint's rules on top of its own Markdown token
tree (modelled on markdownlint's micromark tree), so diagnostics — messages,
positions, rule names, severities, ordering and the exact output format —
match markdownlint-cli **byte for byte** for the implemented rules (verified by
a side-by-side parity harness against the real tool). On top of that,
fastmdlint adds:

- **Speed**: native Rust with parallel processing of multiple files.
- **Auto-fix** (`--fix`): applies markdownlint's fixes with the same
  `applyFixes` algorithm, producing byte-identical results.
- **Dry-run** (`--dry-run`): shows a unified diff of what `--fix` would change
  without writing anything.
- **Same config files**: JSON, JSONC, YAML and TOML, with `default`,
  per-rule options, aliases, tags, `extends`, `--enable`/`--disable`,
  `--configPointer`, and inline `<!-- markdownlint-* -->` directives.

## Usage

```console
markdownlint-compatible CLI:

fastmdlint file.md dir/ '**/*.md'      # files, directories, globs
fastmdlint -c .markdownlint.json file.md
fastmdlint --json file.md              # JSON output
fastmdlint -s < file.md                # read from STDIN
fastmdlint --disable MD013 -- file.md  # disable rules
fastmdlint --enable MD041 -- file.md   # enable rules
fastmdlint -f file.md                  # fix in place
fastmdlint -f --dry-run file.md        # show the fix diff, write nothing
fastmdlint --jobs 4 dir/               # limit parallelism
```

### Exit codes

Matches markdownlint-cli:

| code | meaning                                            |
|------|----------------------------------------------------|
| 0    | linting successful, no errors (warnings possible)  |
| 1    | linting successful, some errors                    |
| 2    | unable to write `-o`/`--output` file               |
| 4    | unexpected problem (e.g. malformed config)         |

## Configuration

fastmdlint reads the same configuration as markdownlint-cli:

- project files `.markdownlint.jsonc` / `.markdownlint.json` /
  `.markdownlint.yaml` / `.markdownlint.yml` in the current directory, or a
  file passed with `-c`/`--config` (JSON, JSONC, YAML or TOML);
- the config object maps rule ids (`MD013`), aliases (`line-length`) or tags
  (`whitespace`) to `true`, `false`, `"warning"`, or an options object;
  `default` sets the baseline; `extends` merges a base config;
- `--enable` / `--disable` override the config; `--configPointer` selects a
  sub-object via JSON Pointer;
- inline directives: `<!-- markdownlint-disable -->`, `enable`,
  `disable-line`, `disable-next-line`, `disable-file`, `enable-file`,
  `capture`, `restore` (with optional `MD0xx`/alias/tag arguments).

## Rule coverage

fastmdlint implements **all 52** of markdownlint's rules at byte-for-byte
parity:

`MD001` `MD003` `MD004` `MD005` `MD007` `MD009` `MD010` `MD011` `MD012`
`MD013` `MD014` `MD018` `MD019` `MD020` `MD021` `MD022` `MD023` `MD024`
`MD025` `MD026` `MD027` `MD028` `MD029` `MD030` `MD031` `MD032` `MD033`
`MD034` `MD035` `MD036` `MD037` `MD038` `MD039` `MD040` `MD041` `MD042`
`MD043` `MD044` `MD045` `MD046` `MD047` `MD048` `MD049` `MD050` `MD051`
`MD052` `MD053` `MD054` `MD055` `MD056` `MD058` `MD059` `MD060`

The parser reproduces the CommonMark + GFM constructs the rules depend on:
ATX/setext headings, fenced/indented code, blockquotes (with per-line
prefixes), ordered/unordered lists (with CommonMark nesting), thematic breaks,
HTML blocks, reference definitions, GFM tables, and the full inline layer —
code spans, autolinks, raw HTML, links/images (inline and reference), literal
autolinks, and emphasis/strong via the delimiter-run algorithm.

A few narrow edge cases are approximated (documented at the top of the
affected rule modules): shortcut reference links are matched by scan rather
than tokenized (affects the non-default `MD052`/`MD054` shortcut options), and
`MD007` does not model the blockquote-indent adjustment. Everything exercised
by the parity corpus matches exactly.

fastmdlint is written in 100% safe Rust (`#![forbid(unsafe_code)]`).

## Development

```console
# Build
cargo build --release

# Rust unit + integration tests
cargo test

# Side-by-side parity against the real markdownlint-cli
#   (expects it at ~/tmp/markdownlint-cli/markdownlint.js)
CONFIG=tests/only-implemented.json bash tests/parity.sh tests/corpus
```

## Verified parity with markdownlint-cli

`tests/parity.sh` runs the real markdownlint-cli and fastmdlint side by side
over a corpus (markdownlint's own rule docs, the CLI's test fixtures, and
hand-written violation files) and asserts **byte-for-byte identical stdout**,
identical **JSON** output, identical **`--fix`** results, and identical exit
codes, with all 52 rules enabled (the default). The current corpus reports
**204/204 comparisons identical**, including a dense "kitchen-sink" fixture
that triggers 20+ rules at once.

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for an overview of the code.

## License and acknowledgements

fastmdlint is a reimplementation of
[markdownlint](https://github.com/DavidAnson/markdownlint) by David Anson and
[markdownlint-cli](https://github.com/igorshubovych/markdownlint-cli) by Igor
Shubovych — the rules, their options and messages, the configuration system and
the output format are reimplementations of their behavior, developed with them
as the reference. Both are released under the MIT License, and fastmdlint keeps
the same license: **MIT** (see [LICENSE](LICENSE)).
