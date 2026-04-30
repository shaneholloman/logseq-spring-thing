---
title: Known Issues
description: Active P1/P2 issues in VisionClaw — read before debugging unexpected behaviour
category: reference
updated-date: 2026-04-12
---

# Known Issues

This file tracks active production issues and design limitations in VisionClaw. Read it before spending time debugging behaviour that is already understood. Each entry includes root cause, impact, and the direction of the intended fix. Where an ADR or explanation doc exists, it is linked.

---

## P1 Issues (Production Impact)

### ONT-001: Ontology Edge Gap — 62% of Ontology Nodes Isolated

**Status**: Fixed (2026-04-14)
**Impact**: Was: 623 `SUBCLASS_OF` relationships excluded from client graph. 62% of ontology nodes appeared visually isolated.

**Root Cause**: `iri_to_id` map in `neo4j_adapter.rs` was never populated from KGNode nodes. The map was declared empty, and the only population code was in a dead OwlClass fallback path (unreachable after an early return). The ontology edge bridge at line 735 (`if !iri_to_id.is_empty()`) never executed.

**Fix**: Added `iri_to_id` population loop after KGNode loading, iterating loaded nodes and inserting `owl_class_iri → node.id` mappings. Removed 80 lines of dead OwlClass fallback code. The ontology edge bridge now executes and maps `SUBCLASS_OF` + `RELATES` relationships between `OwlClass` nodes into numeric `KGNode` IDs.

**Verification**: After rebuild, `info!` log will show "ONT-001: Built iri_to_id map — N KGNode nodes have owl_class_iri" followed by "Loaded M ontology edges (SUBCLASS_OF + RELATES)".

---

## P2 Issues (Degraded Feature)

### WS-001: Delta Encoding — Permanently Retired (historical)

**Status**: Resolved by [ADR-037](adr/ADR-037-binary-protocol-consolidation.md) (delta lock-in) and [ADR-061](adr/ADR-061-binary-protocol-unification.md) (single binary protocol). Per-frame delta encoding is not, and will not become, part of the binary protocol — see [docs/binary-protocol.md](binary-protocol.md).

**Historical impact**: Delta-encoded position frames caused position state divergence and latency spikes due to resync overhead. The graph is a force-directed spring network — every node moves every tick — so the encoding never compressed in practice while introducing real correctness risks (stale-position drift on reconnect, silent drop of pin signals, parallel decode paths).

**Current state**: The wire format is the single binary protocol with 24 bytes/node, fixed forever. Bandwidth is managed via broadcast cadence (token-bucket backpressure on `ForceComputeActor`, see [ADR-031](adr/ADR-031-broadcast-backpressure.md)), not payload encoding.

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

### GPU-002: Analytics Actors Missing SharedGPUContext

**Status**: Resolved
**Impact**: PageRank, SSSP, APSP, and Connected Components endpoints return "GPU context not initialized" or "actor not available". The GPU context is created by `ForceComputeActor` but not shared to `PageRankActor`, `ShortestPathActor`, or `ConnectedComponentsActor` in the analytics subsystem.

**Root Cause**: `AnalyticsSupervisor` spawns analytics actors but does not forward the `SetSharedGPUContext` message from the physics supervisor. Only `ForceComputeActor` receives the GPU context.

**Workaround**: Clustering (K-means, DBSCAN, Louvain) works because `ClusteringActor` accesses the GPU via `UnifiedGPUCompute` which self-initializes. Other analytics actors use a different initialization path.

**Fix Direction**: Forward `SetSharedGPUContext` from `PhysicsSupervisor` to `AnalyticsSupervisor` → all child actors. Estimated 50 lines.

---

### PHYS-001: No Graph Position Reset Endpoint

**Status**: Resolved
**Impact**: When physics parameters change, the layout evolves from the current positions. If a previous extreme parameter set pushed nodes to boundary extremes, moderate parameters cannot recover them (gravity too weak at distance). The only reset is a full container restart.

**Root Cause**: `POST /api/physics/reset` exists but depends on `PhysicsService` which is not injected into AppState. `POST /api/admin/sync` triggers `ReloadGraphFromDatabase` but requires power user auth and the full GitHub sync pipeline.

**Fix Direction**: Add `POST /api/graph/reset-positions` that randomizes GPU positions and triggers a reheat. Estimated 30 lines in `force_compute_actor.rs`.

---

### UI-001: Client Slider Init Race — Settings Pushed Before Server Load

**Status**: Resolved
**Impact**: On first client connect, sliders may send values before fetching server state. With the old max ranges (repelK: 50000), this produced extreme physics parameters. Slider ranges are now capped to sane values (repelK: 2000, centerGravityK: 10) which limits damage, but the race condition remains.

**Fix Direction**: Client should fetch `GET /api/settings/physics` and populate slider values before enabling any PUT calls. Add a `settingsLoaded` flag to gate writes.

---

### UI-002: `SETTINGS_AUTH_BYPASS` Not Picked Up by Docker Compose

**Status**: Resolved
**Impact**: `.env` contains `SETTINGS_AUTH_BYPASS=true` but `docker compose config` resolves it to `false`. Settings PUT calls return 401.

