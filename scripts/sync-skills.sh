#!/bin/bash
# sync-skills.sh — Canonical skill symlink setup
#
# Single source of truth: agentbox/skills/ (git submodule)
# Run after: git submodule update, agentbox bump, or fresh deploy
#
# Creates relative symlinks in project/.claude/skills/ → agentbox/skills/
# Preserves project-specific skills (QE, v3, etc.) that don't exist in agentbox.
# Clears ~/.claude/skills/ of any agentbox duplicates (host mount drift prevention).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
AGENTBOX_SKILLS="$PROJECT_ROOT/agentbox/skills"
PROJECT_SKILLS="$PROJECT_ROOT/.claude/skills"
GLOBAL_SKILLS="$HOME/.claude/skills"

if [ ! -d "$AGENTBOX_SKILLS" ]; then
    echo "ERROR: agentbox/skills/ not found at $AGENTBOX_SKILLS"
    echo "Run: git submodule update --init"
    exit 1
fi

mkdir -p "$PROJECT_SKILLS" "$GLOBAL_SKILLS"

linked=0
skipped=0
cleaned=0

for entry in "$AGENTBOX_SKILLS"/*/; do
    [ -d "$entry" ] || continue
    skill="$(basename "$entry")"

    # Project-level: relative symlink
    target="$PROJECT_SKILLS/$skill"
    if [ -L "$target" ]; then
        : # already a symlink
    elif [ -d "$target" ]; then
        rm -rf "$target"
        ln -s "../../agentbox/skills/$skill" "$target"
        linked=$((linked + 1))
    else
        ln -s "../../agentbox/skills/$skill" "$target"
        linked=$((linked + 1))
    fi

    # Global: remove any copy (prevents host mount drift)
    gtarget="$GLOBAL_SKILLS/$skill"
    if [ -d "$gtarget" ] || [ -L "$gtarget" ]; then
        rm -rf "$gtarget"
        cleaned=$((cleaned + 1))
    fi
done

# Also handle top-level files (SKILL-DIRECTORY.md, mcp.json, ruvector-config.json)
for f in SKILL-DIRECTORY.md mcp.json ruvector-config.json; do
    src="$AGENTBOX_SKILLS/$f"
    [ -f "$src" ] || continue
    ptarget="$PROJECT_SKILLS/$f"
    if [ ! -L "$ptarget" ]; then
        rm -f "$ptarget"
        ln -s "../../agentbox/skills/$f" "$ptarget"
    fi
    gtarget="$GLOBAL_SKILLS/$f"
    if [ -f "$gtarget" ] || [ -L "$gtarget" ]; then
        rm -f "$gtarget"
    fi
done

echo "sync-skills: linked=$linked, cleaned=$cleaned from global, skipped=$skipped"
echo "Canonical source: agentbox/skills/ ($(ls "$AGENTBOX_SKILLS" | grep -v '\.json$\|\.md$\|\.sample$' | wc -l) skills)"
echo "Project-specific: $(find "$PROJECT_SKILLS" -maxdepth 1 -type d | tail -n +2 | wc -l) real dirs"
