#!/usr/bin/env bash
# scripts/rename-webxr-to-visionflow-server.sh
#
# Project-wide rename: the `webxr` Rust crate / binary name → `visionflow-server`
# (ADR-090 final crate-naming).
#
# Two distinct identifiers need replacing:
#   - kebab-case `webxr`             → `visionflow-server`   (Cargo package, docs, paths)
#   - snake_case `webxr`             → `visionflow_server`   (Rust use paths — `use webxr::…`)
#
# NOTE: there's no separate kebab/snake form for the source token — it's already
# just "webxr". The TARGET differs by context: `visionflow-server` in TOML / docs /
# shell, `visionflow_server` in Rust `use` paths.
#
# Usage:
#   ./scripts/rename-webxr-to-visionflow-server.sh dry-run   # produce a report, NO writes
#   ./scripts/rename-webxr-to-visionflow-server.sh apply     # actually rename
#
# The report lists every match with file:line so you can spot the genuinely
# infrastructure-named "webxr"s (e.g. docker hostname, project history) before
# the apply pass.

set -euo pipefail

MODE="${1:-dry-run}"
ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

REPORT="rename-webxr-report-$(date -u +%Y%m%dT%H%M%SZ).md"

# ---------------------------------------------------------------------------
# Exclusions: paths whose `webxr` references are intentional infrastructure
# names or historical artefacts that should NOT be auto-rewritten.
# ---------------------------------------------------------------------------
EXCLUDES=(
  '--glob=!.git/**'
  '--glob=!target/**'
  '--glob=!node_modules/**'
  '--glob=!client/dist/**'
  '--glob=!.tmp/**'
  '--glob=!.claude/worktrees/**'         # ephemeral worktrees
  '--glob=!*.log'
  '--glob=!*.jsonl'                       # agent transcripts
  '--glob=!*.lock'                        # Cargo.lock / yarn.lock — regenerated
  '--glob=!.agentic-qe/**'                # session caches
  '--glob=!logs/**'
  '--glob=!screenshots/**'
)

# Tokens we MIGHT want to preserve (review in the dry-run, decide per-line):
#   - "webxr_container"             docker container literal name (infra)
#   - "HOSTNAME=webxr"              docker compose hostname (infra)
#   - "WebXR" / "Web XR"            the W3C standard name — DO NOT rewrite
#   - "webxr-rs" / "@webxr/*"       upstream crate / npm package names — DO NOT rewrite
#   - "WebXRScene.tsx"              client filename (might be intentional WebXR-standard usage)
#
# These are caught by the report so you can grep them out before apply.

# ---------------------------------------------------------------------------
# Phase 1: gather matches
# ---------------------------------------------------------------------------
echo "## Rename report: webxr → visionflow-server" > "$REPORT"
echo "" >> "$REPORT"
echo "Generated: $(date -u +%Y-%m-%dT%H:%M:%SZ)" >> "$REPORT"
echo "Mode: $MODE" >> "$REPORT"
echo "" >> "$REPORT"

# A. Snake-case `webxr` in Rust `use` paths and `extern crate` declarations
echo "### A. Rust import paths (\`use webxr::…\`, \`extern crate webxr\`)" >> "$REPORT"
echo "" >> "$REPORT"
echo '```' >> "$REPORT"
rg -n '(\buse webxr::|\bextern crate webxr\b|::webxr::)' --type=rust "${EXCLUDES[@]}" >> "$REPORT" 2>/dev/null || echo "(none)" >> "$REPORT"
echo '```' >> "$REPORT"
echo "" >> "$REPORT"

# B. Cargo.toml package + dependency declarations
echo "### B. Cargo.toml package / dependency keys" >> "$REPORT"
echo "" >> "$REPORT"
echo '```' >> "$REPORT"
rg -n '(^name = "webxr"|^webxr = |"webxr"|\[\[bin\]\][^[]*name = "webxr"|\[\[test\]\][^[]*"webxr")' --type=toml "${EXCLUDES[@]}" >> "$REPORT" 2>/dev/null || echo "(none)" >> "$REPORT"
echo '```' >> "$REPORT"
echo "" >> "$REPORT"

# C. Shell scripts (build, deploy, launch)
echo "### C. Shell scripts" >> "$REPORT"
echo "" >> "$REPORT"
echo '```' >> "$REPORT"
rg -n '\bwebxr\b' --type=sh --type=fish "${EXCLUDES[@]}" >> "$REPORT" 2>/dev/null || echo "(none)" >> "$REPORT"
echo '```' >> "$REPORT"
echo "" >> "$REPORT"

