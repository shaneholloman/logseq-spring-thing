#!/bin/bash
# Comprehensive GPU Test Runner
# Compiles CUDA kernels and runs all integration tests + benchmarks

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}╔════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║  GPU Integration Test Suite - Week 6          ║${NC}"
echo -e "${BLUE}║  REAL CUDA Validation with unified.db         ║${NC}"
echo -e "${BLUE}╚════════════════════════════════════════════════╝${NC}"
echo ""

# Check prerequisites
echo -e "${YELLOW}📋 Checking prerequisites...${NC}"

# Check CUDA
if ! command -v nvcc &> /dev/null; then
    echo -e "${RED}❌ CUDA toolkit not found. Install CUDA 12.4+${NC}"
    exit 1
fi

CUDA_VERSION=$(nvcc --version | grep "release" | awk '{print $6}' | cut -d',' -f1)
echo -e "   ${GREEN}✓${NC} CUDA Toolkit: $CUDA_VERSION"

# Check GPU
if ! command -v nvidia-smi &> /dev/null; then
    echo -e "${RED}❌ nvidia-smi not found. GPU drivers may not be installed${NC}"
    exit 1
fi

GPU_COUNT=$(nvidia-smi --query-gpu=name --format=csv,noheader | wc -l)
if [ "$GPU_COUNT" -eq 0 ]; then
    echo -e "${RED}❌ No CUDA-capable GPU detected${NC}"
    exit 1
fi

GPU_NAME=$(nvidia-smi --query-gpu=name --format=csv,noheader | head -1)
echo -e "   ${GREEN}✓${NC} GPU: $GPU_NAME"

# Check Rust
if ! command -v cargo &> /dev/null; then
    echo -e "${RED}❌ Rust/Cargo not found${NC}"
    exit 1
fi

RUST_VERSION=$(rustc --version | awk '{print $2}')
echo -e "   ${GREEN}✓${NC} Rust: $RUST_VERSION"

echo ""

# Step 1: Compile CUDA kernels
echo -e "${YELLOW}🔧 Step 1/4: Compiling CUDA kernels...${NC}"
cd "$PROJECT_ROOT"

if [ -f "$PROJECT_ROOT/target/visionclaw_unified.ptx" ]; then
    echo -e "   ${YELLOW}⚠${NC}  PTX already exists, recompiling..."
fi

./scripts/compile_cuda.sh

if [ ! -f "$PROJECT_ROOT/target/visionclaw_unified.ptx" ]; then
    echo -e "${RED}❌ PTX compilation failed${NC}"
    exit 1
fi

echo -e "   ${GREEN}✓${NC} PTX compiled successfully"
echo ""

# Step 2: Run integration tests
echo -e "${YELLOW}🧪 Step 2/4: Running integration tests...${NC}"

export VISIONCLAW_PTX_PATH="$PROJECT_ROOT/target/visionclaw_unified.ptx"
export RUST_BACKTRACE=1

echo ""
echo -e "${BLUE}═══ Test 1/7: Spatial Grid Kernel ═══${NC}"
cargo test --features gpu --test cuda_integration_tests test_spatial_grid_with_unified_db -- --nocapture || {
    echo -e "${RED}❌ Spatial Grid test failed${NC}"
    exit 1
}

echo ""
echo -e "${BLUE}═══ Test 2/7: Barnes-Hut Force Computation ═══${NC}"
cargo test --features gpu --test cuda_integration_tests test_barnes_hut_performance -- --nocapture || {
    echo -e "${RED}❌ Barnes-Hut test failed${NC}"
    exit 1
}

echo ""
echo -e "${BLUE}═══ Test 3/7: SSSP Relaxation Kernel ═══${NC}"
cargo test --features gpu --test cuda_integration_tests test_sssp_relaxation_kernel -- --nocapture || {
    echo -e "${RED}❌ SSSP test failed${NC}"
    exit 1
}

echo ""
echo -e "${BLUE}═══ Test 4/7: K-means Clustering ═══${NC}"
cargo test --features gpu --test cuda_integration_tests test_kmeans_clustering -- --nocapture || {
    echo -e "${RED}❌ K-means test failed${NC}"
    exit 1
}

echo ""
echo -e "${BLUE}═══ Test 5/7: LOF Anomaly Detection ═══${NC}"
cargo test --features gpu --test cuda_integration_tests test_lof_anomaly_detection -- --nocapture || {
    echo -e "${RED}❌ LOF test failed${NC}"
    exit 1
}

