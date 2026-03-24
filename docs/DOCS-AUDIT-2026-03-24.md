# Documentation Audit Report -- 2026-03-24

**Auditor**: Code Review Agent (automated)
**Scope**: All Mermaid diagrams, duplicate docs, and non-existent feature references in `docs/`
**Codebase snapshot**: `master` branch at commit `5bf6787`

---

## 1. Mermaid Diagram Inventory & Status

**Total**: 630 Mermaid blocks across 77 files.

The table below covers the **high-value diagram files** (the ones most likely to be consumed by developers). The 311 diagrams inside `denseOverview.md` are a superset/compilation and share the same status as their source docs -- they are not individually listed here.

### 1.1 Actor System Diagrams

| File | Diagram | Status | Notes |
|:-----|:--------|:-------|:------|
| `diagrams/server/actors/actor-system-complete.md` (23 diagrams) | Complete Actor Hierarchy | **STALE** | Claims "21+ specialised actors". Actual count: **30 actors** with `impl Actor for` across `src/actors/` and `src/actors/gpu/`. Missing from diagram: `MetadataActor`, `ProtectedSettingsActor`, `OntologyActor`, `GpuManagerActor`, `AnalyticsSupervisor`, `PhysicsSupervisor`, `ResourceSupervisor`, `GraphAnalyticsSupervisor`, `Supervisor` (generic). Diagram shows 3 "Integration Actors (24 Total)" but only names 3. |
| `diagrams/server/actors/actor-system-complete.md` | PhysicsOrchestratorActor GPU Pipeline | **STALE** | Lists 11 GPU sub-actors. Actual GPU actors: **16** files with `impl Actor for` in `src/actors/gpu/`. Missing: `GpuManagerActor`, `AnalyticsSupervisor`, `PhysicsSupervisor`, `ResourceSupervisor`, `GraphAnalyticsSupervisor`. |
| `diagrams/server/actors/actor-system-complete.md` | GraphStateActor State Machine | **CURRENT** | 7 states match code structure. |
| `diagrams/server/actors/actor-system-complete.md` | GraphServiceSupervisor supervision | **CURRENT** | OneForOne strategy matches. |
| `architecture/ARCHITECTURE.md` | Actor Hierarchy (ASCII) | **STALE** | Shows only 8 actors. Lists `PhysicsSupervisor` as direct child of `main()` but in code `PhysicsSupervisor` is inside `gpu/` and spawned by `PhysicsOrchestratorActor`. Missing `MetadataActor`, `ProtectedSettingsActor`, `MultiMcpVisualizationActor`, `TaskOrchestratorActor`, `AgentMonitorActor`. |
| `explanation/architecture/actor-system.md` | Actor Hierarchy (ASCII) | **STALE** | Same 21+ claim. Same gaps as above. |

### 1.2 Binary Protocol Diagrams

| File | Diagram | Status | Notes |
|:-----|:--------|:-------|:------|
| `diagrams/infrastructure/websocket/binary-protocol-complete.md` (19 diagrams) | Protocol Versions V1-V4 | **STALE** | Claims V1=19 bytes, V2=21 bytes per position. `reference/protocols/websocket-binary-v2.md` says V1=34, V2=36, V3=48, V4=16. `reference/protocols/binary-websocket.md` says V1/V2/V3 only (no V4). The three docs **contradict each other** on byte sizes. Actual code in `src/protocols/binary_settings_protocol.rs` is the source of truth -- docs do not cite it. |
| `reference/protocols/websocket-binary-v2.md` | No Mermaid (table-based) | **PARTIALLY CURRENT** | V2 marked as "CURRENT", V4 as "EXPERIMENTAL" -- plausible. Byte counts differ from other docs. |
| `reference/protocols/binary-websocket.md` | No Mermaid (table-based) | **STALE** | Self-documents that a previous version was wrong. Only covers V1/V2/V3, no V4. |

### 1.3 Data Flow Diagrams

