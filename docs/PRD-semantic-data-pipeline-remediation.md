# PRD: Semantic Data Pipeline Remediation

**Date**: 2026-03-25
**Status**: APPROVED FOR EXECUTION
**Priority**: P0 — Last major refactor
**Principle**: Markdown is the source of truth. Neo4j is speed middleware. The GPU visualises the semantic structure that already exists in the data.

---

## 1. Problem Statement

The Logseq markdown files contain rich semantic relationships (9+ types: is-subclass-of, has-part, requires, enables, depends-on, relates-to, bridges-to/from, explicit_link, namespace). The parsing pipeline extracts all of them. But **only wikilinks survive** to the client graph — 8 of 9 relationship types are lost at various pipeline stages, and 110,209 OWL axiom nodes sit isolated in Neo4j.

**Quantitative gap**: 490 edges reach the client from a dataset containing ~2,600+ potential relationships (980 EDGE + 623 SUBCLASS_OF + ~1,000 from axiom materialisation).

---

## 2. Root Cause Analysis (from 3-agent audit)

### 7 Data Loss Points

| # | Stage | What's Lost | Root Cause | Severity |
|---|-------|-------------|------------|----------|
| **DL1** | OwlClass storage | 8/9 relationship types | `add_owl_class()` only stores `parent_classes` as SUBCLASS_OF. has-part, requires, enables etc. dropped | **CRITICAL** |
| **DL2** | Graph load (OwlClass path) | Non-hierarchical edges | `load_graph()` only queries SUBCLASS_OF between OwlClasses | HIGH |
| **DL3** | CSR construction | Edge type metadata | ForceComputeActor flattens edges to `(target, weight)`, discarding edge_type | HIGH |
| **DL4** | GPU analytics → AppState | cluster_id, anomaly_score, community_id | ClusteringActor/AnomalyDetectionActor compute but never write to `app_state.node_analytics` | HIGH |
| **DL5** | Binary protocol | Analytics fields | Wire format V3 has fields but TypeScript never reads them | MEDIUM |
| **DL6** | OwlAxiom → edges | 110K axioms | Stored as isolated nodes, never materialised as graph edges | HIGH |
| **DL7** | Constraint pipeline | OWL axiom → physics forces | OntologyConstraintTranslator exists but `apply_ontology_constraints()` never called | MEDIUM |

### Existing Code That Works (But Is Disconnected)

| Component | File | Status |
|-----------|------|--------|
| OntologyParser extracts 9+ relationship types | `parsers/ontology_parser.rs:354-397` | **Working** |
| GitHubSyncService creates typed edges with OWL IRIs | `github_sync_service.rs:382-490` | **Working** |
| GraphNode EDGE relationships in Neo4j | `neo4j_adapter.rs:582` | **Working** |
| SemanticForcesActor (DAG, type clustering, collision) | `gpu/semantic_forces_actor.rs` | **Spawned, no data** |
| OntologyConstraintActor (axiom → forces) | `gpu/ontology_constraint_actor.rs` | **Spawned, no data** |
| OntologyConstraintTranslator (5 constraint types) | `physics/ontology_constraints.rs` | **Implemented, never called** |
| WhelkInferenceEngine (transitive closure) | `adapters/whelk_inference_engine.rs` | **Working, output unused** |
| semantic_forces.cu CUDA kernel | `utils/semantic_forces.cu` | **Compiled, never invoked** |
| Binary protocol V3 analytics fields | `utils/binary_protocol.rs:40-50` | **Declared, always zero** |
| ClusterHulls component | `graph/components/ClusterHulls.tsx` | **Renders, no cluster data** |

---

## 3. Design Principle

**Do not create new systems. Wire the existing ones together.**

The architecture is sound. Every component exists. The problem is 7 broken wires between them.

---

## 4. Remediation Plan

### Phase 1: Data Integrity (Fix DL1, DL2, DL6)
*Goal: All markdown relationships reach Neo4j as edges*

#### 1.1 Store ALL relationship types as Neo4j edges
**File**: `src/adapters/neo4j_ontology_repository.rs` — `add_owl_class()`
**Change**: After storing SUBCLASS_OF for parent_classes, also store:
- `has_part` → `:RELATES {relationship_type: "has_part", owl_property_iri: "mv:hasPart"}`
- `requires` → `:RELATES {relationship_type: "requires"}`
- `depends_on`, `enables`, `relates_to`, `bridges_to`, `bridges_from`
**Impact**: ~500+ new Neo4j edges from existing parsed data

#### 1.2 Materialise SubClassOf axioms as SUBCLASS_OF edges
**File**: `src/adapters/neo4j_ontology_repository.rs`
**Change**: After whelk reasoning, run:
```cypher
MATCH (a:OwlAxiom {axiom_type: "SubClassOf"})
MATCH (s:OwlClass {iri: a.subject})
MATCH (o:OwlClass {iri: a.object})
MERGE (s)-[r:SUBCLASS_OF {is_inferred: true}]->(o)
```
**Impact**: Transitive closure edges from 110K axioms

