#!/usr/bin/env bash
# Build drawer-fx WASM and copy artifacts into the feature directory.
#
#   client/src/wasm/drawer-fx/        -- Rust source
#   client/src/features/enterprise/fx/wasm/  -- generated JS + .wasm (consumed by drawerFx.ts)
#
# Requires: wasm-pack (https://rustwasm.github.io/wasm-pack/installer/)
set -euo pipefail

CRATE_DIR="$(cd "$(dirname "$0")" && pwd)"
OUT_DIR="$CRATE_DIR/../../features/enterprise/fx/wasm"

if ! command -v wasm-pack >/dev/null 2>&1; then
  echo "[drawer-fx] wasm-pack not found. Install via:" >&2
  echo "  curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh" >&2
  exit 1
fi

echo "[drawer-fx] building -> $OUT_DIR"
mkdir -p "$OUT_DIR"
cd "$CRATE_DIR"
wasm-pack build --target web --release --out-dir "$OUT_DIR" --out-name drawer_fx
echo "[drawer-fx] done. Artifacts in $OUT_DIR"
