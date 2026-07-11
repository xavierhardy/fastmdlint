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

- **Speed**: typically 5–12× faster than markdownlint-cli (see
  [Performance](#performance)), with parallel processing of multiple files
  (configurable with `--jobs`).
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

fastmdlint implements **all 53** of markdownlint's rules at byte-for-byte
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

Shortcut reference links (`[label]` with a matching definition) are tokenized
like the reference implementation, so the `MD054` `shortcut` option and link
rules on shortcut links behave identically; undefined references (full,
collapsed, and — with `MD052`'s `shortcut_syntax` — shortcut) are detected
exactly like upstream's undefined-reference tracking; lists inside blockquotes
are parsed as containers, so `MD007` applies the same blockquote-indent
adjustment as upstream. Even upstream's `parser: "none"` quirk is reproduced:
`MD052`/`MD053` see an empty token list (and report nothing) when no
parser-based rule is enabled. Everything exercised by the parity corpus
matches exactly.

fastmdlint is written in 100% safe Rust (`#![forbid(unsafe_code)]`).

## Development

```console
# Build
cargo build --release

# Rust unit + integration tests
cargo test

# Side-by-side parity against the real markdownlint-cli
#   (expects it at ~/tmp/markdownlint-cli/markdownlint.js)
bash tests/parity.sh tests/corpus

# Benchmarks vs markdownlint-cli
bash bench/bench.sh
```

## Verified parity with markdownlint-cli

`tests/parity.sh` runs the real markdownlint-cli and fastmdlint side by side
over a corpus (markdownlint's own rule docs, the CLI's test fixtures, and
hand-written violation files) and asserts **byte-for-byte identical stdout**,
identical **JSON** output, identical **`--fix`** results, and identical exit
codes, with all 53 rules enabled (the default). The current corpus reports
**213/213 comparisons identical**, including a dense "kitchen-sink" fixture
that triggers 20+ rules at once.

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for an overview of the code.

## Performance

`bench/bench.sh` compares both tools on the same inputs with the default
configuration (all 53 rules). Representative results (Apple M1 Pro,
markdownlint-cli 0.49.0 on Node.js v22, average of 5 runs including process
startup):

| scenario                      | markdownlint-cli | fastmdlint | speedup |
|-------------------------------|------------------|------------|---------|
| single small file (18 lines)  | 301.0 ms         | 24.2 ms    | ~12×    |
| single large file (11.5k lines) | 1051.7 ms      | 117.2 ms   | ~9×     |
| many files (400 files)        | 1303.2 ms        | 254.9 ms   | ~5×     |

The small-file case is dominated by process startup on both sides (Node.js
startup and module loading vs. a native binary); the large-file case reflects
raw linting throughput; and the many-files case additionally benefits from
parallel processing (configurable with `--jobs`).

## License and acknowledgements

fastmdlint is a reimplementation of
[markdownlint](https://github.com/DavidAnson/markdownlint) by David Anson and
[markdownlint-cli](https://github.com/igorshubovych/markdownlint-cli) by Igor
Shubovych — the rules, their options and messages, the configuration system and
the output format are reimplementations of their behavior, developed with them
as the reference. Both are released under the MIT License, and fastmdlint keeps
the same license: **MIT** (see [LICENSE](LICENSE)).
