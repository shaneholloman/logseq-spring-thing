#!/usr/bin/env bash
# scripts/rename-visionflow-to-visionclaw.sh
#
# Rename the *substrate* workspace crates from the `visionflow-*` prefix to
# `visionclaw-*`. Per the user's clarification: `visionclaw` is the substrate
# (server, persistence, GPU pipeline) and `VisionFlow` is the meta ecosystem
# (product/brand visible to users — UI page title, marketing strings).
#
# Rewrites:
#   - kebab-case  `visionflow-X` → `visionclaw-X`   (TOML, docs, shell, dotenvs)
#   - snake-case  `visionflow_X` → `visionclaw_X`   (Rust use paths, module idents)
#
# Preserves (case-sensitive):
#   - `VisionFlow`  (Pascal/Camel — the product brand)
#   - `visionflow_container`, `visionflow_network` if any
#   - `VISIONFLOW_AGENT_KEY` etc. if present — env var names
#
# Renames `crates/visionflow-*/` directories to `crates/visionclaw-*/`.
#
# Usage: bash scripts/rename-visionflow-to-visionclaw.sh

set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

# The seven crate stems.
STEMS=(domain adapters gpu actors protocol ontology server)

# Word-boundary-anchored excludes so we don't touch agent transcripts etc.
EXCLUDES=(
  '--glob=!.git/**'
  '--glob=!target/**'
  '--glob=!node_modules/**'
  '--glob=!.tmp/**'
  '--glob=!*.lock'
  '--glob=!*.jsonl'
  '--glob=!.agentic-qe/**'
  '--glob=!.claude/worktrees/**'
  '--glob=!rename-webxr-report-*'
  '--glob=!scripts/rename-visionflow-to-visionclaw.sh'
  '--glob=!scripts/rename-webxr-to-visionflow-server.sh'
)

echo "== Phase 1: rewriting kebab-case visionflow-X → visionclaw-X (7 stems) =="
for stem in "${STEMS[@]}"; do
  count_before=$(rg --count-matches "\\bvisionflow-${stem}\\b" "${EXCLUDES[@]}" 2>/dev/null | awk -F: '{s+=$NF} END {print s+0}')
  echo "  visionflow-${stem} → visionclaw-${stem}  (${count_before} hits)"
  rg -l "\\bvisionflow-${stem}\\b" "${EXCLUDES[@]}" 2>/dev/null \
    | xargs -r sed -i -E "s/\\bvisionflow-${stem}\\b/visionclaw-${stem}/g"
done

echo
echo "== Phase 2: rewriting snake-case visionflow_X → visionclaw_X =="
for stem in "${STEMS[@]}"; do
  count_before=$(rg --count-matches "\\bvisionflow_${stem}\\b" "${EXCLUDES[@]}" 2>/dev/null | awk -F: '{s+=$NF} END {print s+0}')
  echo "  visionflow_${stem} → visionclaw_${stem}  (${count_before} hits)"
  rg -l "\\bvisionflow_${stem}\\b" "${EXCLUDES[@]}" 2>/dev/null \
    | xargs -r sed -i -E "s/\\bvisionflow_${stem}\\b/visionclaw_${stem}/g"
done

echo
echo "== Phase 3: renaming crate directories =="
for stem in "${STEMS[@]}"; do
  src="crates/visionflow-${stem}"
  dst="crates/visionclaw-${stem}"
  if [ -d "$src" ]; then
    git mv "$src" "$dst"
    echo "  $src → $dst"
  fi
done

echo
echo "== Done =="
echo
echo "Remaining lowercase 'visionflow' references (should be empty unless"
echo "they're intentional VisionFlow brand or env-var leaks):"
rg -n '\bvisionflow\b|\bvisionflow_|\bvisionflow-' "${EXCLUDES[@]}" 2>/dev/null | head -20 || echo "  (none — clean)"