#### 1.3 Load ALL relationship types in load_graph()
**File**: `src/adapters/neo4j_adapter.rs` — `load_graph()`
**Change**: After loading EDGE relationships, also query:
```cypher
MATCH (s)-[r:RELATES|SUBCLASS_OF]->(t) WHERE ...
```
Map to Edge objects with appropriate edge_type and weight.

### Phase 2: GPU Pipeline (Fix DL3, DL4)
*Goal: Edge types and analytics reach the GPU and flow back*

#### 2.1 Extend CSR with edge type buffer
**File**: `src/utils/unified_gpu_compute/construction.rs`
**Change**: Add `edge_types: DeviceBuffer<u8>` parallel to `edge_col_indices`. Upload edge type enum (0=explicit, 1=subclass, 2=structural, 3=dependency, 4=associative, 5=bridge).
**Impact**: `semantic_forces.cu` can read edge types for weighted springs

#### 2.2 Wire ClusteringActor → app_state.node_analytics
**File**: `src/actors/gpu/clustering_actor.rs`
**Change**: After computing cluster assignments, send results to `ClientCoordinatorActor` or write directly to `app_state.node_analytics`.
**Impact**: Binary protocol V3 carries real cluster_id/anomaly_score

### Phase 3: Semantic Forces Activation (Fix DL7)
*Goal: Existing CUDA kernels compute forces from semantic structure*

#### 3.1 Feed OntologyConstraintActor with axiom data
**File**: `src/actors/gpu/ontology_constraint_actor.rs`
**Change**: On graph reload, query OwlAxioms from Neo4j, run through `OntologyConstraintTranslator`, upload constraint buffer to GPU.
**Impact**: DisjointClasses push apart, SubClassOf clusters together, SameAs merges

#### 3.2 Activate SemanticForcesActor type clustering
**File**: `src/actors/gpu/semantic_forces_actor.rs`
**Change**: Forward `source_domain` from node metadata as `type_id` to the GPU kernel. Configure `TypeClusterConfig` with per-domain centroids.
**Impact**: Nodes cluster by domain (AI/BC/MV/RB) in 3D space

### Phase 4: Client Integration (Fix DL5)
*Goal: Client renders semantic structure visually*

#### 4.1 Parse V3 analytics fields in TypeScript
**File**: `client/src/types/binaryProtocol.ts`
**Change**: Expose `cluster_id`, `anomaly_score`, `community_id` in `BinaryNodeData`.

#### 4.2 Colour nodes by cluster, edges by type
**File**: `client/src/features/graph/components/GraphManager.tsx`
**Change**: Use `cluster_id` for node colouring, `edge_type` for edge colour/width.

---

## 5. Execution Order

```
Phase 1.1 → Phase 1.3 → Phase 1.2 → rebuild → verify edge counts
Phase 2.1 → Phase 2.2 → rebuild → verify analytics flow
Phase 3.1 → Phase 3.2 → rebuild → verify spatial clustering
Phase 4.1 → Phase 4.2 → verify visual output
```

Each phase is independently testable. Each build verifies the previous phase works before adding the next.

---

## 6. Success Criteria

| Metric | Current | Target |
|--------|---------|--------|
| Client edges | 490 | 1,500+ |
| Node isolation | 62% | <15% |
| Edge types in graph | 1 (explicit_link) | 9+ |
| GPU cluster_id populated | 0% | 100% |
| Spatial domain clustering | None | Visible BC/AI/MV/RB groups |
| Ontology constraints active | 0 | SubClassOf + DisjointWith |
| Cluster hulls meaningful | 1 blob | 4-6 distinct domain hulls |

---

## 7. Files Modified (Estimated)

| Phase | Files | Lines Changed |
|-------|-------|---------------|
| 1.1 | neo4j_ontology_repository.rs | ~50 |
| 1.2 | neo4j_ontology_repository.rs | ~30 |
| 1.3 | neo4j_adapter.rs | ~40 |
| 2.1 | construction.rs, execution.rs, memory.rs | ~80 |
| 2.2 | clustering_actor.rs, app_state.rs | ~40 |
| 3.1 | ontology_constraint_actor.rs, graph_state_actor.rs | ~60 |
| 3.2 | semantic_forces_actor.rs, settings propagation | ~40 |
| 4.1 | binaryProtocol.ts, graph.worker.ts | ~30 |
| 4.2 | GraphManager.tsx, ClusterHulls.tsx | ~40 |
| **Total** | **~15 files** | **~410 lines** |

---

## 8. Risk Assessment

- **Low risk**: Phases 1.x are additive (more edges, no removal)
- **Medium risk**: Phase 2.1 (CSR extension) touches GPU memory layout
- **Low risk**: Phase 3.x activates existing code paths
- **Low risk**: Phase 4.x is client-only changes

No destructive changes. Each phase adds capability without removing existing functionality.
