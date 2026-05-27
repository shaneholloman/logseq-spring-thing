# DDD-16 — Domain-Driven Design: Hexagonal Crate Boundaries

Date        : 2026-05-23
Related     : PRD-016, ADR-090

## 1. Bounded Contexts

The VisionFlow backend has six bounded contexts, each mapping to a workspace crate:

### 1.1 Graph Domain (`visionflow-domain`)

**Ubiquitous Language**: Node, Edge, GraphData, MetadataStore, BinaryNodeData, SimParams, SettleMode

**Aggregate Roots**:
- `GraphData` — owns nodes, edges, metadata, id_to_metadata map
- `SimParams` — owns all physics simulation parameters including SettleMode

**Value Objects**: `Vec3Data`, `BinaryNodeDataClient`, `NodeId` (u32), `EdgeId` (String)

**Port Traits** (defined here, implemented by adapters):
- `GraphRepository` — load/save graph data
- `OntologyRepository` — load/save ontology classes, axioms, properties
- `SettingsRepository` — load/save application settings

**Domain Events**:
- `GraphLoaded`, `GraphUpdated`, `NodePositionsChanged`
- `SettingsChanged`, `PhysicsParametersUpdated`
- `OntologyValidated`, `OntologyConstraintViolation`

### 1.2 GPU Physics (`visionflow-gpu`)

**Ubiquitous Language**: ForceCompute, BroadcastOptimizer, FastSettle, Continuous, CUDA kernel, PTX module, spatial hash, Barnes-Hut

**Aggregate Root**: `UnifiedGPUCompute` — owns GPU buffers, kernel launches, position/velocity state

**Invariants**:
- Broadcasts are ALWAYS full position snapshots (BROADCAST-001)
- No delta/diff encoding in the broadcast pipeline
- Energy threshold must be reachable for the current node count
- NaN/Inf positions are filtered before broadcast

**Anti-Corruption Layer**: `GpuBuffers` translates between domain `SimParams` and GPU-native `CudaSimParams`

### 1.3 Ontology Reasoning (`visionflow-ontology`)

**Ubiquitous Language**: OwlClass, OwlProperty, OwlAxiom, Whelk, SubClassOf, DisjointWith, FunctionalProperty

**Aggregate Root**: `OntologyState` — owns class hierarchy, property graph, axiom set

**Invariants**:
- Ontology nodes are spatially separated from knowledge nodes (graphSeparationX)
- Ontology mass is 10× knowledge mass for visual stability
- Whelk inference results are cached with TTL

### 1.4 Infrastructure Adapters (`visionflow-adapters`)

**Ubiquitous Language**: Oxigraph, SQLite, SPARQL, GitHub sync, Nostr bridge

**Repository Implementations**:
- `OxigraphGraphRepository` — implements `GraphRepository` port
- `OxigraphOntologyRepository` — implements `OntologyRepository` port
- `SqliteSettingsRepository` — implements `SettingsRepository` port

**Anti-Corruption Layer**: SPARQL query builders translate domain queries to Oxigraph-specific SPARQL

### 1.5 Actor Orchestration (`visionflow-actors`)

**Ubiquitous Language**: Supervisor, ForceComputeActor, PhysicsOrchestratorActor, GraphStateActor, ClientCoordinatorActor

**Aggregate Root**: `GraphServiceSupervisor` — owns actor lifecycle, message routing

**Invariants**:
- Single `ClientCoordinatorActor` instance shared between supervisor and socket handler
- FastSettle → Continuous fallback on energy threshold exhaustion
- Settings changes trigger reheat with gradual decay

### 1.6 HTTP Surface (`visionflow-server`)

**Ubiquitous Language**: Route, Handler, Middleware, SocketFlowServer, Settings endpoint

**Context Map**:
- **Conformist** to `visionflow-domain` (uses domain types directly in JSON responses)
- **Customer-Supplier** with `visionflow-actors` (sends messages, receives results)
- **Published Language** for REST API (camelCase JSON via serde aliases)

