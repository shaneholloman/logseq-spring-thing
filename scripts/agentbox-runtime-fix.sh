#!/bin/bash
# agentbox-runtime-fix.sh — Patches known runtime issues in the agentbox container
# Run after every container restart until these are fixed upstream in the Nix flake.
#
# Issues fixed:
#   1. /usr/bin/env missing (nix2container doesn't include FHS paths)
#   2. NoopTracerProvider constructor bug (OpenTelemetry version mismatch)
#   3. Clears FATAL supervisord state from initial startup race
set -euo pipefail

CYAN='\033[0;36m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${CYAN}=== Agentbox Runtime Fix ===${NC}"

# 1. Ensure /usr/bin/env exists (should be from entrypoint, but verify)
echo -e "${CYAN}[1/3] Checking /usr/bin/env...${NC}"
docker exec agentbox bash -c '
  if [ ! -f /usr/bin/env ]; then
    mkdir -p /usr/bin
    ln -sf $(which env) /usr/bin/env
    echo "CREATED /usr/bin/env symlink"
  else
    echo "/usr/bin/env exists"
  fi
'

# 2. Patch NoopTracerProvider bug in all tracing.js files
echo -e "${CYAN}[2/3] Patching OpenTelemetry tracing bug...${NC}"
docker exec agentbox bash -c '
  patched=0
  for f in $(find /nix/store -name tracing.js -path "*/observability/*" 2>/dev/null); do
    if grep -q "function initTracing()" "$f" && ! grep -q "_initTracing" "$f"; then
      chmod u+w "$f" 2>/dev/null || true
      sed -i "s/function initTracing/function initTracing() { return; } function _initTracing/" "$f" 2>/dev/null && patched=$((patched+1))
    fi
  done
  echo "Patched $patched tracing.js files"
'

# 3. Clear FATAL state and restart all services
echo -e "${CYAN}[3/3] Clearing FATAL services and restarting...${NC}"
docker exec agentbox bash -c '
  # Get list of FATAL services
  fatal=$(supervisorctl status 2>/dev/null | grep FATAL | awk "{print \$1}")
  if [ -z "$fatal" ]; then
    echo "No FATAL services"
    exit 0
  fi

  # Remove all FATAL services
  for svc in $fatal; do
    supervisorctl remove $svc 2>/dev/null
  done

  sleep 1

  # Re-add them
  supervisorctl reread 2>/dev/null
  supervisorctl update 2>/dev/null

  sleep 8

  # Report status
  supervisorctl status
'

# 4. Health check
echo ""
echo -e "${CYAN}=== Health Check ===${NC}"
health=$(docker exec agentbox curl -sf http://localhost:9190/health 2>/dev/null)
if [ -n "$health" ]; then
    echo -e "${GREEN}Management API: OK${NC}"
    echo "$health" | python3 -m json.tool 2>/dev/null || echo "$health"
else
    echo -e "${YELLOW}Management API: not responding yet${NC}"
fi
