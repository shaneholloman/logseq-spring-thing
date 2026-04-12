# GPU Kernel Integration QE Plan

## Executive Summary

A 6-agent hierarchical mesh swarm audited the full VisionFlow GPU pipeline from Neo4j through CUDA kernels to client WebSocket rendering. The core force-directed graph pipeline (11 sequential kernels) is **fully operational**. However, **~58 of 110 CUDA kernels are disconnected** — they compile into the binary but are never called from Rust. The disconnects follow a consistent pattern: CUDA kernels and CPU fallbacks exist, but the Rust FFI bridge layer was never completed.

### Key Metrics

| Metric | Value |
|--------|-------|
| Total CUDA kernels defined | 110 |
| Kernels wired end-to-end | ~52 |
| Kernels with CPU fallback only | ~20 |
| Dead code kernels | ~38 |
| .cu files compiled | 11/11 (100%) |
| Client UI controls | ~165+ |
| UI coverage of GPU features | ~92% |

---

## Pipeline Status Matrix

### Core Physics Pipeline (WORKING)

```
Neo4j -> GraphData -> CSR adjacency -> GPU upload -> 11 kernels ->
position readback -> delta encoding -> WebSocket V3/V4 -> Three.js render
```

All 9 stages verified working. FastSettle convergence (KE < 0.005, 50-iter warmup, 2000 cap) is sound.

### Feature Pipeline Status

| Feature | GPU Kernel | Rust FFI | Actor | Message | HTTP API | Client UI | End-to-End |
|---------|:----------:|:--------:|:-----:|:-------:|:--------:|:---------:|:----------:|
| **Force-directed layout** | 27 | Y | Y | Y | Y | Y | WORKING |
| **Stability/convergence** | 3/4 | Y | Y | Y | - | Y | WORKING |
| **K-means clustering** | 5/6 | Y | Y | Y | Y | Y | WORKING |
| **Louvain community** | 1 | Y | Y | Y | Y | Y | WORKING |
| **LOF anomaly detection** | 1 | Y | Y | Y | Y | Y | WORKING |
| **Z-Score anomaly** | 2 | Y | Y | Y | Y | Y | WORKING |
| **SSSP shortest path** | 2 | Y | Y | Y | Y | Partial | WORKING |
| **APSP landmark** | 3 | Y | Y | Y | Y | N | WORKING |
| **Connected components** | 3 | Y | Y | Y | Y | N | API only |
| **Semantic DAG forces** | 8 | Y | Y | Y | Y | Y | WORKING |
| **Semantic physicality** | 3 | N | N | N | N | N | CPU FALLBACK |
| **Semantic role cluster** | 3 | N | N | N | N | N | CPU FALLBACK |
| **Semantic maturity layout** | 1 | N | N | N | N | N | CPU FALLBACK |
| **PageRank centrality** | 8 | Y | Y | Y | **N** | Partial | BROKEN |
| **DBSCAN clustering** | 4 | Y | Y | **N** | **N** | N | PARTIAL |
| **Ontology constraints (5)** | 5 | N | Partial | Y | Y | Y | GENERIC ONLY |
| **Stress majorization** | 2 | N | Y (CPU) | N | N | N | CPU ONLY |
| **AABB reduction** | 1 | Y | Y | - | - | - | WORKING |
| **Spatial grid/hash** | 2 | Y | Y | - | - | - | WORKING |

---

## Disconnect Analysis

### Pattern: FFI Bridge Never Completed

The consistent pattern across all disconnected features:

```
1. CUDA kernel written (complete, optimized, production-ready)
2. CPU fallback implementation added (works, slower)
3. Rust FFI extern "C" declarations NEVER ADDED
4. Actor method to call GPU kernel NEVER ADDED
5. CPU fallback runs silently instead of GPU
6. No one notices because the feature "works" (just slow)
```

### Root Causes

1. **Incremental development** — GPU kernels were written in batch, FFI was wired feature-by-feature, and the later features never got their bridge
2. **Silent CPU fallback** — Features work on CPU so there's no failure signal
3. **Missing integration tests** — No tests verify "this ran on GPU, not CPU"
4. **Actor boundary** — Each GPU actor has its own FFI block; new kernels in .cu files don't auto-register

---

## Integration Waves (Priority Order)

### Wave 1: Quick Wins (1-2 days, ~300 lines)

These features have all infrastructure except the final wiring.

#### 1.1 PageRank HTTP Endpoints (2-3 hours)

**Gap**: GPU kernels + actor + compute engine all complete. NO HTTP endpoints.

**Files to modify**:
- `src/handlers/api_handler/analytics/mod.rs` — add pagerank route module
- NEW: `src/handlers/api_handler/analytics/pagerank_handlers.rs` — 3 endpoints
- `src/app_state.rs` — expose pagerank_actor as public field

