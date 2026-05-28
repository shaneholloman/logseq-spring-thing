#!/bin/bash
# Compile CUDA kernels to PTX for Rust cust library

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

CUDA_SRC="$PROJECT_ROOT/src/utils/visionclaw_unified.cu"
PTX_OUTPUT="$PROJECT_ROOT/target/visionclaw_unified.ptx"

echo "🔧 Compiling CUDA kernels to PTX..."
echo "   Source: $CUDA_SRC"
echo "   Output: $PTX_OUTPUT"

# Check CUDA toolkit
if ! command -v nvcc &> /dev/null; then
    echo "❌ ERROR: nvcc not found. Install CUDA Toolkit 12.4+"
    exit 1
fi

NVCC_VERSION=$(nvcc --version | grep "release" | awk '{print $6}' | cut -d',' -f1)
echo "   CUDA Version: $NVCC_VERSION"

# Create target directory
mkdir -p "$PROJECT_ROOT/target"

# Compile with optimization
nvcc -ptx \
    -O3 \
    --gpu-architecture=sm_70 \
    --use_fast_math \
    --maxrregcount=128 \
    -I"$PROJECT_ROOT/src/utils" \
    -o "$PTX_OUTPUT" \
    "$CUDA_SRC"

if [ $? -eq 0 ]; then
    echo "✅ CUDA kernels compiled successfully!"
    echo "   PTX file: $PTX_OUTPUT"
    ls -lh "$PTX_OUTPUT"
else
    echo "❌ Compilation failed!"
    exit 1
fi
