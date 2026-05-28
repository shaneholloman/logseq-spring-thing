# ADR-090 — Hexagonal Crate Modularisation

Status      : Accepted (2026-05-28)
Date        : 2026-05-23
Implemented : 2026-05-28 (commits a61a15f..70d979b)
Supersedes  : Implicit single-crate monolith decision
Related     : PRD-016, ADR-11 (Persistence Migration), ADR-061 (Binary Protocol)

## Context

The `webxr` crate is a 123k-line Rust monolith. With `codegen-units=1` (release profile), LLVM processes the entire crate as a single compilation unit. A one-line change in any `.rs` file triggers ~12 minutes of optimisation. During the current bug-squashing phase, this latency blocks rapid iteration.

The codebase already exhibits hexagonal architecture *by convention*: `src/ports/` defines traits, `src/adapters/` implements them, `src/models/` holds domain types, and `src/actors/` manages orchestration. But cargo treats this as a single crate — it cannot skip unchanged modules.

Existing workspace crates:
- `crates/visionclaw-contracts/` — cross-boundary typed contracts (leaf, no workspace deps)
- `crates/graph-cognition-core/` — graph cognition primitives
- `crates/graph-cognition-extract/` — graph extraction utilities
- `crates/graph-cognition-physics-presets/` — physics preset configurations

## Decision

### D1. Split `webxr` into 7 workspace crates along hexagonal boundaries

The 30 top-level modules in `src/` map to 7 crates based on the hexagonal pattern:

| Crate | Modules | Responsibility |
|-------|---------|---------------|
| `visionflow-domain` | `models/`, `types/`, `errors/`, `events/`, `ports/` | Domain model, port traits, no framework deps |
| `visionflow-gpu` | `gpu/`, `physics/`, `layout/`, `constraints/` | GPU compute, CUDA kernels, force layout |
| `visionflow-ontology` | `ontology/`, `inference/`, `reasoning/`, `validation/` | OWL reasoning, Whelk integration |
| `visionflow-adapters` | `adapters/`, `repositories/`, `services/parsers/` | Oxigraph, SQLite, GitHub, Nostr |
| `visionflow-protocol` | `protocol/`, `protocols/`, `utils/binary_*` | Wire protocol encode/decode |
| `visionflow-actors` | `actors/`, `cqrs/`, `application/` | Actix actor hierarchy, CQRS bus |
| `visionflow-server` | `handlers/`, `middleware/`, `config/`, `settings/`, `telemetry/`, `services/` | HTTP routes, WebSocket handlers |

The root `webxr` crate becomes a thin binary: `main.rs` + startup wiring.

### D2. Acyclic dependency invariant

The workspace dependency graph MUST be a DAG. `cargo tree` in CI will fail the build if cycles are detected. The canonical ordering (inner to outer):

```
contracts → domain → {gpu, ontology, protocol} → adapters → actors → server → webxr
```

### D3. Feature flag propagation

The `gpu` and `ontology` Cargo features propagate from the root `webxr` crate to the crates that need them. Each crate defines its own feature gates for conditional compilation. The root re-exports:

```toml
[features]
default = ["gpu", "ontology"]
gpu = ["visionflow-gpu/gpu", "visionflow-actors/gpu"]
ontology = ["visionflow-ontology/ontology", "visionflow-actors/ontology"]
```

### D4. Migration is incremental, phase-per-PR

Each phase extracts one crate, updates `Cargo.toml` workspace members, adjusts `use` paths, and validates with `cargo build --release` + `cargo test --workspace`. No big-bang refactor.

### D5. `codegen-units` per crate

Individual crates use `codegen-units = 16` (default) for fast compilation. Only the final `webxr` binary link step uses LTO. This gives us incremental build speed without sacrificing release binary performance.

## Consequences

### Positive
- Incremental release builds drop from ~12 min to ~2 min for single-crate changes
- Each crate is independently testable without GPU or network
- Domain types stabilise as a published API surface
- New adapters (e.g., new persistence backend) only touch `visionflow-adapters`
- CI can parallelise crate compilation

