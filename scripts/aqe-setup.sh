#!/usr/bin/env bash
# AQE + Ruflo coexistence setup for agentbox container
# Applies repair kit patches for session init, memory backend, and hook coexistence.
#
# Run once after agentbox rebuild, or source from session-start hooks.

set -euo pipefail

AQE_BIN=$(command -v aqe 2>/dev/null || true)
RUFLO_BIN=$(command -v claude-flow 2>/dev/null || true)

if [ -z "$AQE_BIN" ]; then
  echo "[aqe-setup] ERROR: aqe not found in PATH" >&2
  exit 1
fi

AQE_NIX_BASE=$(dirname "$(dirname "$(readlink -f "$AQE_BIN")")")/lib/agentic-qe
MCP_BUNDLE="$AQE_NIX_BASE/dist/mcp/bundle.js"

if [ ! -f "$MCP_BUNDLE" ]; then
  echo "[aqe-setup] ERROR: AQE MCP bundle not found at $MCP_BUNDLE" >&2
  exit 1
fi

echo "[aqe-setup] AQE $(aqe --version 2>/dev/null || echo 'unknown')"
echo "[aqe-setup] Ruflo $(claude-flow --version 2>/dev/null || echo 'not installed')"

# 1. Export env vars for AQE in-memory backend (bypass broken native deps)
export AQE_MEMORY_BACKEND=memory
export AQE_VERBOSE=false
export NODE_NO_WARNINGS=1

# 2. Verify ruflo daemon is running (ruflo owns persistent memory via RuVector PG)
if [ -n "$RUFLO_BIN" ]; then
  if claude-flow doctor 2>&1 | grep -q "Daemon Status: Running"; then
    echo "[aqe-setup] Ruflo daemon: running"
  else
    echo "[aqe-setup] WARNING: Ruflo daemon not running. Starting..." >&2
    claude-flow daemon start 2>/dev/null || true
  fi
fi

# 3. Verify AQE MCP bundle starts cleanly
if timeout 5 node "$MCP_BUNDLE" </dev/null 2>&1 | grep -q "MCP.*Ready"; then
  echo "[aqe-setup] AQE MCP server: verified"
else
  echo "[aqe-setup] WARNING: AQE MCP server did not reach Ready state within 5s" >&2
fi

# 4. Check workspace .mcp.json has AQE registered
WORKSPACE_MCP="/home/devuser/workspace/.mcp.json"
if [ -f "$WORKSPACE_MCP" ] && grep -q "agentic-qe" "$WORKSPACE_MCP"; then
  echo "[aqe-setup] AQE MCP registration: present in $WORKSPACE_MCP"
else
  echo "[aqe-setup] WARNING: AQE not registered in $WORKSPACE_MCP" >&2
  echo "[aqe-setup]   Add to .mcp.json: {\"command\":\"node\",\"args\":[\"$MCP_BUNDLE\"],\"env\":{\"AQE_MEMORY_BACKEND\":\"memory\"}}" >&2
fi

# 5. Summary
echo ""
echo "[aqe-setup] Configuration:"
echo "  AQE_MEMORY_BACKEND=memory (in-memory, session-scoped)"
echo "  Persistent memory: ruflo → RuVector PG (ruvector-postgres:5432)"
echo "  AQE MCP: 86 tools (fleet/reasoning degraded without native deps)"
echo "  Native deps (better-sqlite3, hnswlib-node): unavailable in Nix"
echo ""
echo "[aqe-setup] Done."
