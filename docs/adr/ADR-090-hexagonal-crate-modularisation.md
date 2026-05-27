# ADR-090 — Hexagonal Crate Modularisation

Status      : Proposed
Date        : 2026-05-23
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