### Negative
- Initial migration effort across 7 phases
- `pub(crate)` items must become `pub` when crossing crate boundaries (increases API surface)
- Actor message types may need to move to domain crate to break cycles
- Feature flag wiring is more verbose

### Neutral
- No change to binary protocol, REST API, or WebSocket behaviour
- No change to Docker image structure (still one `webxr` binary)
- No change to CUDA kernel compilation (stays in `build.rs` of `visionflow-gpu`)

---

## Realisation

*Added 2026-05-28 — reflects the shipped state as of commits a61a15f..70d979b.*

### Crates extracted (6 of 7)

| Crate | Status | Notes |
|-------|--------|-------|
| `visionflow-domain` | Shipped | Phase 1 + 1b — owns all domain types, port traits, GPU adapter ports |
| `visionflow-adapters` | Shipped | Phase 2 / A1+ — Oxigraph ontology store, Whelk inference engine |
| `visionflow-gpu` | Shipped | CUDA kernels, force-layout, build.rs PTX compilation |
| `visionflow-ontology` | Shipped | OWL types, horned-owl pipeline, OntologyPipelineService |
| `visionflow-actors` | Shipped | Actor message types; actor implementations remain in webxr (see below) |
| `visionflow-protocol` | Shipped | Binary V2/V3 encode/decode, BinaryMessage wire types |

`visionflow-server` is the intended name for the final extracted binary wrapper; in practice the root `webxr` crate serves that role and remains the Cargo binary target.

### Phase 1b — keystone unblock

Phase 1b (model unification) was the critical path item. Once `GraphData`, `Node`, and `Edge` became canonical domain types inside `visionflow-domain`, the downstream chain (`GpuPhysicsAdapter`, `GpuSemanticAnalyzer`, `OntologyRepository`) could all move in a single coordinated pass. Without Phase 1b the later phases would have required circular workarounds.

### Production bugs uncovered during extraction

The test-coverage pass that accompanied the crate extraction surfaced three real production defects, fixed in commit 70d979b:

1. **SPARQL aggregate + FROM clause** — `COUNT(*)` queries silently returned zero when combined with a named graph `FROM` clause on Oxigraph; the OntologyRepository now strips the `FROM` on aggregate paths.
2. **`BinaryMessage::Delta` wire asymmetry** — the encoder wrote a 2-byte flags field that the decoder skipped, causing silent payload misalignment on delta frames.
3. **Stateful `flate2::Compress` exhaustion** — the compression context was reused across frames without reset, causing deflate stream corruption after ~300 frames.

### Test suite

2,200+ tests pass across the workspace (`cargo test --workspace`).

## What did NOT move

### Adapters still in webxr (blocked on actor extraction)

Five adapter implementations remain in `src/adapters/` of the `webxr` crate because they depend on actor types and message files that have not yet been moved:

- `actor_graph_repository` — depends on `crate::actors::graph_state_actor`
- `actix_physics_adapter` — depends on `crate::actors::physics_orchestrator_actor`
- `actix_semantic_adapter` — depends on `crate::actors::semantic_processor_actor`
- `oxigraph_graph_repository` — depends on `crate::actors::graph_actor` + `socket_flow_messages`
- `physics_orchestrator_adapter` — depends on `crate::actors::*` + `crate::utils::socket_flow_messages`

### Port traits still in webxr

`GraphRepository` and `SettingsRepository` remain in `src/ports/` of the `webxr` crate because their signatures depend on `PhysicsState`, `AutoBalanceNotification`, and `AppFullSettings` — types that are webxr-internal and not yet promoted to the domain crate.

### Actor implementations still in webxr

14+ actor implementations (`GraphStateActor`, `PhysicsOrchestratorActor`, `ClientCoordinatorActor`, and others) remain in `webxr/src/actors/` because they depend on `crate::config::AppFullSettings`, `crate::utils::socket_flow_messages`, and `crate::handlers::*` — all of which are webxr-local. Extracting these is the next major phase.
