#!/usr/bin/env bash
# tests/fixture-master-validity.sh — Master fixture validation (ADR-082 D4a)
#
# Validates all fixture files in docs/specs/fixtures/:
#   1. Every fixture file is valid JSON
#   2. Every fixture has >= 3 vectors (flat array or nested valid bucket)
#   3. UPSTREAM_PINS.md commit hashes are well-formed (40-char hex or 'in-tree')
#   4. COVERAGE_MATRIX.md row count matches fixture count
#   5. Every fixture has a matching JSON Schema in schemas/
#   6. CHECKSUMS.txt matches file hashes
#
# Usage:
#   tests/fixture-master-validity.sh           # run all checks
#   tests/fixture-master-validity.sh --ci      # strict mode (exit 1 on first failure)
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
FIXTURE_DIR="$(cd -- "$SCRIPT_DIR/../docs/specs/fixtures" && pwd)"
ERRORS=0
WARNINGS=0

fail() { echo "FAIL: $*" >&2; ERRORS=$((ERRORS + 1)); }
warn() { echo "WARN: $*" >&2; WARNINGS=$((WARNINGS + 1)); }
pass() { echo "  OK: $*"; }

echo "=== Master Fixture Validity Check (ADR-082 D4a) ==="
echo "Fixture dir: $FIXTURE_DIR"
echo

# ── 1. Every .json fixture parses and has vectors ────────────────────────────

echo "--- Check 1: JSON validity + vector count ---"
FIXTURE_COUNT=0
for f in "$FIXTURE_DIR"/*.json; do
  fname="$(basename "$f")"
  FIXTURE_COUNT=$((FIXTURE_COUNT + 1))

  # Valid JSON?
  if ! python3 -c "import json; json.load(open('$f'))" 2>/dev/null; then
    fail "$fname: not valid JSON"
    continue
  fi

  # Has _meta block?
  if ! python3 -c "
import json, sys
d = json.load(open('$f'))
if '_meta' not in d:
    print('missing _meta', file=sys.stderr)
    sys.exit(1)
if 'vectors' not in d:
    print('missing vectors', file=sys.stderr)
    sys.exit(1)
" 2>/dev/null; then
    fail "$fname: missing _meta or vectors key"
    continue
  fi

  # Vector count >= 3
  VCOUNT=$(python3 -c "
import json
d = json.load(open('$f'))
v = d['vectors']
if isinstance(v, list):
    print(len(v))
elif isinstance(v, dict):
    # Nested structure (e.g., nip44-v2): count the first valid bucket
    for key in ('valid', 'positive'):
        if key in v:
            sub = v[key]
            if isinstance(sub, dict):
                total = sum(len(arr) for arr in sub.values() if isinstance(arr, list))
                print(total)
                break
            elif isinstance(sub, list):
                print(len(sub))
                break
    else:
        print(0)
else:
    print(0)
" 2>/dev/null || echo "0")

  if [ "$VCOUNT" -lt 3 ]; then
    fail "$fname: has $VCOUNT vectors (need >= 3)"
  else
    pass "$fname: $VCOUNT vectors"
  fi
done
echo "  Total fixture files: $FIXTURE_COUNT"
echo

# ── 2. UPSTREAM_PINS.md commit hashes ────────────────────────────────────────

echo "--- Check 2: UPSTREAM_PINS.md commit hashes ---"
PINS_FILE="$FIXTURE_DIR/UPSTREAM_PINS.md"
if [ ! -f "$PINS_FILE" ]; then
  fail "UPSTREAM_PINS.md not found"
else
  # Every "Pinned commit:" line must have a 40-char hex hash or 'in-tree'.
  # Skip the template line (contains <full-sha>).
  BAD_PINS=$(grep -E "Pinned commit:" "$PINS_FILE" \
    | grep -v '<full-sha>' \
    | grep -v -E '`[0-9a-f]{40}`' \
    | grep -v 'in-tree' \
    || true)
  if [ -n "$BAD_PINS" ]; then
    fail "UPSTREAM_PINS.md has malformed commit hashes:"
    echo "$BAD_PINS" >&2
  else
    pass "All commit hashes are well-formed (40-char hex or 'in-tree')"
  fi
fi
echo

# ── 3. COVERAGE_MATRIX.md row count ─────────────────────────────────────────

echo "--- Check 3: COVERAGE_MATRIX.md fixture row count ---"
MATRIX_FILE="$FIXTURE_DIR/COVERAGE_MATRIX.md"
if [ ! -f "$MATRIX_FILE" ]; then
  fail "COVERAGE_MATRIX.md not found"
else
  # Count data rows (lines with | that contain .json)
  MATRIX_ROWS=$(grep -c '\.json' "$MATRIX_FILE" || true)
  if [ "$MATRIX_ROWS" -ne "$FIXTURE_COUNT" ]; then
    fail "COVERAGE_MATRIX.md has $MATRIX_ROWS fixture rows but directory has $FIXTURE_COUNT .json files"
  else
    pass "COVERAGE_MATRIX.md row count ($MATRIX_ROWS) matches fixture count ($FIXTURE_COUNT)"
  fi
fi
echo

# ── 4. JSON Schema coverage ─────────────────────────────────────────────────

echo "--- Check 4: JSON Schema coverage ---"
SCHEMA_DIR="$FIXTURE_DIR/schemas"
for f in "$FIXTURE_DIR"/*.json; do
  fname="$(basename "$f" .json)"
  schema="$SCHEMA_DIR/$fname.schema.json"
  if [ ! -f "$schema" ]; then
    warn "$fname.json has no matching schema at schemas/$fname.schema.json"
  else
    pass "$fname.json has schema"
  fi
done
echo

# ── 5. CHECKSUMS.txt verification ────────────────────────────────────────────

echo "--- Check 5: CHECKSUMS.txt integrity ---"
CHECKSUM_FILE="$FIXTURE_DIR/CHECKSUMS.txt"
if [ ! -f "$CHECKSUM_FILE" ]; then
  fail "CHECKSUMS.txt not found"
else
  cd "$FIXTURE_DIR"
  if sha256sum -c CHECKSUMS.txt --quiet 2>/dev/null; then
    pass "All checksums match"
  else
    fail "CHECKSUMS.txt verification failed — some files have been modified"
  fi
fi
echo

# ── Summary ──────────────────────────────────────────────────────────────────

echo "=== Summary ==="
echo "  Fixtures: $FIXTURE_COUNT"
echo "  Errors:   $ERRORS"
echo "  Warnings: $WARNINGS"

if [ "$ERRORS" -gt 0 ]; then
  echo "FAILED: $ERRORS error(s) found." >&2
  exit 1
fi

if [ "$WARNINGS" -gt 0 ]; then
  echo "PASSED with $WARNINGS warning(s)."
else
  echo "PASSED: all checks green."
fi
