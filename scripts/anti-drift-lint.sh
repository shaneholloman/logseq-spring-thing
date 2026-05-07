#!/usr/bin/env bash
# scripts/anti-drift-lint.sh — ADR-077 P3 anti-drift lint
#
# Rejects:
#   1. Ad-hoc URN minting outside the canonical mint module: any source file
#      under src/ (excluding src/uri/) that contains a string literal matching
#      `urn:visionclaw:` in a `format!`/`println!`/string-concat construction
#      bypasses the canonical grammar enforced in src/uri/mint.rs (ADR-013 +
#      ADR-077 P3). Tests are exempted.
#   2. Hand-rolled DreamLab-only Schnorr verification key suite identifiers
#      (NostrSchnorrKey2024, SchnorrSecp256k1VerificationKey2022 / 2025): the
#      canonical W3C suite is SchnorrSecp256k1VerificationKey2019 (ADR-074 D1).
#
# Exit code 0 = clean, 1 = drift detected.

set -euo pipefail

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

EXIT=0

# --- Rule 1: ad-hoc urn:visionclaw minting outside src/uri/ ----------------
# Match common minting constructs. Allow:
#   - src/uri/                  (canonical mint site)
#   - src/handlers/uri_resolver_handler.rs  (parses, doesn't mint)
#   - tests / docs / comments
ADHOC_URN=$(
  grep -RIn --include='*.rs' \
    -E '(format!|write!|writeln!|println!|to_string\(\) \+ "urn:visionclaw:|format_args!).*"urn:visionclaw:' \
    src 2>/dev/null \
    | grep -v '^src/uri/' \
    | grep -v '^src/handlers/uri_resolver_handler.rs' \
    | grep -v '/tests?/' \
    | grep -v '//.*urn:visionclaw' \
    || true
)

if [ -n "$ADHOC_URN" ]; then
  echo "::error::ADR-077 P3: ad-hoc urn:visionclaw: minting found outside src/uri/. Use src/uri/mint.rs."
  echo "$ADHOC_URN"
  EXIT=1
fi

# --- Rule 2: stale Schnorr suite identifiers in Rust + JS + Python --------
# These are spec-drift fabrications; the canonical W3C identifier is
# SchnorrSecp256k1VerificationKey2019 (ADR-074 D1).
#
# Match ONLY string-literal occurrences (preceded by `"` or `'`) so that
# documentation comments explaining "the previous X was a fabrication" don't
# false-positive. We also require the trailing closing quote so partial-word
# matches inside larger identifiers don't trip.
STALE_SUITES=$(
  grep -RIn \
    --include='*.rs' --include='*.js' --include='*.ts' --include='*.py' --include='*.json' \
    -E '["'"'"'](NostrSchnorrKey2024|SchnorrSecp256k1VerificationKey20(2[245]|26))["'"'"']' \
    . 2>/dev/null \
    | grep -v '/target/' \
    | grep -v 'node_modules' \
    | grep -v '/docs/' \
    | grep -v '/scripts/anti-drift-lint.sh' \
    | grep -v '\.lock\b' \
    || true
)

if [ -n "$STALE_SUITES" ]; then
  echo "::error::ADR-074 D1: stale Schnorr verification suite identifier in source."
  echo "Canonical: SchnorrSecp256k1VerificationKey2019"
  echo "$STALE_SUITES"
  EXIT=1
fi

if [ $EXIT -eq 0 ]; then
  echo "anti-drift lint: clean."
fi
exit $EXIT