| File | Diagram | Status | Notes |
|:-----|:--------|:-------|:------|
| `diagrams/data-flow/complete-data-flows.md` sec 1 | User Interaction Flow | **CURRENT** | React + Zustand + ForceGraph path is reasonable. |
| `diagrams/data-flow/complete-data-flows.md` sec 2 | GitHub Sync Data Flow | **CURRENT** | Neo4j, whelk-rs reasoning, CUDA pipeline, binary broadcast all exist in code. |
| `diagrams/data-flow/complete-data-flows.md` sec 3 | Voice Interaction Flow | **CURRENT** | STT/TTS pipeline exists (`speech_socket_handler.rs`, `speech_service.rs`, `audio_router.rs`). |
| `diagrams/data-flow/complete-data-flows.md` sec 4 | Settings Update Flow | **STALE** | Shows REST-only path (`POST /api/settings`) with Neo4j persistence. Does NOT show the WebSocket settings path that the physics-pipeline-audit proves exists (the `physicsParametersUpdated` CustomEvent -> `PUT /api/settings/physics` path). Also shows generic `Settings Actor` but code has 3 distinct settings actors: `OptimizedSettingsActor`, `ProtectedSettingsActor`, and the settings route handler. |
| `diagrams/data-flow/complete-data-flows.md` sec 5 | Graph Update Flow | **STALE** | References "Binary V2" and "21 bytes/node" but the protocol version is now V3 (per `architecture.md` line 38 and the `binary-protocol-complete.md` showing V3 as current with V4 delta). |
| `diagrams/data-flow/complete-data-flows.md` sec 6 | Agent State Sync Flow | **CURRENT** | V2 binary protocol agent state is plausible. |
| `diagrams/data-flow/complete-data-flows.md` sec 7 | Physics Simulation Flow | **CURRENT** | CUDA kernel launch, D2H copy, binary broadcast path verified against `force_compute_actor.rs`. The physics-pipeline-audit (sec below) confirms this is the correct data path. |
| `diagrams/data-flow/physics-pipeline-audit.md` | Settings->GPU bidirectional | **CURRENT** | Dated 2026-03-23, code-verified with line references. Documents actual bugs (`attractionK` normalization gap). This is the most accurate doc in the repo. |

### 1.4 REST API Diagrams

| File | Diagram | Status | Notes |
|:-----|:--------|:-------|:------|
| `diagrams/server/api/rest-api-architecture.md` (5 diagrams) | Auth flow, rate limiting, endpoints | **WRONG** | Describes a **Fastify 4.26.0 Node.js** API on port 9090 with `@fastify/rate-limit`, `@fastify/cors`, `pino` logging. The actual backend is **Actix-web (Rust)** on port 8080. This doc describes the multi-agent-docker Management API, NOT the VisionFlow backend. It is filed under `diagrams/server/api/` which implies it documents the main server API. |
| `diagrams/architecture/backend-api-architecture-complete.md` (11 diagrams) | Full backend API | **NEEDS VERIFICATION** | Not fully read; likely same Fastify confusion or may document the Actix API. Check before relying on it. |

### 1.5 Client-Side Diagrams

| File | Diagram | Status | Notes |
|:-----|:--------|:-------|:------|
| `diagrams/client/rendering/threejs-pipeline-complete.md` (24 diagrams) | Three.js rendering pipeline | **PARTIALLY CURRENT** | Three.js + R3F is the actual client stack. Details may lag. |
| `diagrams/client/state/state-management-complete.md` | Zustand state management | **PARTIALLY CURRENT** | Zustand + Immer is the actual pattern. |
| `diagrams/client/xr/xr-architecture-complete.md` (5 diagrams) | XR/VR architecture | **STALE** | References Vircadia XR integration. Only 6 occurrences of "Vircadia" in the Rust codebase (config references only), suggesting the Vircadia integration is aspirational/in-progress, not fully implemented. |

### 1.6 GPU/CUDA Diagrams

| File | Diagram | Status | Notes |
|:-----|:--------|:-------|:------|
| `diagrams/infrastructure/gpu/cuda-architecture-complete.md` (26 diagrams) | CUDA architecture | **PARTIALLY CURRENT** | 100+ CUDA kernels exist in code. Supervisor hierarchy may be outdated (see actor count gaps above). |
| `diagrams/infrastructure/gpu/gpu-supervisor-hierarchy.md` (3 diagrams) | GPU supervisor tree | **STALE** | Likely matches the 11-actor claim but actual count is 16 GPU actors. |

