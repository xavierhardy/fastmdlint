#!/usr/bin/env bash
# Side-by-side parity check: run the real markdownlint-cli and fastmdlint over
# a corpus of Markdown files and assert byte-identical stdout + exit code.
#
# Usage: tests/parity.sh [corpus_dir ...]
set -u

REF_CLI="${REF_CLI:-$HOME/tmp/markdownlint-cli/markdownlint.js}"
BIN="${BIN:-./target/release/fastmdlint}"
[ -x "$BIN" ] || BIN="./target/debug/fastmdlint"

if [ ! -f "$REF_CLI" ]; then
  echo "reference markdownlint-cli not found at $REF_CLI" >&2
  exit 2
fi

corpora=("$@")
if [ ${#corpora[@]} -eq 0 ]; then
  corpora=(examples tests/corpus)
fi

pass=0
fail=0
failed_files=()

CONFIG_ARGS=()
if [ -n "${CONFIG:-}" ]; then CONFIG_ARGS=(-c "$CONFIG"); fi

run_ref() { node "$REF_CLI" "${CONFIG_ARGS[@]}" "$@" 2>&1; }
run_fast() { "$BIN" "${CONFIG_ARGS[@]}" "$@" 2>&1; }

check() {
  local file="$1"; shift
  local out_ref rc_ref out_fast rc_fast
  out_ref="$(run_ref "$file" "$@")"; rc_ref=$?
  out_fast="$(run_fast "$file" "$@")"; rc_fast=$?
  if [ "$out_ref" == "$out_fast" ] && [ "$rc_ref" == "$rc_fast" ]; then
    pass=$((pass+1))
  else
    fail=$((fail+1))
    failed_files+=("$file $*")
    if [ -n "${VERBOSE:-}" ]; then
      echo "=== MISMATCH: $file $* (ref rc=$rc_ref fast rc=$rc_fast) ==="
      diff <(printf '%s\n' "$out_ref") <(printf '%s\n' "$out_fast") | head -40
    fi
  fi
}

check_fix() {
  local file="$1"
  local ref fast
  ref="$(mktemp)"; fast="$(mktemp)"
  cp "$file" "$ref"; cp "$file" "$fast"
  node "$REF_CLI" "${CONFIG_ARGS[@]}" --fix "$ref" >/dev/null 2>&1
  "$BIN" "${CONFIG_ARGS[@]}" --fix "$fast" >/dev/null 2>&1
  if diff -q "$ref" "$fast" >/dev/null; then
    pass=$((pass+1))
  else
    fail=$((fail+1))
    failed_files+=("$file --fix")
    if [ -n "${VERBOSE:-}" ]; then
      echo "=== FIX MISMATCH: $file ==="
      diff "$ref" "$fast" | head -30
    fi
  fi
  rm -f "$ref" "$fast"
}

for dir in "${corpora[@]}"; do
  [ -d "$dir" ] || continue
  while IFS= read -r -d '' f; do
    check "$f"
    check "$f" --json
    check_fix "$f"
  done < <(find "$dir" -type f \( -name '*.md' -o -name '*.markdown' \) -print0)
done

echo ""
echo "parity: $pass matched, $fail mismatched"
if [ "$fail" -gt 0 ]; then
  printf '  mismatch: %s\n' "${failed_files[@]}" | sort -u | head -60
  exit 1
fi
