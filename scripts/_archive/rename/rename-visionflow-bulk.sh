#!/usr/bin/env bash
# One-shot bulk rename: all case variants of visionflow → visionclaw.
# User accepts that genuine VisionFlow (ecosystem) references can be
# patched back when they surface — easier to fix exceptions than to
# manually distinguish substrate vs ecosystem across 4000+ sites.

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

EXCLUDES=(
  '--glob=!.git/**'
  '--glob=!target/**'
  '--glob=!node_modules/**'
  '--glob=!.tmp/**'
  '--glob=!*.lock'
  '--glob=!*.jsonl'
  '--glob=!.agentic-qe/**'
  '--glob=!.claude/worktrees/**'
  '--glob=!scripts/rename-*'
  '--glob=!rename-*-report-*'
)

# Phase 1: lowercase visionflow → visionclaw
echo "Phase 1: lowercase visionflow → visionclaw"
rg -l 'visionflow' "${EXCLUDES[@]}" 2>/dev/null \
  | xargs -r sed -i 's/visionflow/visionclaw/g'

# Phase 2: CamelCase VisionFlow → VisionClaw
echo "Phase 2: VisionFlow → VisionClaw"
rg -l 'VisionFlow' "${EXCLUDES[@]}" 2>/dev/null \
  | xargs -r sed -i 's/VisionFlow/VisionClaw/g'

# Phase 3: UPPER VISIONFLOW → VISIONCLAW
echo "Phase 3: VISIONFLOW → VISIONCLAW"
rg -l 'VISIONFLOW' "${EXCLUDES[@]}" 2>/dev/null \
  | xargs -r sed -i 's/VISIONFLOW/VISIONCLAW/g'

echo
echo "Remaining references (should be in skipped paths only):"
rg -c 'visionflow' "${EXCLUDES[@]}" 2>/dev/null | wc -l
echo "  lowercase files"
rg -c 'VisionFlow' "${EXCLUDES[@]}" 2>/dev/null | wc -l
echo "  CamelCase files"
rg -c 'VISIONFLOW' "${EXCLUDES[@]}" 2>/dev/null | wc -l
echo "  UPPER files"
