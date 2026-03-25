#!/usr/bin/env bash
# Daily CLAUDE.md Deduplication via Ruflo Swarm
# Launched as a cron job to keep all CLAUDE.md files efficient and non-redundant.
# Uses a single background Claude Code agent (reviewer) to audit and deduplicate.
set -euo pipefail

LOG="/var/log/claude-md-dedup.log"
WORKSPACE="/home/devuser/workspace/project"
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

echo "[$TIMESTAMP] Starting CLAUDE.md dedup audit..." >> "$LOG"

# Find all CLAUDE.md files in the hierarchy
CLAUDE_FILES=$(find /home/devuser/workspace -name "CLAUDE.md" \
  -not -path "*/node_modules/*" \
  -not -path "*/.lazy/*" \
  -not -path "*/dist/*" \
  -not -path "*/target/*" \
  2>/dev/null | sort)

FILE_COUNT=$(echo "$CLAUDE_FILES" | wc -l)
TOTAL_LINES=$(echo "$CLAUDE_FILES" | xargs wc -l 2>/dev/null | tail -1 | awk '{print $1}')

echo "[$TIMESTAMP] Found $FILE_COUNT CLAUDE.md files, $TOTAL_LINES total lines" >> "$LOG"

# Launch a Claude Code reviewer agent in the background
# The agent reads all CLAUDE.md files, identifies duplication, and fixes it
cd "$WORKSPACE"

claude --dangerously-skip-permissions -p "You are a CLAUDE.md deduplication agent. Your task:

1. Read ALL these CLAUDE.md files:
$(echo "$CLAUDE_FILES" | sed 's/^/   - /')

2. Claude Code loads them hierarchically (parent -> child). Content in a parent is ALREADY available to all children. Identify any content that appears in more than one file.

3. For each duplicate:
   - Keep it in the HIGHEST appropriate file (closest to root that makes sense)
   - Remove it from child files, replacing with a one-line reference if needed
   - Never remove content that is genuinely specific to the child's scope

4. Apply the edits. Use Edit tool (not Write) to make surgical changes.

5. After all edits, read each file again and verify:
   - No section appears in more than one file
   - No file lost unique content
   - All files still parse correctly
   - Total line count is less than or equal to before ($TOTAL_LINES lines)

6. Report: list what you changed and the new line counts.

CONSTRAINT: Changes must be NET POSITIVE. If you cannot improve the situation, make no changes. Quality over quantity." \
  --max-turns 30 \
  >> "$LOG" 2>&1 || true

NEW_TOTAL=$(echo "$CLAUDE_FILES" | xargs wc -l 2>/dev/null | tail -1 | awk '{print $1}')
echo "[$TIMESTAMP] Dedup complete. Before: $TOTAL_LINES lines, After: $NEW_TOTAL lines" >> "$LOG"

# Verify net positive (no content loss)
if [[ "$NEW_TOTAL" -gt "$TOTAL_LINES" ]]; then
  echo "[$TIMESTAMP] WARNING: Line count increased ($TOTAL_LINES -> $NEW_TOTAL). Review manually." >> "$LOG"
fi