**Root Cause**: Docker Compose `.env` file resolution depends on the working directory of the `docker compose` command, which may differ from the project root when invoked via `launch.sh` from a DinD container.

**Workaround**: Auth bypass now also triggers on `DOCKER_ENV=1 + NODE_ENV=development` (added to `auth_extractor.rs`).

---

## Resolved Issues

Previously known issues that are now fixed. Listed here so that old bug reports, forum threads, or git blame comments referencing these symptoms can be traced to their resolution.

| Issue | Status | Fixed In | Reference |
|-------|--------|----------|-----------|
| Periodic full broadcast — nodes that converged before client connection never received positions; late-connecting clients saw all nodes at origin | Fixed Mar 2026 | `force_compute_actor.rs` — periodic full broadcast now fires inside the non-empty delta branch every 300 iterations, not only when all nodes are stopped | `docs/explanation/physics-gpu-engine.md` |
| PTX ISA version mismatch — CUDA kernel failed to load on drivers supporting only PTX 9.0 when nvcc 13.2 emitted `.version 9.2` | Fixed Mar 2026 | `build.rs` auto-downgrade: detects driver PTX cap at build time and passes `--gpu-architecture` accordingly | `docs/explanation/physics-gpu-engine.md` |
| Worker position data race — all edges vanished on the first rendered frame because the physics worker initialised all nodes at origin (0, 0, 0) | Fixed | `handleGraphUpdate` now returns `dataWithPositions` (with generated positions); the caller sends this to the worker instead of the original position-free `data` | `docs/explanation/client-architecture.md` |
| Edge hash dedup — edges appeared frozen after physics convergence because `updatePoints` used a 3-value hash (`len`, `pts[0]`, `pts[len-1]`) that matched even when intermediate edge endpoints changed | Fixed | Hash removed; `computeInstanceMatrices` runs unconditionally every frame | `docs/explanation/client-architecture.md` |
| CUDA thrust SM_890 error in GPU initialization — cuBlas context creation failed intermittently on Ada Lovelace GPUs | Fixed Apr 2026 | `force_compute_actor.rs` — added device synchronization before context creation and PTX module cache invalidation on arch mismatch | `src/actors/gpu/force_compute_actor.rs` |
| PTX module lookup in community.rs — clustering kernels referenced incorrect module path causing kernel dispatch failures | Fixed Apr 2026 | `src/utils/ptx.rs` — added module name mapping for `gpu_clustering_kernels`, `ontology_constraints`, `pagerank` | `src/utils/ptx.rs` |
| Slider range degeneration — max values capped at 50000 produced extreme physics parameters on first client connect | Fixed Apr 2026 | `client/src/features/physics/components/SettingsPanel.tsx` — slider ranges capped to sane defaults (repelK: 2000, centerGravityK: 10, damping: 0.98) | `docs/KNOWN_ISSUES.md` |
| Clustering visualization missing analytics — DBSCAN and Louvain results not appearing in client analytics panel | Fixed Apr 2026 (analytics moved off the per-frame wire by ADR-061) | Historical: prior wire format wrote cluster_id to node_analytics. Post-ADR-061: cluster_id rides the `analytics_update` JSON message at recompute cadence; the per-frame binary protocol carries position+velocity only. | `src/actors/gpu/clustering_actor.rs` |
| **Dual ClientCoordinatorActor instances** — `SocketFlowServer` registered clients in one actor while `PhysicsOrchestratorActor` broadcast to a second internally-created instance; result: "2241 positions, 0 clients in manager", zero binary frames delivered to any client | Fixed Apr 2026 (commit fcfc1a166) | `graph_service_supervisor.rs` — new `with_client()` constructor accepts externally-injected `ClientCoordinatorActor`; `app_state.rs` passes shared address; internal creation skipped when external instance provided | `src/actors/graph_service_supervisor.rs` |
| **ClientFilter default filtering to zero** — `ClientFilter::default()` had `enabled: true` with empty `filtered_node_ids`, causing every new client to receive zero nodes from `broadcast_with_filter` | Fixed Apr 2026 (commit fcfc1a166) | `client_filter.rs` — default changed to `enabled: false`; opt-in filtering semantics | `src/actors/client_filter.rs` |
| **FastSettle permanent physics halt** — `FastSettle` mode latched `fast_settle_complete = true` and `is_physics_paused = true` on reaching the iteration cap even without energy convergence; subsequent settings changes could not resume simulation | Fixed Apr 2026 (commit fcfc1a166) | `physics_orchestrator_actor.rs` — non-convergent exhaustion falls back to `Continuous` mode rather than halting | `src/actors/physics_orchestrator_actor.rs` |
| **Boundary-pinned nodes** — 468 nodes oscillating at ±2000 (viewport bounds) on Y/Z axes; runaway rescue threshold of 10× bounds did not catch nodes already at the wall | Fixed Apr 2026 (commit fcfc1a166) | `force_compute_actor.rs` — added `boundary_stuck_frames` counter (per node); nodes at boundary for ≥60 frames are teleported to randomised interior position with zeroed velocity | `src/actors/gpu/force_compute_actor.rs` |