# D. Dotenv / config files
echo "### D. Dotenv / TOML / YAML / config" >> "$REPORT"
echo "" >> "$REPORT"
echo '```' >> "$REPORT"
rg -n '\bwebxr\b' --glob='*.env' --glob='.env*' --glob='*.yml' --glob='*.yaml' --glob='*.conf' --glob='*.ini' "${EXCLUDES[@]}" >> "$REPORT" 2>/dev/null || echo "(none)" >> "$REPORT"
echo '```' >> "$REPORT"
echo "" >> "$REPORT"

# E. Docker / supervisord
echo "### E. Docker / supervisord / compose" >> "$REPORT"
echo "" >> "$REPORT"
echo "REVIEW CAREFULLY: \`webxr\` as a docker hostname or container name might be infra-canonical, not a rename target." >> "$REPORT"
echo "" >> "$REPORT"
echo '```' >> "$REPORT"
rg -n '\bwebxr\b' --glob='Dockerfile*' --glob='docker-compose*.yml' --glob='supervisord*.conf' "${EXCLUDES[@]}" >> "$REPORT" 2>/dev/null || echo "(none)" >> "$REPORT"
echo '```' >> "$REPORT"
echo "" >> "$REPORT"

# F. Documentation (Markdown / ADRs / README / CLAUDE.md)
echo "### F. Documentation (md / adoc / txt)" >> "$REPORT"
echo "" >> "$REPORT"
echo '```' >> "$REPORT"
rg -n '\bwebxr\b' --type=md --glob='*.adoc' --glob='*.txt' "${EXCLUDES[@]}" >> "$REPORT" 2>/dev/null || echo "(none)" >> "$REPORT"
echo '```' >> "$REPORT"
echo "" >> "$REPORT"

# G. Client TS/TSX/JS
echo "### G. Client TypeScript / JavaScript" >> "$REPORT"
echo "" >> "$REPORT"
echo "REVIEW CAREFULLY: \"WebXR\" (capitalised) is the W3C standard name and probably should stay. Lower-case 'webxr' in TS likely refers to your binary." >> "$REPORT"
echo "" >> "$REPORT"
echo '```' >> "$REPORT"
rg -n '\bwebxr\b' --type=ts --type=tsx --type=js --type=jsx "${EXCLUDES[@]}" >> "$REPORT" 2>/dev/null || echo "(none)" >> "$REPORT"
echo '```' >> "$REPORT"
echo "" >> "$REPORT"

# H. Python / agent prompt files
echo "### H. Python + agent chat / prompt files" >> "$REPORT"
echo "" >> "$REPORT"
echo '```' >> "$REPORT"
rg -n '\bwebxr\b' --type=py "${EXCLUDES[@]}" >> "$REPORT" 2>/dev/null || echo "(none)" >> "$REPORT"
echo '```' >> "$REPORT"
echo "" >> "$REPORT"

# I. Everything else (catch-all minus the typed categories above)
echo "### I. Everything else (not covered by A-H)" >> "$REPORT"
echo "" >> "$REPORT"
echo '```' >> "$REPORT"
rg -n '\bwebxr\b' "${EXCLUDES[@]}" \
   --type-not=rust --type-not=toml --type-not=sh --type-not=fish \
   --type-not=md --type-not=ts --type-not=tsx --type-not=js --type-not=jsx --type-not=py \
   --glob='!*.env*' --glob='!*.yml' --glob='!*.yaml' --glob='!*.conf' --glob='!*.ini' \
   --glob='!Dockerfile*' --glob='!docker-compose*.yml' --glob='!supervisord*.conf' \
   --glob='!*.adoc' --glob='!*.txt' \
   2>/dev/null >> "$REPORT" || echo "(none)" >> "$REPORT"
echo '```' >> "$REPORT"
echo "" >> "$REPORT"

# ---------------------------------------------------------------------------
# Tallies
# ---------------------------------------------------------------------------
TOTAL_FILES=$(rg -l '\bwebxr\b' "${EXCLUDES[@]}" 2>/dev/null | wc -l)
TOTAL_HITS=$(rg --count-matches '\bwebxr\b' "${EXCLUDES[@]}" 2>/dev/null | awk -F: '{sum+=$NF} END {print sum}')
echo "## Tallies" >> "$REPORT"
echo "" >> "$REPORT"
echo "- Files containing \`webxr\`: $TOTAL_FILES" >> "$REPORT"
echo "- Total occurrences:          $TOTAL_HITS" >> "$REPORT"
echo "" >> "$REPORT"

