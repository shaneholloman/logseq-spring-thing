#!/usr/bin/env bash
# Regression smoke test: Cross-Origin Isolation headers on Vite dev server.
#
# Covers BUG #1: the module worker at
#   /src/features/graph/workers/graph.worker.ts
# fails silently if its response lacks `Cross-Origin-Embedder-Policy` under a
# `credentialless` parent context. `nginx.dev.conf` re-adds COEP per-location
# because Vite's dev server does not emit it. This test guards against future
# nginx refactors losing that per-location re-declaration.
#
# Exits 0 if all required headers are present on both the HTML root and the
# module worker URL. Exits non-zero on the first mismatch so it plugs straight
# into CI without needing a framework.
#
# Usage:
#   BASE_URL=http://localhost:3001 tests/smoke/nginx-coep-headers.sh
#   (BASE_URL defaults to http://localhost:3001 — the dev nginx listen addr)

set -euo pipefail

BASE_URL="${BASE_URL:-http://localhost:3001}"

# URLs to probe. The worker URL reflects the path used by
# graphWorkerProxy.ts when constructing `new Worker(new URL(...), { type: 'module' })`.
ROOT_URL="${BASE_URL}/"
WORKER_URL="${BASE_URL}/src/features/graph/workers/graph.worker.ts"

# Required headers and their expected values.
# COEP must be `credentialless` (not `require-corp`) because the app loads
# cross-origin subresources (CDN fonts, WS etc.) that do not ship CORP.
declare -a REQUIRED=(
  "Cross-Origin-Opener-Policy: same-origin"
  "Cross-Origin-Embedder-Policy: credentialless"
  "Cross-Origin-Resource-Policy: same-origin"
)

# Pretty printing
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m'

fail_count=0

probe() {
  local url="$1"
  local label="$2"

  echo "-- Probing ${label}: ${url}"

  # -s silent, -S show errors, -I head-only, --max-time safety, -k permissive TLS
  local headers
  if ! headers="$(curl -sSI --max-time 5 -k "${url}" 2>&1)"; then
    echo -e "${RED}FAIL${NC} ${label}: curl failed -- ${headers}"
    return 1
  fi

  # HTTP status check — accept 2xx and 304. Worker may 200 or 304 depending
  # on Vite caching. Anything else is a smoke-test failure.
  local status
  status="$(printf '%s\n' "${headers}" | awk 'NR==1{print $2}')"
  case "${status}" in
    2??|304) ;;
    *)
      echo -e "${RED}FAIL${NC} ${label}: unexpected HTTP status ${status}"
      return 1
      ;;
  esac

  local local_fail=0
  for spec in "${REQUIRED[@]}"; do
    local name="${spec%%:*}"
    local want="${spec#*: }"
    # Headers are case-insensitive — grep -i matches both `Cross-Origin-*`
    # and any lowercased variant a proxy might emit.
    local line
    line="$(printf '%s\n' "${headers}" | grep -i "^${name}:" | tr -d '\r' || true)"

    if [[ -z "${line}" ]]; then
      echo -e "  ${RED}MISSING${NC} ${name} on ${label}"
      local_fail=$((local_fail + 1))
      continue
    fi

    local got
    got="$(printf '%s' "${line}" | awk -F': ' '{print $2}' | awk '{$1=$1};1')"

    if [[ "${got}" != "${want}" ]]; then
      echo -e "  ${RED}MISMATCH${NC} ${name} on ${label}: want '${want}', got '${got}'"
      local_fail=$((local_fail + 1))
    else
      echo -e "  ${GREEN}OK${NC} ${name}: ${got}"
    fi
  done

  return "${local_fail}"
}

if ! probe "${ROOT_URL}" "html-root"; then
  fail_count=$((fail_count + $?))
fi
if ! probe "${WORKER_URL}" "module-worker"; then
  fail_count=$((fail_count + $?))
fi

echo
if [[ "${fail_count}" -eq 0 ]]; then
  echo -e "${GREEN}PASS${NC} all COEP/COOP/CORP headers present"
  exit 0
else
  echo -e "${RED}FAIL${NC} ${fail_count} header assertion(s) failed"
  echo -e "${YELLOW}hint:${NC} inspect nginx.dev.conf — each /src/, /@vite/, and /@fs/"
  echo -e "      location must re-add Cross-Origin-* headers after proxy_hide_header."
  exit 1
fi
