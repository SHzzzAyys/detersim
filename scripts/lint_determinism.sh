#!/usr/bin/env bash
# Determinism lint gate: fail if a known non-determinism source appears in
# simulation/SUT paths. This is the cheap, fast counterpart to the determinism
# meta-test. See AGENTS.md §0.
#
# Comments are stripped before scanning, so documentation that *names* a
# forbidden API (e.g. "never use thread_rng") does not trip the gate.
#
# Usage: scripts/lint_determinism.sh
set -euo pipefail

# Paths that must stay deterministic. (detersim-real, when it exists, is the
# production tokio/std implementation and is intentionally NOT scanned.)
SCAN_DIRS=("crates/detersim-core/src" "crates/detersim-sim/src")
SCAN_DIRS+=("crates/detersim-nemesis/src")
SCAN_DIRS+=("crates/detersim-net/src")
SCAN_DIRS+=("crates/detersim-net/tests")
SCAN_DIRS+=("crates/detersim-protocols/src")
SCAN_DIRS+=("crates/detersim-check/src")
SCAN_DIRS+=("crates/detersim-check/tests")
SCAN_DIRS+=("crates/detersim-search/src")
SCAN_DIRS+=("crates/detersim-search/tests")
SCAN_DIRS+=("crates/detersim-shrink/src")
SCAN_DIRS+=("crates/detersim-shrink/tests")
SCAN_DIRS+=("crates/detersim-viz/src")
SCAN_DIRS+=("crates/detersim-viz/tests")
SCAN_DIRS+=("crates/detersim-testkit/src")
SCAN_DIRS+=("crates/detersim-testkit/examples")
SCAN_DIRS+=("crates/detersim-testkit/tests")
SCAN_DIRS+=("crates/detersim-sim/examples")
SCAN_DIRS+=("crates/detersim-sim/tests")

# Regexes for forbidden APIs. Each is a real door for non-determinism.
PATTERNS=(
  'Instant::now'
  'SystemTime::now'
  '\.elapsed\(\)'
  'std::thread'
  'thread::spawn'
  'tokio::spawn'
  'spawn_blocking'
  'std::net'
  'tokio::net'
  'std::fs'
  'tokio::fs'
  'thread_rng'
  'rand::random'
  'HashMap'
  'HashSet'
)

fail=0
while IFS= read -r -d '' file; do
  # Blank out line comments (// ...) but keep line numbers intact.
  stripped="$(sed 's://.*::' "$file")"
  for pat in "${PATTERNS[@]}"; do
    hits="$(printf '%s\n' "$stripped" | grep -nE "$pat" || true)"
    if [ -n "$hits" ]; then
      echo "FORBIDDEN pattern '$pat' in $file:"
      printf '%s\n' "$hits"
      fail=1
    fi
  done
done < <(find "${SCAN_DIRS[@]}" -name '*.rs' -print0)

if [ "$fail" -ne 0 ]; then
  echo ""
  echo "Determinism lint FAILED. Route the offending call through an Env capability"
  echo "(Clock/Rng/Network/Storage) or use a deterministic collection (BTreeMap/IndexMap)."
  exit 1
fi

echo "Determinism lint passed: no forbidden non-determinism sources in sim/SUT paths."