# ---------------------------------------------------------------------------
# Apply pass
# ---------------------------------------------------------------------------
if [ "$MODE" = "apply" ]; then
  echo "" >> "$REPORT"
  echo "## Apply pass" >> "$REPORT"
  echo "" >> "$REPORT"
  echo "Applying rewrites…" >&2

  # Docker hostname + cross-repo infra references INCLUDED in this pass
  # — the user opted to tear down and rebuild the docker stack rather
  # than defer to a Pass 2. All `webxr` references (binary name AND
  # docker hostname) become `visionflow-server` in one coordinated
  # rewrite.
  EXCLUDE_INFRA=()

  # Rust use paths: webxr → visionflow_server (snake_case)
  # Use word-boundary regex so we only match the standalone token.
  rg -l '(\buse webxr::|\bextern crate webxr\b|::webxr::|\bwebxr::)' --type=rust "${EXCLUDES[@]}" "${EXCLUDE_INFRA[@]}" 2>/dev/null \
    | xargs -r sed -i -E 's/\b(use|extern crate)\s+webxr\b/\1 visionflow_server/g; s/::webxr::/::visionflow_server::/g; s/(^|[^a-zA-Z0-9_])webxr::/\1visionflow_server::/g'

  # Cargo.toml: webxr → visionflow-server (kebab-case). Word-boundary anchored.
  rg -l '\bwebxr\b' --type=toml "${EXCLUDES[@]}" "${EXCLUDE_INFRA[@]}" 2>/dev/null \
    | xargs -r sed -i -E 's/\bwebxr\b/visionflow-server/g'

  # Shell scripts, Python, Markdown, dotenvs, YAML, conf, ini.
  # Now INCLUDES docker compose, agentbox schema, config.yml — user
  # opted to rebuild the docker stack with the new hostname.
  rg -l '\bwebxr\b' \
     --type=sh --type=fish --type=py --type=md \
     --glob='*.env*' --glob='*.yml' --glob='*.yaml' --glob='*.conf' --glob='*.ini' \
     --glob='*.json' \
     "${EXCLUDES[@]}" 2>/dev/null \
    | xargs -r sed -i -E 's/\bwebxr\b/visionflow-server/g'

  # Safety check: verify we did NOT touch W3C WebXR identifiers
  # (capital case — these are word-boundary-anchored and case-sensitive
  # so our \bwebxr\b regex should never have matched them).
  echo "" >> "$REPORT"
  echo "### Apply safety check" >> "$REPORT"
  echo "" >> "$REPORT"
  echo "W3C WebXR identifiers preserved (capital case, never rewritten):" >> "$REPORT"
  echo '```' >> "$REPORT"
  rg -cn 'WebXR' --type=ts --type=tsx 2>/dev/null | head -5 >> "$REPORT" || echo "(none)" >> "$REPORT"
  echo '```' >> "$REPORT"
  echo "" >> "$REPORT"
  echo "Remaining lowercase \`webxr\` (should be empty after a clean apply):" >> "$REPORT"
  echo '```' >> "$REPORT"
  rg -n '\bwebxr\b' "${EXCLUDES[@]}" --glob='!rename-webxr-report-*.md' --glob='!scripts/rename-webxr-to-visionflow-server.sh' 2>/dev/null | head -20 >> "$REPORT" || echo "(none — clean)" >> "$REPORT"
  echo '```' >> "$REPORT"

  echo "Rewrites applied. Verify with:" >&2
  echo "  cargo check" >&2
  echo "  cargo test --no-run" >&2
  echo "  rg '\\bwebxr\\b' --type-not binary" >&2
  echo "(review remaining matches — they're either infra-canonical or false positives)" >&2

  echo "Done — see $REPORT for the before-state and review the still-matching files." >> "$REPORT"
fi

echo ""
echo "Report written to: $REPORT"
echo "Files: $TOTAL_FILES, occurrences: $TOTAL_HITS"

if [ "$MODE" = "dry-run" ]; then
  echo ""
  echo "Dry run — no files modified. To apply, run:"
  echo "  $0 apply"
fi
