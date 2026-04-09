---
title: Known Issues
description: Active P1/P2 issues in VisionClaw — read before debugging unexpected behaviour
category: reference
updated-date: 2026-04-09
---

# Known Issues

This file tracks active production issues and design limitations in VisionClaw. Read it before spending time debugging behaviour that is already understood. Each entry includes root cause, impact, and the direction of the intended fix. Where an ADR or explanation doc exists, it is linked.

---

## P1 Issues (Production Impact)

### ONT-001: Ontology Edge Gap — 62% of Ontology Nodes Isolated

**Status**: Known / Under Investigation
**Impact**: 623 `SUBCLASS_OF` relationships originating from `OwlClass` nodes in Neo4j are excluded from the client graph. 62% of ontology nodes appear visually isolated with no edges in the 3D visualisation. SemanticForcesActor receives incomplete constraint data, so GPU-enforced semantic clustering and disjointness forces have no effect on the affected nodes.

**Root Cause**: `OwlClass` nodes in Neo4j use a different label scheme from the `GraphNode` entries the client constructs. The 623 `SUBCLASS_OF` relationships originate from `OwlClass` source nodes, but client-side graph construction expects `GraphNode`-to-`GraphNode` edges. The mapping between `OwlClass` nodes and `GraphNode` entries requires `owl_class_iri` field matching that is not currently implemented. Without this mapping, the edges are silently dropped during graph state loading; no error is logged.

**Symptom**: The dense ontology subgraph appears as a cloud of disconnected nodes in the 3D view. The knowledge graph (`public:: true` files) and agent nodes are unaffected and render correctly.

**Workaround**: None currently available for end users.

**Fix Direction**: Map `OwlClass` → `GraphNode` via `owl_class_iri` field at the `GraphStateActor` level using the following Cypher pattern:

```cypher
MATCH (oc:OwlClass)-[:SUBCLASS_OF]->(parent:OwlClass)
MATCH (gn_child:GraphNode {owl_class_iri: oc.iri})
MATCH (gn_parent:GraphNode {owl_class_iri: parent.iri})
CREATE (gn_child)-[:SUBCLASS_OF]->(gn_parent)
```

Full details including the node schema, Neo4j index definitions, and the full pipeline sequence diagram are in `docs/explanation/ontology-pipeline.md` — specifically sections 4 (Neo4j Storage) and 8 (The Ontology Edge Gap Problem).

---

## P2 Issues (Degraded Feature)

### WS-001: V4 Delta Encoding — Not Production Ready

**Status**: Experimental / Do Not Use
**Impact**: V4 (16-byte per-changed-node delta frames) causes position state divergence and latency spikes every 60 frames due to resync overhead. Nodes gradually drift to incorrect positions on the client, then snap back on the forced full-state resync that fires every 60 frames. Under packet loss or reconnect scenarios, client-side delta accumulation diverges further from server state before the next resync corrects it.

**Root Cause**: The V4 resync strategy (full V2 frame at frame 0 and every 60 frames) is insufficient to bound drift under real network conditions. The `i16` delta encoding (`position × 100.0`) provides 0.01-unit precision, which accumulates rounding error across many frames. There is no sequence-number-based consistency check; the client has no way to detect a missed delta frame other than waiting for the next scheduled resync.

**Symptom**: Intermittent "all nodes at origin" visual glitch. Nodes drifting slowly then snapping. Latency spikes at 60-frame intervals visible in browser devtools WebSocket frame timing.

**Workaround**: Use V2 (36-byte standard format) or V3 (48-byte analytics format) in all production and staging deployments. V4 is disabled by default and must be explicitly opted into. Do not enable it.

**Fix Direction**: Implement sequence-number-based resync without full-broadcast penalty. See `docs/reference/websocket-binary.md` (V4 Delta Format section, around line 284) for the current frame layout. No ETA.

---

### AUTH-001: Enterprise SSO Not Supported

**Status**: Gap / Architecture Decision Pending
**Impact**: VisionClaw's authentication stack is built on Nostr NIP-98 (cryptographic keypairs, browser extension, npubs). There is no OIDC, SAML, or LDAP integration. Enterprise deployments in regulated industries (healthcare, finance) cannot use Nostr browser extensions for staff authentication.

**Workaround**: None currently. JWT was fully removed in November 2025 (not deprecated — removed). The `VIRCADIA_JWT_SECRET` env var retained in the compose file is solely for legacy Vircadia World Server compatibility and does not re-enable VisionClaw API auth. See `docs/explanation/security-model.md`.

**Fix Direction**: An ADR is required before any implementation work begins. The three candidate approaches are:
- (a) Wrap NIP-98 behind a SAML-to-Nostr proxy so enterprise identity providers map to Nostr keypairs transparently.
- (b) Add a first-class OIDC port alongside NIP-98, with NIP-98 remaining the default for open deployments.
- (c) Scope enterprise auth to API-key-per-service for M2M use cases only, leaving human auth as Nostr-only.

---

### AGENT-001: RVF File Store Not Yet Implemented

**Status**: Draft PRD / Postgres-only in Production
**Impact**: The `rvf-integration-prd.md` describes replacing RuVector's PostgreSQL dependency with portable `.rvf` files for agent memory. This is **not implemented**. The current production agent memory backend is exclusively `ruvector-postgres:5432` (pgvector + HNSW). Any deployment that lacks the `ruvector-postgres` container will fail on the first `memory_store` call with a connection refused error; there is no fallback.

**Workaround**: Ensure the `ruvector-postgres` container is running and reachable at `ruvector-postgres:5432` before starting agents. Connection string is available via `$RUVECTOR_PG_CONNINFO`. Run `docker ps | grep ruvector` to confirm the container is up.

**Fix Direction**: See `docs/adr/rvf-integration-prd.md` for the full specification. Implementation is blocked on `rvf-runtime` WASM target stabilisation.

---

## Resolved Issues

Previously known issues that are now fixed. Listed here so that old bug reports, forum threads, or git blame comments referencing these symptoms can be traced to their resolution.

| Issue | Status | Fixed In | Reference |
|-------|--------|----------|-----------|
| Periodic full broadcast — nodes that converged before client connection never received positions; late-connecting clients saw all nodes at origin | Fixed Mar 2026 | `force_compute_actor.rs` — periodic full broadcast now fires inside the non-empty delta branch every 300 iterations, not only when all nodes are stopped | `docs/explanation/physics-gpu-engine.md` |
| PTX ISA version mismatch — CUDA kernel failed to load on drivers supporting only PTX 9.0 when nvcc 13.2 emitted `.version 9.2` | Fixed Mar 2026 | `build.rs` auto-downgrade: detects driver PTX cap at build time and passes `--gpu-architecture` accordingly | `docs/explanation/physics-gpu-engine.md` |
| Worker position data race — all edges vanished on the first rendered frame because the physics worker initialised all nodes at origin (0, 0, 0) | Fixed | `handleGraphUpdate` now returns `dataWithPositions` (with generated positions); the caller sends this to the worker instead of the original position-free `data` | `docs/explanation/client-architecture.md` |
| Edge hash dedup — edges appeared frozen after physics convergence because `updatePoints` used a 3-value hash (`len`, `pts[0]`, `pts[len-1]`) that matched even when intermediate edge endpoints changed | Fixed | Hash removed; `computeInstanceMatrices` runs unconditionally every frame | `docs/explanation/client-architecture.md` |