**Endpoints needed**:
```
POST /api/analytics/pagerank/compute    {damping: 0.85, max_iter: 100, epsilon: 1e-6}
GET  /api/analytics/pagerank/result     -> {scores: [{node_id, score}], converged, iterations}
POST /api/analytics/pagerank/clear      -> {ok: true}
```

**Client wiring**: `client/src/features/analytics/` already has UI dropdown for PageRank — just needs API call implementation.

**Test**: `curl -X POST http://localhost:4000/api/analytics/pagerank/compute`

#### 1.2 DBSCAN as Standalone Clustering (2-3 hours)

**Gap**: GPU kernel complete, called from AnomalyDetectionActor. Needs own message type + handler.

**Files to modify**:
- `src/actors/messages/analytics_messages.rs` — add `RunDBSCAN` message
- `src/actors/gpu/clustering_actor.rs` — add DBSCAN handler (delegate to unified_compute)
- `src/handlers/api_handler/analytics/clustering.rs` — add `/clustering/dbscan` endpoint
- `client/src/features/analytics/components/SemanticClusteringControls.tsx` — add DBSCAN option

**Parameters**: `{epsilon: f32, min_points: u32}`

#### 1.3 Connected Components Client UI (1-2 hours)

**Gap**: API endpoint `/api/analytics/pathfinding/connected-components` works. No client visualization.

**Files to modify**:
- `client/src/features/analytics/` — add component visualization
- `client/src/features/graph/managers/graphDataManager.ts` — color nodes by component_id

---

### Wave 2: Semantic GPU Acceleration (1-2 days, ~200 lines)

#### 2.1 Physicality/Role/Maturity GPU FFI Bridge

**Gap**: 7 CUDA kernels defined, CPU fallbacks running, Rust FFI declarations missing.

**Files to modify**:
- `src/actors/gpu/semantic_forces_actor.rs` — add 7 extern "C" FFI declarations
- `src/gpu/kernel_bridge.rs` — add 7 kernel bridge wrappers
- `src/gpu/semantic_forces.rs` — modify `apply_forces()` to prefer GPU path when available

**Pattern** (repeat for each kernel):
```rust
// In semantic_forces_actor.rs extern block:
fn apply_physicality_cluster_force(
    pos_x: *const f32, pos_y: *const f32, pos_z: *const f32,
    force_x: *mut f32, force_y: *mut f32, force_z: *mut f32,
    centroids_x: *const f32, centroids_y: *const f32, centroids_z: *const f32,
    class_ids: *const i32,
    num_nodes: i32, num_classes: i32,
    strength: f32, radius: f32,
    stream: *mut std::ffi::c_void,
);
```

**Existing CPU code** (in `apply_forces()`) already gates on config flags:
```rust
if self.config.physicality_cluster.enabled {
    self.apply_physicality_cluster_forces_cpu(graph); // Change to GPU
}
```

**Estimated speedup**: 10-50x for graphs > 1000 nodes.

---

### Wave 3: Ontology Constraint Specialization (2-3 days)

#### 3.1 Wire 5 Specialized Ontology Kernels

**Gap**: All constraints flow through generic `force_pass_kernel`. The 5 specialized kernels (disjoint, hierarchy, colocate, symmetry, cardinality) would optimize each constraint type.

**Current flow**:
```
ConstraintData → generic force_pass_kernel → uniform application
```

**Target flow**:
```
ConstraintData → classify by OntologyConstraintGroup →
  Separation  → apply_disjoint_classes_kernel
  Alignment   → apply_subclass_hierarchy_kernel
  Identity    → apply_sameas_colocate_kernel
  Symmetry    → apply_inverse_symmetry_kernel
  Cardinality → apply_functional_cardinality_kernel
```

**Files to modify**:
- `src/actors/gpu/ontology_constraint_actor.rs` — dispatch constraints by type to specialized kernels
- `src/utils/unified_gpu_compute/execution.rs` — add kernel dispatch logic
- Kernel bridge declarations for the 5 `launch_*` functions

**Prerequisite**: Node class_id and class_charge metadata must be routed to specialized kernels (currently uploaded but unused by generic kernel).

---

### Wave 4: Stress Majorization GPU (1-2 days)

#### 4.1 Wire GPU Stress Majorization

**Gap**: 2 GPU kernels defined (`compute_stress_kernel`, `stress_majorization_step_kernel`). Actor uses CPU-only implementation.

**Files to modify**:
- `src/actors/gpu/stress_majorization_actor.rs` — add GPU path in `perform_stress_majorization()`
- `src/utils/unified_gpu_compute/metrics.rs` or new file — add `run_stress_majorization_gpu()`
- FFI bridge for 2 kernels