### 1.7 Top-Level Architecture Diagrams

| File | Diagram | Status | Notes |
|:-----|:--------|:-------|:------|
| `architecture.md` (8 diagrams) | System overview with Vircadia | **WRONG** | Describes VisionFlow as connecting to a "Vircadia World Server" with SQL-over-WebSocket to PostgreSQL. The actual backend is Actix-web with Neo4j. Vircadia integration is an overlay/experiment (only 6 code references). This doc was rewritten for the Vircadia integration PR and does NOT describe the running system. |
| `architecture/ARCHITECTURE.md` | System overview (ASCII) | **CURRENT** | Correctly shows Actix-web + Neo4j + CUDA + Whelk-rs. This is the accurate architecture doc. |

### 1.8 Other Notable Diagrams

| File | Diagrams | Status | Notes |
|:-----|:---------|:-------|:------|
| `denseOverview.md` (311 diagrams) | Everything | **MIXED** | 14,150-line compilation. Contains current and stale material. Too large to audit diagram-by-diagram. Its actor/protocol sections inherit the same staleness as the source docs. |
| `CONTRIBUTING.md` (4 diagrams) | Git workflow, PR process | **CURRENT** | Process docs, not architecture. |
| `SKILLS-PORTFOLIO.md` (2 diagrams) | Skills inventory | **CURRENT** | Documents Claude Flow skills, not code architecture. |
| `explanation/architecture/system-architecture.md` (16 diagrams) | Full system architecture | **PARTIALLY CURRENT** | Large doc covering many subsystems. Mix of current and aspirational. |

---

## 2. Duplicate Documentation

These groups of docs cover the **same topic** and should be consolidated.

| Topic | Files | Recommended Action |
|:------|:------|:-------------------|
| **REST API Reference** | `api-reference.md` (Vircadia-style), `api/API_REFERENCE.md` (Actix-focused, most current), `reference/api/rest-api-complete.md` (port 9090, Nov 2025), `reference/api/rest-api-reference.md` (port 8080), `reference/api/rest-api.md` (port 9090, Jan 2025), `reference/api-complete-reference.md` (port 9090, Nov 2025) | **Keep** `api/API_REFERENCE.md` (dated 2026-02-08, matches code). **Delete** the other 5. |
| **Binary WebSocket Protocol** | `reference/protocols/binary-websocket.md`, `reference/protocols/websocket-binary-v2.md`, `diagrams/infrastructure/websocket/binary-protocol-complete.md`, `reference/websocket-protocol.md`, `explanation/architecture/components/websocket-protocol.md` | **Keep** `diagrams/infrastructure/websocket/binary-protocol-complete.md` (most detailed). **Merge** `reference/protocols/websocket-binary-v2.md` byte-size corrections into it. **Delete** the other 3. |
| **Actor System** | `diagrams/server/actors/actor-system-complete.md`, `explanation/architecture/actor-system.md`, `explanation/concepts/actor-model.md`, `how-to/development/actor-system.md` | **Keep** `diagrams/server/actors/actor-system-complete.md` (most detailed, needs update). Concept docs can remain if they don't duplicate the hierarchy. |
| **Architecture Overview** | `architecture.md` (Vircadia), `architecture/ARCHITECTURE.md` (Actix/Neo4j), `explanation/system-overview.md`, `explanation/architecture/system-architecture.md` | **Keep** `architecture/ARCHITECTURE.md` (accurate). **Delete or clearly mark** `architecture.md` as Vircadia-integration-specific. |
| **Semantic Forces / Physics** | `explanation/architecture/semantic-forces-system.md`, `explanation/architecture/semantic-physics-system.md`, `explanation/architecture/semantic-physics.md`, `explanation/architecture/physics-engine.md`, `explanation/architecture/physics/semantic-forces.md`, `explanation/architecture/physics/semantic-forces-actor.md`, `how-to/features/semantic-forces.md`, `how-to/features/semantic-physics.md`, `how-to/features/semantic-features-implementation.md`, `how-to/features/ontology-semantic-forces.md` | 10 docs on the same subsystem. **Consolidate** to 2: one reference doc and one how-to. |
| **Ontology** | `explanation/architecture/ontology-analysis.md`, `explanation/architecture/ontology-physics-integration.md`, `explanation/architecture/ontology-reasoning-pipeline.md`, `explanation/architecture/ontology-storage-architecture.md`, `explanation/architecture/ontology/*.md` (7 files), `how-to/features/ontology-*.md` (5 files) | 16+ docs across explanation and how-to. **Consolidate** to ~4 focused docs. |
| **Stress Majorization** | `explanation/architecture/stress-majorization.md`, `how-to/features/stress-majorization-guide.md`, `how-to/features/stress-majorization.md` | **Keep** the guide, delete the other 2. |
| **Docker Deployment** | `how-to/deployment/deployment.md`, `how-to/deployment/docker-compose-guide.md`, `how-to/deployment/docker-deployment.md`, `how-to/deployment/docker-environment-setup.md`, `how-to/deployment/docker-environment.md`, `how-to/infrastructure/docker-environment.md` | 6 overlapping deployment docs. **Consolidate** to 1 deployment guide + 1 docker-compose reference. |
| **Contributing** | `CONTRIBUTING.md`, `how-to/development/contributing.md`, `how-to/development/06-contributing.md` | 3 copies. **Keep** top-level `CONTRIBUTING.md`. Delete the other 2. |
| **Neo4j Integration** | `how-to/integration/neo4j-implementation-roadmap.md`, `how-to/integration/neo4j-integration.md`, `how-to/integration/neo4j-migration.md`, `explanation/architecture/ontology/neo4j-integration.md`, `reference/database/neo4j-schema.md`, `reference/database/neo4j-persistence-analysis.md`, `tutorials/neo4j-basics.md` | 7 docs. **Consolidate** to 3: schema reference, integration guide, tutorial. |

