# fastmdlint Architecture

fastmdlint is a drop-in replacement for markdownlint-cli. Output parity is the
design constraint that shapes everything: markdownlint's rules operate on a
micromark token tree with exact line/column positions, so fastmdlint builds its
own Markdown token tree modelled on that shape rather than using an
off-the-shelf Markdown parser.

## Layers

```
src/
├── md/                Markdown parsing
│   ├── tokens.rs      Token tree model (arena, pre-order = flat order) and
│   │                  the helpers rules rely on (filter by type, heading
│   │                  level/style/text, descendants, parent-of-type)
│   └── parser.rs      Line-oriented block parser + inline code spans,
│                      producing micromark-compatible token types/positions
├── rules/             One module per rule (mdNNN.rs), plus the registry,
│   │                  RuleMeta metadata, the Emit/RawError sink and the
│   │                  addError/addErrorDetailIf/addErrorContext helpers
│   └── helpers.rs     Config accessors, isBlankLine, isHtmlFlowComment,
│                      frontMatterHasTitle, ellipsify
├── config.rs          Config: JSON/JSONC/YAML/TOML parsing, extends,
│                      default/alias/tag resolution (getEffectiveConfig)
├── linter.rs          Pipeline: front-matter removal, inline-config comment
│                      handling, HTML-comment clearing, parse, run rules,
│                      offset line numbers (mirrors lintContent)
├── fix.rs             applyFixes port (sort/dedupe/collapse/apply) + dry-run
│                      unified diff
├── output.rs          text (markdownlint-cli format) and JSON output, with
│                      the CLI's exact sort order
├── runner.rs          File discovery (files/dirs/globs) + parallel linting
├── pyyaml/            Internal YAML loader, reused only to read YAML config
└── main.rs            CLI (markdownlint-cli-compatible flags + --dry-run)
```

## The token tree

`src/md/parser.rs` reproduces the token types markdownlint's rules consume:
`atxHeading`/`atxHeadingSequence`/`atxHeadingText`, `setextHeading`/
`setextHeadingText`/`setextHeadingLine`, `codeFenced`/`codeFencedFence`/
`codeFencedFenceSequence`/`codeFencedFenceInfo`/`codeFlowValue`,
`codeIndented`, `thematicBreak`, `listOrdered`/`listUnordered`/
`listItemPrefix`/`listItemMarker`/`listItemValue`, `blockQuote`, `htmlFlow`,
`paragraph`/`content`/`data`, `codeText`, and the `lineEnding`/`lineEndingBlank`
tokens that whitespace rules depend on.

Tokens live in an arena (`Vec<Token>`) in pre-order, which matches the order of
micromark's flattened token list — the order rules iterate in. Positions are
1-based; `end_column` is one past the last character, exactly as micromark
reports. The tree shape was checked against the real tree using a token dumper
built on markdownlint's own `micromark-parse` module.

The block parser is faithful for the common CommonMark constructs (headings,
code blocks, thematic breaks, lists with marker-change splitting, blockquotes,
paragraphs with setext detection and list interruption). Deep inline
constructs (emphasis, links/images, inline HTML, tables) are not yet fully
tokenized, which bounds the set of rules currently at parity.

## The linting pipeline

`linter::lint` mirrors markdownlint's `lintContent`:

1. Strip the BOM and remove front matter (YAML/TOML/`{}`), remembering the
   front-matter line count.
2. Compute enabled-rules-per-line from the (uncleared) lines by scanning inline
   `<!-- markdownlint-* -->` directives (disable/enable/disable-file/
   enable-file/disable-line/disable-next-line/capture/restore).
3. Parse the token tree from the uncleared, front-matter-stripped content.
4. Clear HTML-comment text to produce the lines rules see.
5. Run each enabled rule; each reported problem's line number is offset by the
   front-matter line count and kept only if the rule is enabled on that line.

Rules produce identical message strings, positions, ranges, fixInfo and
ordering to the upstream rule.

## Fixing

`fix.rs` is a direct port of markdownlint's `applyFixes`: normalize each
`fixInfo`, sort bottom-to-top / line-deletes-last / right-to-left /
long-to-short, remove duplicates, collapse insert/delete pairs, then apply,
skipping overlapping edits. `--dry-run` renders a unified diff instead of
writing. This produces byte-identical results to `markdownlint --fix`.

## Testing strategy

- `tests/parity.sh`: runs the real markdownlint-cli and fastmdlint side by side
  over `tests/corpus/` and asserts identical stdout, identical `--json`,
  identical `--fix` output and identical exit codes.
- `tests/lint_test.rs`: pure-Rust integration tests asserting exact output for
  known cases (captured from the reference), config resolution, inline
  directives, severity and front-matter offsetting.
