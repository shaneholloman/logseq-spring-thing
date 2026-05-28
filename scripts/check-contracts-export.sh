#!/usr/bin/env bash
# scripts/check-contracts-export.sh
#
# CI guard for `crates/visionclaw-contracts`.
#
# Asserts:
#   1. Every public `struct` / `enum` declared under `crates/visionclaw-contracts/src/`
#      also derives `TS` (gated by `feature = "typescript-export"`).
#   2. The generated `.d.ts` files under `crates/visionclaw-contracts/bindings/`
#      regenerate byte-identically — i.e. nobody has edited them by hand.
#   3. `sdk/visionclaw-contracts/bindings/` matches the freshly-generated
#      bindings byte-for-byte (the npm package mirror is in sync).
#
# This script is the contracts-crate complement to
# `cargo xtask check-no-enterprise` (ADR-10 §D7). The two run side-by-side
# in CI; PRs that trip either are blocked.
#
# Exit codes:
#   0  — all checks pass
#   1  — a public type is missing `derive(TS)`
#   2  — bindings drift detected (re-run with --features typescript-export)
#   3  — npm-package mirror drift detected (run `cd sdk/visionclaw-contracts && npm run sync`)

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CRATE="$ROOT/crates/visionclaw-contracts"
SRC="$CRATE/src"
BINDINGS="$CRATE/bindings"
NPM_BINDINGS="$ROOT/sdk/visionclaw-contracts/bindings"

red()    { printf '\033[31m%s\033[0m\n' "$*" >&2; }
green()  { printf '\033[32m%s\033[0m\n' "$*"; }

# ----------------------------------------------------------------------------
# Step 1: every `pub struct` / `pub enum` in src/ must derive TS
# ----------------------------------------------------------------------------
#
# Heuristic: for each line matching `pub (struct|enum) <Name>`, scan upward
# for a `derive(...TS...)` attribute within the preceding 10 lines. Allow
# either `derive(TS)` or `cfg_attr(feature = "typescript-export", derive(TS), ...)`.

missing=()
while IFS=: read -r file line _; do
    name="$(awk "NR==$line" "$file" | sed -E 's/^[[:space:]]*pub (struct|enum)[[:space:]]+([A-Za-z0-9_]+).*/\2/')"
    # Look at the 10 lines above the declaration for a TS derive.
    start=$((line - 10))
    [[ "$start" -lt 1 ]] && start=1
    if ! awk "NR>=$start && NR<$line" "$file" | grep -qE '\bderive\([^)]*\bTS\b'; then
        missing+=("$file:$line:$name")
    fi
done < <(grep -nE '^[[:space:]]*pub (struct|enum)[[:space:]]+[A-Za-z0-9_]+' "$SRC"/*.rs || true)

if [[ ${#missing[@]} -gt 0 ]]; then
    red "[contracts] Public types missing #[derive(TS)] (ts-rs export):"
    for m in "${missing[@]}"; do
        red "  - $m"
    done
    red ""
    red "Every cross-boundary contract MUST be exportable to TypeScript so"
    red "JavaScript consumers (agentbox, forum, the React control panel) get"
    red "byte-identical types. Add:"
    red ""
    red "    #[cfg_attr(feature = \"typescript-export\", derive(TS), ts(export))]"
    red ""
    red "to each flagged type."
    exit 1
fi
green "[contracts] All public types derive TS."

# ----------------------------------------------------------------------------
# Step 2: regenerate bindings and assert byte-identical to committed copy
# ----------------------------------------------------------------------------

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT
mkdir -p "$tmpdir/before"
cp -r "$BINDINGS"/. "$tmpdir/before/"

cd "$CRATE"
# `ts_export` test writes to ./bindings — re-run it.
cargo test --features typescript-export --test ts_export --quiet >/dev/null

# Use git diff --no-index for portability (no `diff` binary in some envs).
if ! git --no-pager diff --no-index --quiet "$tmpdir/before" "$BINDINGS" >/dev/null 2>&1; then
    red "[contracts] Generated .d.ts files drift from committed copy."
    red ""
    git --no-pager diff --no-index "$tmpdir/before" "$BINDINGS" >&2 || true
    red ""
    red "Re-run \`cargo test -p visionclaw-contracts --features typescript-export ts_export\`"
    red "and commit the updated bindings."
    exit 2
fi
green "[contracts] Bindings byte-identical to committed copy."

# ----------------------------------------------------------------------------
# Step 3: assert the npm-package mirror matches
# ----------------------------------------------------------------------------

if [[ -d "$NPM_BINDINGS" ]]; then
    if ! git --no-pager diff --no-index --quiet "$BINDINGS" "$NPM_BINDINGS" >/dev/null 2>&1; then
        red "[contracts] npm-package mirror drift detected:"
        red "  crate:  $BINDINGS"
        red "  npm:    $NPM_BINDINGS"
        red ""
        red "Run:"
        red "    cd sdk/visionclaw-contracts && npm run sync"
        red "to re-sync, then commit."
        exit 3
    fi
    green "[contracts] npm-package mirror in sync."
else
    red "[contracts] WARNING: sdk/visionclaw-contracts/bindings/ missing — skipping mirror check."
fi

green "[contracts] All checks passed."
