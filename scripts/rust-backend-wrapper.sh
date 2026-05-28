#!/bin/bash
# Wrapper script for rust-backend used by supervisord in development mode.
# Skips cargo entirely when binary is already up-to-date — restarts with no
# source changes take ~1s instead of ~30s.

set -e

export DOCKER_ENV=1

log() {
    echo "[RUST-WRAPPER][$(date '+%Y-%m-%d %H:%M:%S')] $1"
}

# Auto-detect GPU compute capability at runtime.
# ALWAYS prefer runtime detection over .env/compose values — .env may be stale.
DETECTED_ARCH=$(nvidia-smi --query-gpu=compute_cap --format=csv,noheader --id=0 2>/dev/null | head -1 | tr -d '.' | tr -d '[:space:]')
if [ -n "$DETECTED_ARCH" ]; then
    if [ -n "${CUDA_ARCH:-}" ] && [ "$CUDA_ARCH" != "$DETECTED_ARCH" ]; then
        log "WARNING: .env CUDA_ARCH=${CUDA_ARCH} != GPU sm_${DETECTED_ARCH}. Overriding."
    fi
    export CUDA_ARCH="$DETECTED_ARCH"
    log "GPU compute capability: sm_${CUDA_ARCH} (runtime-detected)"
else
    export CUDA_ARCH="${CUDA_ARCH:-75}"
    log "WARNING: nvidia-smi failed, using sm_${CUDA_ARCH}"
fi

> /app/logs/rust-error.log

RUST_BINARY="/app/target/release/visionclaw-server"

if [ "${SKIP_RUST_REBUILD:-false}" != "true" ]; then
    cd /app

    # Check if binary is already newer than all source inputs.
    # If so, skip cargo entirely — saves ~30s of fingerprint scanning per restart.
    NEEDS_BUILD=true
    if [ -f "$RUST_BINARY" ]; then
        BIN_MTIME=$(stat -c %Y "$RUST_BINARY" 2>/dev/null || echo 0)

        # Latest mtime across Rust sources, Cargo manifests, build script, and CUDA kernels
        LATEST_SRC=$(find /app/src /app/crates -name "*.rs" -printf '%T@\n' 2>/dev/null | sort -n | tail -1 | cut -d. -f1)
        LATEST_SRC=${LATEST_SRC:-0}

        for f in /app/Cargo.toml /app/Cargo.lock /app/build.rs; do
            T=$(stat -c %Y "$f" 2>/dev/null || echo 0)
            [ "$T" -gt "$LATEST_SRC" ] && LATEST_SRC=$T
        done

        LATEST_CUDA=$(find /app/src -name "*.cu" -printf '%T@\n' 2>/dev/null | sort -n | tail -1 | cut -d. -f1)
        LATEST_CUDA=${LATEST_CUDA:-0}
        [ "$LATEST_CUDA" -gt "$LATEST_SRC" ] && LATEST_SRC=$LATEST_CUDA

        if [ "$BIN_MTIME" -gt "$LATEST_SRC" ] && [ "$LATEST_SRC" -gt 0 ]; then
            log "Binary is up-to-date (no source changes since last build). Skipping cargo."
            NEEDS_BUILD=false
        fi
    fi

    if [ "$NEEDS_BUILD" = "true" ]; then
        log "Source changes detected — building with cargo..."

        if cargo build --release --features gpu,ontology,dev-auth 2>&1; then
            log "✓ Build succeeded"
        else
            log "ERROR: Build failed. Attempting clean rebuild..."
            cargo clean 2>/dev/null || true
            if cargo build --release --features gpu,ontology,dev-auth 2>&1; then
                log "✓ Clean rebuild succeeded"
            else
                log "FATAL: Clean rebuild also failed"
                exit 1
            fi
        fi
    fi
else
    log "Skipping Rust rebuild (SKIP_RUST_REBUILD=true)"
    RUST_BINARY="/app/visionclaw-server"
fi

if [ ! -f "${RUST_BINARY}" ]; then
    log "ERROR: Rust binary not found at ${RUST_BINARY}"
    exit 1
fi

log "Starting Rust backend from ${RUST_BINARY}..."
exec ${RUST_BINARY}
