#!/usr/bin/env bash
# Benchmark fastmdlint against the real markdownlint-cli on the same inputs.
#
# Scenarios: single small file, single large file, many files. Wall-clock
# time including process startup, average of N runs, default config.
#
# Usage: bench/bench.sh
set -u

REF_CLI="${REF_CLI:-$HOME/tmp/markdownlint-cli/markdownlint.js}"
BIN="${BIN:-./target/release/fastmdlint}"
RUNS="${RUNS:-5}"

if [ ! -f "$REF_CLI" ]; then
  echo "reference markdownlint-cli not found at $REF_CLI" >&2
  exit 2
fi
if [ ! -x "$BIN" ]; then
  echo "fastmdlint release binary not found at $BIN (cargo build --release)" >&2
  exit 2
fi

# --- inputs ---
SMALL=/tmp/bench_small.md
LARGE=/tmp/bench_large.md
MANY=/tmp/bench_many

python3 - <<'PY'
import os, random
random.seed(42)
out = ["# Large benchmark document\n"]
for sec in range(500):
    out.append(f"\n## Section {sec}\n")
    out.append(f"\nSome paragraph text for section {sec} with *emphasis*, **strong**, `code span`, and a [link](https://example.com/{sec}).\n")
    out.append("\n- item one\n- item two\n  - nested item\n- item three\n")
    out.append(f"\n```python\ndef func_{sec}():\n    return {sec}\n```\n")
    out.append(f"\n> A blockquote for section {sec}\n> with two lines.\n")
    out.append("\n| col a | col b |\n| ----- | ----- |\n| 1     | 2     |\n")
    out.append(f"\nTrailing paragraph mentioning https://example.org/page/{sec} bare URL.\n")
open("/tmp/bench_large.md","w").write("".join(out))
PY
cp examples/sample.md "$SMALL"
rm -rf "$MANY" && mkdir -p "$MANY"
for i in $(seq 1 400); do cp tests/corpus/kitchensink.md "$MANY/f$i.md"; done

# --- timing helper: average wall-clock ms over $RUNS runs ---
avg_ms() {
  python3 - "$RUNS" "$@" <<'PY'
import subprocess, sys, time
runs = int(sys.argv[1]); cmd = sys.argv[2:]
times = []
for _ in range(runs):
    t0 = time.perf_counter()
    subprocess.run(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    times.append((time.perf_counter() - t0) * 1000)
print(f"{sum(times)/len(times):.1f}")
PY
}

lines_of() { wc -l < "$1" | tr -d ' '; }

echo "runs per scenario: $RUNS (average, wall clock incl. startup)"
echo ""
printf "%-34s %12s %12s %9s\n" "scenario" "markdownlint" "fastmdlint" "speedup"
printf "%-34s %12s %12s %9s\n" "--------" "------------" "----------" "-------"

bench() {
  local name="$1"; shift
  local ref fast
  ref=$(avg_ms node "$REF_CLI" "$@")
  fast=$(avg_ms "$BIN" "$@")
  local speedup
  speedup=$(python3 -c "print(f'{$ref/$fast:.1f}x')")
  printf "%-34s %10s ms %10s ms %9s\n" "$name" "$ref" "$fast" "$speedup"
}

bench "small file ($(lines_of "$SMALL") lines)" "$SMALL"
bench "large file ($(lines_of "$LARGE") lines)" "$LARGE"
bench "many files (400 files)" "$MANY"