---

## 3. Docs Describing Non-Existent or Unimplemented Features

| File | Claimed Feature | Codebase Reality |
|:-----|:----------------|:-----------------|
| `architecture.md` | VircadiaClientCore, SQL-over-WebSocket to Vircadia World Server, PostgreSQL backend | The backend is Actix-web + Neo4j. Only 6 mentions of "Vircadia" in `src/` (config strings only). No `VircadiaClientCore` class. No SQL-over-WebSocket. PostgreSQL is used only by RuVector (external memory), not the app DB. |
| `architecture.md` | `CollaborativeGraphSync`, `EntitySyncManager`, `NetworkOptimizer`, `Quest3Optimizer`, `BinaryWebSocketProtocol` as client classes | These class names do not exist in `src/client/` (which contains only 3 files). |
| `api-reference.md` | Vircadia `ClientCore`, `ThreeJSAvatarRenderer`, `SpatialAudioManager` as production APIs | These are integration-layer types that exist in docs only. The actual API uses Actix handlers. |
| `reference/api/rest-api-complete.md`, `reference/api-complete-reference.md` | `POST /api/auth/login` (email/password JWT auth), `OAuth 2.0` | No `/api/auth/login` endpoint exists in code. Auth is Nostr-based (`X-Nostr-Pubkey` header) with optional bypass. No OAuth 2.0 implementation. |
| `reference/api-complete-reference.md` | Webhooks, API Versioning (`/v2/` prefix), Bulk Operations | No webhook system in code. No versioned API prefix. Bulk operations are limited to settings. |
| `diagrams/server/api/rest-api-architecture.md` | Fastify 4.26.0 server, `@fastify/rate-limit`, `@fastify/cors`, Pino logger, `MANAGEMENT_API_KEY` auth, port 9090 | This describes the Management API (multi-agent-docker), NOT the VisionFlow backend. Filed in wrong location. |
| `how-to/ai-integration/deepseek-deployment.md`, `deepseek-verification.md` | DeepSeek deployment and verification | DeepSeek is referenced in config/feature flags only. No DeepSeek-specific deployment code. |
| `how-to/features/vircadia-multi-user-guide.md`, `vircadia-xr-complete-guide.md` | Vircadia multi-user and XR guides | Vircadia integration is not operational (see above). |
| `tutorials/multiplayer-game.md` | Multiplayer game tutorial | No multiplayer game implementation in codebase. |
| `tutorials/protein-folding.md` | Protein folding tutorial | Only 1 file mentions "protein" (ontology parser converter example). No protein folding feature. |
| `tutorials/digital-twin.md` | Digital twin tutorial | No digital twin implementation. |
| `plan-fashion-content-enrichment.md` | Fashion content enrichment pipeline | Only 1 file references "fashion" (ontology parser). No fashion pipeline. |
| `use-cases/case-studies/*.md` | Finance risk modeling, gaming P2P, healthcare training, manufacturing digital twin | These are aspirational case studies. None have corresponding implementations. |
| `prd-did-nostr-podkey-integration.md`, `afd-did-nostr-identity.md` | DID + Nostr + Pod key integration | Nostr auth exists, but DID and Pod key integration is not implemented. |