## 2. Context Map

```
┌─────────────────────────────────────────────────────┐
│                  visionflow-server                   │
│  (HTTP handlers, WebSocket, middleware)              │
│  Conformist to domain types                         │
└──────────┬──────────────────────────┬───────────────┘
           │ messages                 │ port calls
           ▼                         ▼
┌──────────────────┐    ┌─────────────────────────┐
│ visionflow-actors│    │ visionflow-adapters      │
│ (orchestration)  │    │ (Oxigraph, SQLite, etc.) │
└──────┬───────────┘    └──────────┬──────────────┘
       │                           │
       ▼                           │
┌──────────────┐ ┌──────────────┐  │
│visionflow-gpu│ │visionflow-   │  │
│(CUDA physics)│ │ontology      │  │
└──────┬───────┘ └──────┬───────┘  │
       │                │          │
       ▼                ▼          ▼
┌──────────────────────────────────────┐
│         visionflow-domain            │
│  (models, ports, events, types)      │
└──────────────────┬───────────────────┘
                   │
                   ▼
┌──────────────────────────────────────┐
│      visionclaw-contracts            │
│  (cross-boundary typed contracts)    │
└──────────────────────────────────────┘
```

## 3. Shared Kernel

The following types are shared across ALL bounded contexts via `visionflow-domain`:

- `NodeId` (u32), `EdgeId` (String)
- `Vec3Data` { x, y, z }
- `BinaryNodeData` { node_id, x, y, z, vx, vy, vz }
- `SimParams` (physics parameters)
- `SettleMode` (FastSettle / Continuous)
- `GraphData` { nodes, edges, metadata }
- Domain events (trait + enum)

## 4. Module-to-Crate Migration Map

| Source Module | Target Crate | Lines | Phase |
|--------------|-------------|-------|-------|
| `src/models/` | `visionflow-domain` | 3,256 | 1 |
| `src/types/` | `visionflow-domain` | 1,407 | 1 |
| `src/errors/` | `visionflow-domain` | 989 | 1 |
| `src/events/` | `visionflow-domain` | 3,150 | 1 |
| `src/ports/` | `visionflow-domain` | 1,323 | 1 |
| `src/protocol/` | `visionflow-protocol` | 303 | 2 |
| `src/protocols/` | `visionflow-protocol` | 590 | 2 |
| `src/gpu/` | `visionflow-gpu` | 9,036 | 3 |
| `src/physics/` | `visionflow-gpu` | 5,678 | 3 |
| `src/layout/` | `visionflow-gpu` | 605 | 3 |
| `src/constraints/` | `visionflow-gpu` | 4,804 | 3 |
| `src/ontology/` | `visionflow-ontology` | 1,371 | 4 |
| `src/inference/` | `visionflow-ontology` | 1,394 | 4 |
| `src/reasoning/` | `visionflow-ontology` | 480 | 4 |
| `src/validation/` | `visionflow-ontology` | 205 | 4 |
| `src/adapters/` | `visionflow-adapters` | 7,636 | 5 |
| `src/repositories/` | `visionflow-adapters` | 10 | 5 |
| `src/actors/` | `visionflow-actors` | 34,515 | 6 |
| `src/cqrs/` | `visionflow-actors` | 3,959 | 6 |
| `src/application/` | `visionflow-actors` | 4,912 | 6 |
| `src/handlers/` | `visionflow-server` | 35,145 | 7 |
| `src/middleware/` | `visionflow-server` | 1,149 | 7 |
| `src/config/` | `visionflow-server` | 4,001 | 7 |
| `src/settings/` | `visionflow-server` | 2,195 | 7 |
| `src/telemetry/` | `visionflow-server` | 735 | 7 |
| `src/services/` | `visionflow-server` | 29,217 | 7 |
| `src/utils/` | Split across crates | 25,264 | 1-7 |
| `src/client/` | `visionflow-server` | 373 | 7 |
