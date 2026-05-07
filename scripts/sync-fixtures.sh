#!/usr/bin/env bash
# scripts/sync-fixtures.sh — VisionClaw substrate
#
# Per ADR-082 D5: VisionClaw is the *master* host of cross-substrate test
# fixtures (master files at docs/specs/fixtures/). For VisionClaw itself this
# script is a no-op self-check: it validates that the master files are
# schema-valid and CHECKSUMS.txt is consistent. Other substrates run their
# own sync-fixtures.sh which clones VisionClaw and copies docs/specs/fixtures/
# into their tests/fixtures/ tree.
#
# Usage:
#   scripts/sync-fixtures.sh           # validate master fixtures
#   scripts/sync-fixtures.sh --verify  # CI gate: exit non-zero on any drift
set -euo pipefail

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
FIXTURES_DIR="$REPO_ROOT/docs/specs/fixtures"

if [ ! -d "$FIXTURES_DIR" ]; then
  echo "ERROR: fixtures dir missing at $FIXTURES_DIR" >&2
  exit 1
fi

cd "$FIXTURES_DIR"

# 1) Master CHECKSUMS.txt must be consistent with on-disk files.
if [ ! -f CHECKSUMS.txt ]; then
  echo "ERROR: CHECKSUMS.txt missing — run \`sha256sum *.json README.md UPSTREAM_PINS.md COVERAGE_MATRIX.md > CHECKSUMS.txt\`" >&2
  exit 1
fi
sha256sum -c CHECKSUMS.txt --quiet

# 2) Each Phase-0 vendored fixture must parse as JSON with ≥3 vectors.
for f in nip44-v2.json bip340-schnorr.json rfc8785-jcs.json; do
  if [ ! -s "$f" ]; then
    echo "ERROR: fixture $f missing or empty" >&2
    exit 1
  fi
  python3 - <<PY
import json, sys
with open("$f") as fh:
    d = json.load(fh)
v = d.get("vectors")
if v is None:
    print("$f: missing 'vectors' key", file=sys.stderr); sys.exit(1)
# vectors may be dict (nested) or list — both must be non-empty.
if isinstance(v, dict):
    total = sum(len(x) if isinstance(x, list) else
                sum(len(y) for y in x.values() if isinstance(y, list))
                for x in v.values())
    ok = total >= 3
elif isinstance(v, list):
    ok = len(v) >= 3
else:
    ok = False
if not ok:
    print("$f: vectors must contain ≥3 entries", file=sys.stderr); sys.exit(1)
PY
done

# 3) UPSTREAM_PINS.md commit hashes must be 40-char hex (or marked TBD with note).
if grep -E "Pinned commit: \`[0-9a-f]{1,39}\`" UPSTREAM_PINS.md > /dev/null; then
  echo "ERROR: UPSTREAM_PINS.md has malformed commit hash (must be 40 hex chars)" >&2
  exit 1
fi

case "${1:-}" in
  --verify)
    echo "OK: master fixtures schema-valid, checksums consistent."
    ;;
  *)
    echo "VisionClaw is the fixture master — nothing to sync."
    echo "Master fixtures: $FIXTURES_DIR"
    ;;
esac