**Note**: Stress majorization requires all-pairs distance matrix. For large graphs (>5K nodes), use landmark APSP approximation (already wired in GPU).

---

### Wave 5: Dead Code Audit & Cleanup (1 day)

| Kernel | Decision |
|--------|----------|
| `select_weighted_centroid_kernel` (K-means) | Remove or wire K-means++ initialization |
| `dbscan_find_neighbors_tiled_kernel` | Keep as optimization option, add config flag |
| `dbscan_compact_labels_kernel` | Wire for post-processing GPU relabeling |
| `reduce_kinetic_energy_kernel` (stability) | Verify vs calculate_kinetic_energy, remove if redundant |

---

## Quality Gates

### QG1: GPU Path Verification

For each wired feature, verify GPU execution with:

```rust
#[test]
fn test_feature_runs_on_gpu() {
    // Setup GPU context
    // Run feature
    // Assert: GPU kernel was invoked (not CPU fallback)
    // Assert: Results match CPU fallback within epsilon
}
```

### QG2: Performance Benchmarks

| Feature | Target (1K nodes) | Target (10K nodes) | Current |
|---------|-------------------|---------------------|---------|
| Force-directed step | < 2ms | < 10ms | Unknown |
| K-means (10 clusters) | < 5ms | < 20ms | Unknown |
| PageRank (100 iter) | < 50ms | < 200ms | Unknown |
| DBSCAN | < 10ms | < 50ms | Unknown |
| Semantic forces (all) | < 3ms | < 15ms | Unknown (CPU) |
| Ontology constraints | < 1ms | < 5ms | Unknown (generic) |

### QG3: End-to-End Integration

For each feature, verify the full pipeline:
```
1. Load graph from Neo4j (or test fixture)
2. Upload to GPU
3. Trigger computation
4. Verify positions/results change correctly
5. Verify WebSocket binary broadcast contains feature data
6. Verify client receives and renders correctly
```

### QG4: Regression Safety

- Existing CPU fallbacks MUST remain functional
- GPU path failures MUST fall back to CPU (not crash)
- Binary protocol V3/V4 format MUST NOT change (breaking clients)

---

## Build & Deployment Fixes

### CUDA_ARCH Auto-Detection (DONE)

**Problem**: `.env` contained `CUDA_ARCH=89` (from old GPU) but RTX A6000 is `sm_86`.

**Fix applied**: `scripts/rust-backend-wrapper.sh` now always prefers runtime GPU detection:
```bash
DETECTED_ARCH=$(nvidia-smi --query-gpu=compute_cap --format=csv,noheader --id=0 ...)
if [ "$CUDA_ARCH" != "$DETECTED_ARCH" ]; then
    log "WARNING: .env CUDA_ARCH=${CUDA_ARCH} does not match GPU (sm_${DETECTED_ARCH})"
fi
export CUDA_ARCH="$DETECTED_ARCH"
```

**Action needed**: Restart containers to pick up fix. The `.env` has been updated by user.

### Multi-Arch Gencode (RECOMMENDED)

Currently `build.rs` compiles for single arch. For portability:
```
-gencode=arch=compute_75,code=[sm_75,compute_75]  # Turing
-gencode=arch=compute_86,code=[sm_86,compute_86]  # Ampere (A6000)
-gencode=arch=compute_89,code=[sm_89,compute_89]  # Ada Lovelace
-gencode=arch=compute_90,code=[compute_90]         # Hopper (PTX JIT)
```

---

## Documentation Gaps

1. `docs/gpu-physics-architecture.md` missing `semantic_forces.cu` from PTX module table
2. `gpu_landmark_apsp.cu` kernels marked "unused" but ARE called — update comments
3. No architecture doc for the binary protocol V3/V4 wire format
4. No doc for the FastSettle convergence algorithm parameters

---

## Estimated Total Effort

| Wave | Scope | Rust Lines | Client Lines | Duration |
|------|-------|:----------:|:------------:|:--------:|
| 1 | Quick wins (PageRank, DBSCAN, CC UI) | ~200 | ~150 | 1-2 days |
| 2 | Semantic GPU FFI bridge (7 kernels) | ~200 | 0 | 1-2 days |
| 3 | Ontology constraint specialization | ~300 | ~50 | 2-3 days |
| 4 | Stress majorization GPU | ~150 | 0 | 1-2 days |
| 5 | Dead code audit + cleanup | ~50 | 0 | 1 day |
| **Total** | | **~900** | **~200** | **6-10 days** |

All GPU kernel code exists and is production-quality. The work is purely **bridge and plumbing** — no new algorithms or CUDA code needed.