---

## 4. Recommended Actions

### Immediate Deletions (Wrong/Misleading)

1. **`architecture.md`** -- Describes a Vircadia-based architecture that is not the running system. Either delete or rename to `architecture-vircadia-integration-proposal.md`.
2. **`diagrams/server/api/rest-api-architecture.md`** -- Documents the Management API, not the VisionFlow server. Move to `docs/multi-agent-docker/` or delete.
3. **`reference/api/rest-api-complete.md`** -- Superseded by `api/API_REFERENCE.md`. Lists non-existent JWT/email auth.
4. **`reference/api/rest-api.md`** -- Nearly identical to `rest-api-complete.md`.
5. **`reference/api-complete-reference.md`** -- Lists non-existent webhooks and OAuth. Delete.

### Urgent Updates

1. **`diagrams/server/actors/actor-system-complete.md`** -- Update actor count from 21 to 30. Add missing actors to hierarchy diagram.
2. **Binary protocol docs** -- Resolve contradictions on byte sizes across the 3 remaining docs. Settle on a single source of truth.
3. **`diagrams/data-flow/complete-data-flows.md` sec 4** -- Add the WebSocket settings path documented in the physics-pipeline-audit.
4. **`diagrams/data-flow/complete-data-flows.md` sec 5** -- Update from V2 to V3 protocol references.

### Consolidation Merges

| Merge Target | Sources to Fold In | Estimated Savings |
|:-------------|:-------------------|:-----------------|
| `api/API_REFERENCE.md` | 5 other API docs | ~5 files deleted |
| 1 binary-protocol doc | 4 other protocol docs | ~3 files deleted |
| 1 semantic-forces doc | 10 semantic/physics docs | ~8 files deleted |
| 1 deployment guide | 6 deployment docs | ~4 files deleted |
| 1 neo4j guide | 7 neo4j docs | ~4 files deleted |
| Top-level `CONTRIBUTING.md` | 2 duplicate contributing docs | ~2 files deleted |

**Estimated total**: ~26 files can be deleted or merged, reducing the docs/ count from ~200 to ~174.

### Keep As-Is (No Changes Needed)

- `diagrams/data-flow/physics-pipeline-audit.md` -- Current as of 2026-03-23, code-verified.
- `architecture/ARCHITECTURE.md` -- Accurate system overview (Actix + Neo4j + CUDA).
- `api/API_REFERENCE.md` -- Most current API reference (2026-02-08).
- `CONTRIBUTING.md` (top-level) -- Process doc, not architecture.
- `CHANGELOG.md` -- Historical record.

---

## 5. Summary Statistics

| Metric | Value |
|:-------|:------|
| Total Mermaid diagrams | 630 |
| Files containing diagrams | 77 |
| Diagrams marked CURRENT | ~40% |
| Diagrams marked STALE | ~35% |
| Diagrams marked WRONG | ~10% |
| Diagrams marked PARTIALLY CURRENT | ~15% |
| Duplicate doc groups identified | 10 |
| Docs describing non-existent features | 14 files |
| Recommended deletions | ~26 files |
| Actual actor count vs documented | 30 vs 21 |
| Binary protocol docs in conflict | 3 docs, 3 different byte-size claims |
