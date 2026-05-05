---
title: Known Issues
description: Active P1/P2 issues in VisionClaw — read before debugging unexpected behaviour
category: reference
updated-date: 2026-05-05
---

# Known Issues

This file tracks active production issues and design limitations in VisionClaw. Read it before spending time debugging behaviour that is already understood. Each entry includes root cause, impact, and the direction of the intended fix. Where an ADR or explanation doc exists, it is linked.

---

## P1 Issues (Production Impact)

None currently active.

---

## P2 Issues (Degraded Feature)

### AUTH-001: Enterprise SSO Not Supported

**Status**: Gap / Architecture Decision Pending
**Impact**: VisionClaw's authentication stack is built on Nostr NIP-98 (cryptographic keypairs, browser extension, npubs). There is no OIDC, SAML, or LDAP integration. Enterprise deployments in regulated industries (healthcare, finance) cannot use Nostr browser extensions for staff authentication.

**Workaround**: None currently. JWT was fully removed in November 2025 (not deprecated — removed). The `VIRCADIA_JWT_SECRET` env var can be removed from compose files — Vircadia is no longer part of the stack. See `docs/explanation/security-model.md`.

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
| ONT-001: Ontology Edge Gap — 62% of ontology nodes isolated due to empty `iri_to_id` map in `neo4j_adapter.rs` | Fixed Apr 2026 | `neo4j_adapter.rs` — added `iri_to_id` population loop after KGNode loading | `docs/explanation/ontology-pipeline.md` |
| WS-001: Delta Encoding — permanently retired; caused position state divergence and latency spikes | Resolved by design | [ADR-037](adr/superseded/ADR-037-binary-protocol-consolidation.md), [ADR-061](adr/ADR-061-binary-protocol-unification.md) | [docs/binary-protocol.md](binary-protocol.md) |
| GPU-002: Analytics actors missing SharedGPUContext — PageRank, SSSP, APSP endpoints returned "GPU context not initialized" | Fixed Apr 2026 | Forward `SetSharedGPUContext` from `PhysicsSupervisor` to `AnalyticsSupervisor` | `src/actors/gpu/` |
| PHYS-001: No graph position reset endpoint — extreme parameters could not be recovered without container restart | Fixed Apr 2026 | `POST /api/graph/reset-positions` randomizes GPU positions and triggers reheat | `src/actors/gpu/force_compute_actor.rs` |
| UI-001: Client slider init race — sliders sent values before fetching server state | Fixed Apr 2026 | Slider ranges capped; `settingsLoaded` gate added | `client/src/features/physics/` |
| UI-002: `SETTINGS_AUTH_BYPASS` not picked up by Docker Compose | Fixed Apr 2026 | Auth bypass also triggers on `DOCKER_ENV=1 + NODE_ENV=development` | `src/auth_extractor.rs` |
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