echo ""
echo -e "${BLUE}═══ Test 6/7: Label Propagation Community Detection ═══${NC}"
cargo test --features gpu --test cuda_integration_tests test_label_propagation_community_detection -- --nocapture || {
    echo -e "${RED}❌ Label Propagation test failed${NC}"
    exit 1
}

echo ""
echo -e "${BLUE}═══ Test 7/7: Constraint Evaluation with Ontology ═══${NC}"
cargo test --features gpu --test cuda_integration_tests test_constraint_evaluation_with_ontology -- --nocapture || {
    echo -e "${RED}❌ Constraint test failed${NC}"
    exit 1
}

echo ""
echo -e "${GREEN}✅ All integration tests passed!${NC}"
echo ""

# Step 3: Run performance benchmarks (optional)
if [ "$1" == "--bench" ] || [ "$1" == "-b" ]; then
    echo -e "${YELLOW}⚡ Step 3/4: Running performance benchmarks...${NC}"
    echo -e "   ${YELLOW}⚠${NC}  This will take 5-10 minutes"
    echo ""

    cargo bench --features gpu --bench cuda_performance_benchmarks || {
        echo -e "${RED}❌ Benchmarks failed${NC}"
        exit 1
    }

    echo ""
    echo -e "${GREEN}✅ Benchmarks completed!${NC}"
    echo -e "   Results: ${BLUE}target/criterion/report/index.html${NC}"
else
    echo -e "${YELLOW}⏭  Step 3/4: Skipping benchmarks (use --bench to run)${NC}"
fi

echo ""

# Step 4: Generate test report
echo -e "${YELLOW}📊 Step 4/4: Generating test report...${NC}"

REPORT_FILE="$PROJECT_ROOT/target/gpu_test_report.txt"

cat > "$REPORT_FILE" <<EOF
GPU Integration Test Report
===========================
Generated: $(date)

System Information
------------------
CUDA Version: $CUDA_VERSION
GPU: $GPU_NAME
Rust Version: $RUST_VERSION
PTX Location: $PROJECT_ROOT/target/visionclaw_unified.ptx

Test Results
------------
✓ Spatial Grid Kernel - PASSED
✓ Barnes-Hut Force Computation (10K nodes) - PASSED
✓ SSSP Relaxation Kernel - PASSED
✓ K-means Clustering - PASSED
✓ LOF Anomaly Detection - PASSED
✓ Label Propagation Community Detection - PASSED
✓ Constraint Evaluation with Ontology - PASSED

All 7 Tier 1 CUDA kernels validated with unified.db integration.

Performance Targets
-------------------
Target: 30 FPS (33ms per frame) for 10K nodes
Status: See benchmark results in target/criterion/

Database Integration
--------------------
Schema: migration/unified_schema.sql
Tables Used:
  - graph_nodes (x, y, z, vx, vy, vz physics state)
  - graph_edges (CSR-ready edge weights)
  - owl_classes (ontology classes)
  - owl_axioms (semantic constraints)

Test Coverage
-------------
✓ Spatial hashing and grid acceleration
✓ Barnes-Hut force approximation
✓ SSSP with frontier compaction
✓ K-means with k-means++ initialization
✓ LOF outlier detection
✓ Label propagation for communities
✓ Semantic constraint forces

Week 6 Deliverable Status
--------------------------
✅ REAL CUDA integration tests (NO MOCKS)
✅ unified.db schema integration
✅ 7 Tier 1 kernels validated
✅ Performance benchmarks with 30 FPS target
✅ Ontology constraint validation
✅ CI-ready test suite

Next Steps
----------
1. Profile performance with nsys
2. Optimize kernels < 33ms target
3. Add advanced ontology axioms
4. Integrate with production pipeline

EOF

echo -e "   ${GREEN}✓${NC} Report saved: $REPORT_FILE"
cat "$REPORT_FILE"

echo ""
echo -e "${GREEN}╔════════════════════════════════════════════════╗${NC}"
echo -e "${GREEN}║  ✅ GPU Test Suite PASSED                      ║${NC}"
echo -e "${GREEN}║  All 7 Tier 1 CUDA kernels validated          ║${NC}"
echo -e "${GREEN}╚════════════════════════════════════════════════╝${NC}"
echo ""
echo -e "${BLUE}📁 Test files:${NC}"
echo -e "   - tests/cuda_integration_tests.rs"
echo -e "   - tests/cuda_performance_benchmarks.rs"
echo -e "   - tests/README_GPU_TESTS.md"
echo ""
echo -e "${BLUE}📊 Results:${NC}"
echo -e "   - Test report: $REPORT_FILE"
echo -e "   - Benchmarks: target/criterion/report/index.html"
echo ""
