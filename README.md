<div align="center">

# VisionClaw

### The first platform where knowledge graph and ontology negotiate live.

[![Build](https://img.shields.io/github/actions/workflow/status/DreamLab-AI/VisionClaw/ci.yml?branch=main&style=flat-square&logo=github)](https://github.com/DreamLab-AI/VisionClaw/actions)
[![Version](https://img.shields.io/github/v/release/DreamLab-AI/VisionClaw?style=flat-square&logo=semantic-release)](https://github.com/DreamLab-AI/VisionClaw/releases)
[![License](https://img.shields.io/badge/License-MPL%202.0-blue?style=flat-square)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-2021-orange?style=flat-square&logo=rust)](https://www.rust-lang.org/)
[![CUDA](https://img.shields.io/badge/CUDA-13.1-76B900?style=flat-square&logo=nvidia)](https://developer.nvidia.com/cuda-toolkit)
[![Docs](https://img.shields.io/badge/Docs-Diataxis-4A90D9?style=flat-square)](docs/README.md)

**Your Logseq note from last Tuesday just became a governed OWL class. Notes become vocabulary as a by-product of note-taking — physics-visible, agent-proposed, human-brokered, Nostr-signed.**

<br/>

https://github.com/user-attachments/assets/f45c92dc-4800-4b57-a6e2-178da6bb0a38

<br/>

[The Migration Event](#the-migration-event) · [Quick Start](#quick-start) · [Positioning](#positioning--what-makes-this-different) · [Architecture](#architecture) · [Capabilities](#core-capabilities) · [Tradeoffs](#tradeoffs--honest-risks) · [Documentation](#documentation)

</div>

---

## The problem

Every organisation rots in the same two directions. Tribal knowledge accumulates in personal notes, Slack threads, and half-finished docs — rich and current but illegible to agents, query tools, or anyone who wasn't in the room. Meanwhile the formal ontology someone wrote in 2023 was out of date by 2024 — expensive to author, expensive to keep clean, and nobody uses it.

Agents sit between the two, forced to improvise shared meaning on every request. They hallucinate categories, fabricate entity relationships, and route on keyword overlap because there's no canonical vocabulary they can trust.

VisionClaw treats this as one problem with one mechanism: **let the notes become the vocabulary, under human governance, visibly, as the graph itself.**

---

## The Migration Event

Every capability below exists to serve this one event. A `public:: true` Logseq note accumulates signals — ontology wikilinks, agent proposals, authoring maturity — until it surfaces in a Judgment Broker's inbox. The broker takes two minutes, approves, the system opens a GitHub PR, and on merge the note becomes a live OWL class Whelk can reason over. The 3D physics view redraws around it. A Nostr-signed provenance bead closes the chain.

![The Migration Event — a Logseq note traverses detection, broker review, PR approval, merge, and physics re-settle in six stages](./docs/diagrams/06-migration-event.png)

The ontology grows from the bottom up, not the top down. Nothing formal is authored unless something informal earned it.

### The two tiers it bridges

![Dual-tier identity — KG tier (public notes) with BRIDGE_TO filaments crossing to the Ontology tier (OWL classes), candidate/promoted/colocated states, orphan zone](./docs/diagrams/07-dual-tier-identity.png)

`KGNode` (your notes, fast-moving) and `OntologyClass` (the formal canon, slow-moving) are different Neo4j labels sharing one canonical IRI scheme `vc:{domain}/{slug}`. They connect through `BRIDGE_TO` edges that advance `candidate → promoted` via the migration event — or stay `colocated` when a note and class are the same concept from two angles. See [ADR-048](docs/adr/ADR-048-dual-tier-identity-model.md) for the full identity model.

### How candidates are scored

![Eight-signal scoring radar — wikilink count, agent proposal, OWL declaration, maturity, centrality, recency, authority, cooccurrence — all feed a sigmoid with 0.60 threshold](./docs/diagrams/08-scoring-radar.png)

No LLM opinion-as-fact. Eight structural signals feed a sigmoid. Above 0.60 a candidate surfaces to the broker. Below 0.35 for three days it auto-expires. Agent confidence is *one* signal of eight — it does not bypass the gate. See [candidate-scoring.md](docs/design/2026-04-18-insight-migration-loop/05-candidate-scoring.md).

### Two loops, one mesh

![Dual ingestion loops — workflow and knowledge migration both instantiate the same Discover → Codify → Validate → Integrate → Amplify pattern](./docs/diagrams/11-dual-ingestion-loops.png)

Shadow workflows become governed patterns. Shadow concepts become governed classes. Same five-beat loop, different units. See [Insight Migration Loop master design](docs/design/2026-04-18-insight-migration-loop/00-master.md).

---

## Positioning — what makes this different

![Positioning quadrant — VisionClaw alone in the upper-right: the only tool that is both highly formal (OWL, Whelk reasoning) and highly interactive (physics, agents, real-time co-authoring)](./docs/diagrams/10-prior-art-quadrant.png)

Every axis of this tool has good prior art. Obsidian and Logseq do wikilinks beautifully. Protégé and TopBraid do formal OWL authoring. Palantir does enterprise ontology. NotebookLM does LLM over notebooks. Neo4j Bloom does graph interaction.

**Nobody puts them in the same room.** The upper-right quadrant — high formality *and* high interactivity — was empty because the migration *event* itself is the hard part. VisionClaw lives there. See [prior-art analysis](docs/design/2026-04-18-insight-migration-loop/01-prior-art.md) for the full comparison and [bridge-theory.md](docs/design/2026-04-18-insight-migration-loop/02-bridge-theory.md) for the academic anchors (SECI, OntoClean, FCA, ontology evolution).

---

## Physics the metadata makes possible

![Physics outcome — authoritative canon core, role bands (concept/process/agent), physicality clusters (abstract/concrete), draft orbital, orphan zone](./docs/diagrams/09-physics-outcome.png)

The CUDA physics is not decoration. It reads `owl:physicality`, `owl:role`, and `maturity` properties and turns them into five semantic forces: abstract clumps with abstract, processes band together, authoritative nodes anchor a stable core while drafts drift outward, orphaned notes (no ontology anchor yet) repel into a distinct zone that makes them visible as migration candidates.

92 CUDA kernel functions. 55× over single-threaded CPU. Every force is tunable per-domain. See [physics mapping](docs/design/2026-04-18-insight-migration-loop/03-physics-mapping.md).

---

## Quick Start

```bash
git clone https://github.com/DreamLab-AI/VisionClaw.git
cd VisionClaw && cp .env.example .env
docker-compose --profile dev up -d
```

| Service | URL | Description |
|:--------|:----|:------------|
| Frontend | http://localhost:3001 | 3D knowledge graph interface |
| API | http://localhost:4000/api | REST + WebSocket endpoints |
| Neo4j Browser | http://localhost:7474 | Graph database explorer |
| JSS Solid | http://localhost:3030 | Solid Pod server |

<details>
<summary><strong>Voice routing · Multi-user XR · Native Rust+CUDA build</strong></summary>

```bash
# Voice routing (LiveKit + whisper + TTS)
docker-compose -f docker-compose.yml -f docker-compose.voice.yml --profile dev up -d

# Multi-user XR (Vircadia World Server)
docker-compose -f docker-compose.yml -f docker-compose.vircadia.yml --profile dev up -d

# Native build
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
cargo build --release --features gpu
cd client && npm install && npm run build && cd ..
./target/release/webxr
```

Requires CUDA 13.1. See [Deployment Guide](docs/how-to/deployment-guide.md).

</details>

---

## Architecture

VisionClaw is a three-layer mesh. Insights bubble up from discovery, orchestrated through formal semantic pipelines, governed by declarative policy — with humans as the irreplaceable judgment layer at the top.

![The three layers of the VisionClaw governed mesh — declarative governance, orchestration, discovery](./docs/diagrams/01-three-layer-mesh.png)

Beneath those layers sits a hexagonal Rust backend with 21 Actix actors, 9 ports and 12 adapters, 114 CQRS handlers, CUDA 13.1 compute, and OWL 2 EL reasoning via Whelk-rs.

![System architecture — hexagonal ports/adapters, 21 supervised actors, CUDA kernels, dual graph store, agent mesh](./docs/diagrams/05-architecture-hexagonal.png)

<details>
<summary><strong>Hexagonal details · 21 actors · 17 DDD bounded contexts</strong></summary>

- **Core Domain**: Knowledge Graph · Ontology Governance · Physics Simulation · [Judgment Broker (BC11)](docs/explanation/ddd-enterprise-contexts.md) · Workflow Lifecycle (BC12) · Insight Discovery (BC13 — owns the Migration Candidate aggregate)
- **Supporting Domain**: Authentication · Enterprise Identity (BC14) · Agent Orchestration · Semantic Analysis · Policy Engine (BC17)
- **Generic Domain**: Bead Provenance · Configuration · KPI Observability (BC15) · Connector Ingestion (BC16)

Each context has its own aggregate roots, domain events, and anti-corruption layers. Cross-context communication uses domain events, never direct model sharing.

- [Core Bounded Contexts (BC1–BC10)](docs/explanation/ddd-bounded-contexts.md)
- [Enterprise Bounded Contexts (BC11–BC17)](docs/explanation/ddd-enterprise-contexts.md)
- [Insight Migration context refinement](docs/explanation/ddd-insight-migration-context.md)

</details>

---

## Sovereign Mesh Architecture (2026-04-19 sprint)

Every wikilink target becomes a first-class `:KGNode` regardless of publish state. Public nodes appear with full label and metadata; private nodes appear as topology-only (node shape and edges visible, label and metadata opacified via bit 29 on node_id in the V5 binary protocol and stripped from REST).

Each user's content lives in their own Solid Pod: `./public/kg/` for published pages (world-readable, canonical URI) and `./private/kg/` for working graph (owner-only). The Pod is write-master; the backend serves as performance layer, aggregation point, and physics engine. The system never writes outside the owner's container.

| Component | What it does | Source |
|-----------|--------------|--------|
| NIP-98 optional auth | Anonymous callers see public only; signed callers see own-private + opacified-others | `src/utils/auth.rs`, `src/middleware/auth.rs` |
| KGNode schema | visibility + owner_pubkey + opaque_id + pod_url, HMAC with daily salt rotation, bit 29 on wire | `src/models/node.rs`, `src/utils/binary_protocol.rs`, `src/utils/canonical_iri.rs`, `src/utils/opaque_id.rs` |
| Two-pass parser | Build wikilink adjacency, classify visibility per page, emit private stubs | `src/services/parsers/knowledge_graph_parser.rs`, `src/services/parsers/visibility.rs` |
| Pod-first ingest saga | Pod write → Neo4j commit, crash-safe with pending markers | `src/services/ingest_saga.rs`, `src/services/pod_client.rs` |
| BRIDGE_TO promotion | 8-signal sigmoid scoring, monotonic confidence invariant, orphan retraction | `src/services/bridge_edge.rs`, `src/services/orphan_retraction.rs` |
| Server-as-identity | Server signs kind 30023/30100/30200/30300 events (migration / bridge / bead / audit) | `src/services/server_identity.rs`, `src/actors/server_nostr_actor.rs`, `src/handlers/server_identity_handler.rs` |
| solid-pod-rs crate | Rust-native Solid Pod server (WAC + LDP + NIP-98 + FS/Memory backends); planned JSS parity | `crates/solid-pod-rs/` |
| Power-user CLI | `vc-cli bootstrap-power-user --env .env` writes GitHub creds to Pod | `src/bin/vc_cli.rs` |

### Solid Ecosystem Integration

VisionClaw aligns with two external ecosystems:

**URN-Solid registry** — We emit `owl:sameAs urn:solid:<Name>` on `:OntologyClass` entries where well-known vocabulary equivalents exist (Person, Document, Event, etc.). Each user's Pod publishes `./public/kg/corpus.jsonl` — a line-delimited JSON-LD snapshot following the URN-Solid registry generation convention (`scripts/build.js`). See the [URN-Solid registry](https://github.com/urn-solid/urn-solid.github.io) (dual-licensed: code + LICENSE-DATA).

**solid-schema** — JSON Schema 2020-12 contracts for `urn:solid:` types, sitting between vocabulary (URN-Solid) and runtime (LOSOS). We publish `./public/schema/kg-node.schema.json` following the solid-schema convention (JSON Schema 2020-12 + `x-urn-solid` extension with term id, status, and lineage) so the same contract can be submitted upstream for ecosystem-wide adoption. `solid-pod-rs` validates JSON-LD PUTs against user-published schemas via the `jsonschema` Rust crate. See the [solid-schema registry](https://github.com/solid-schema/solid-schema.github.io) (AGPL-3.0).

**Solid-Apps (LOSOS)** — We publish `./public/schema/kg-node.schema.json` and `./public/schema/manifest.jsonld` with the `urn:solid:KGNode` type binding so LOSOS apps built on LION + solid-schema + solid-panes + LOSOS can render any user's KG content directly from their Pod without VisionClaw-specific code. See the [Solid-Apps project](https://github.com/solid-apps/solid-apps.github.io) (AGPL-3.0 code, separate LICENSE-DATA).

This alignment is behind feature flag `URN_SOLID_ALIGNMENT=true` (default false in v1). Full specification: [ADR-054 — URN-Solid and Solid-Apps Alignment](docs/adr/ADR-054-urn-solid-and-solid-apps-alignment.md).

---

## Core capabilities

<table>
<tr>
<td width="50%">

**🪢 Insight Migration Loop** ([PRD](docs/prd-insight-migration-loop.md))
- 8-signal sigmoid scoring over `public:: true` notes
- Auto-surfacing at confidence ≥ 0.60
- Broker inbox with side-by-side KG/OWL diff
- One-click approve → auto GitHub PR
- Whelk EL++ consistency check before merge
- Physics re-settles; Nostr provenance bead closes chain

</td>
<td width="50%">

**🧠 Semantic Governance**
- OWL 2 EL reasoning via Whelk-rs
- `subClassOf` → attraction, `disjointWith` → repulsion
- Ontology mutations gate on GitHub PR
- Content-addressed provenance beads (Nostr NIP-09)
- 17 DDD bounded contexts · 114 CQRS handlers
- Policy engine · 6 built-in rules · TOML config

</td>
</tr>
<tr>
<td width="50%">

**⚡ GPU Physics**
- 92 CUDA kernels · 55× vs single-threaded CPU
- 5 semantic forces driven by OWL metadata
- Force-directed layout · stress majorisation
- K-Means · Louvain · LOF anomaly · PageRank
- Periodic full broadcast every 300 iterations

</td>
<td width="50%">

**🤖 Agent Mesh**
- Claude-Flow orchestration · RAFT hive-mind
- 7 MCP ontology tools · 83 agent skill modules
- Nostr NIP-98 signed agent identities
- `ontology_propose` tool emits PRs on demand
- RuVector PostgreSQL · pgvector + HNSW
- 1.17M+ memory entries · 61µs p50 semantic search

</td>
</tr>
<tr>
<td width="50%">

**🌐 Immersive XR**
- Babylon.js WebXR · Quest 3 optimised
- React Three Fiber for desktop graph
- Vircadia avatar sync · HRTF spatial audio
- WebGPU with TSL shaders · WebGL fallback
- Foveated rendering · dynamic resolution

</td>
<td width="50%">

**🔐 Dual-Stack Identity**
- Enterprise OIDC/SAML (Entra, Okta, Google)
- Nostr NIP-98 signed HTTP auth for provenance
- Ephemeral keypair delegation (OIDC → secp256k1)
- Solid Pods · per-user data sovereignty
- 4 roles: Broker · Admin · Auditor · Contributor

</td>
</tr>
</table>

<details>
<summary><strong>Voice routing — four planes, spatial HRTF audio</strong></summary>

![Four-plane voice architecture — private 1:1 agent dialogue (Planes 1-2) and public spatial audio via LiveKit SFU (Planes 3-4)](./docs/diagrams/03-four-plane-voice.png)

| Plane | Direction | Scope |
|:------|:----------|:------|
| 1 | User mic → turbo-whisper STT → Agent | Private (PTT held) |
| 2 | Agent → Kokoro TTS → User ear | Private |
| 3 | User mic → LiveKit SFU → All users | Public spatial |
| 4 | Agent TTS → LiveKit → All users | Public spatial |

Opus 48 kHz mono end-to-end. HRTF spatial panning from Vircadia entity positions. See [voice routing guide](docs/how-to/features/voice-routing.md).

</details>

<details>
<summary><strong>MCP ontology tools — seven spokes feeding Whelk-rs</strong></summary>

![MCP tools radial — 7 ontology tools radiating from Whelk-rs core, serving 83 Claude-Flow agents and bridged to Neo4j + GitHub](./docs/diagrams/04-mcp-tools-radial.png)

`ontology_discover` · `ontology_read` · `ontology_query` · `ontology_traverse` · `ontology_propose` · `ontology_validate` · `ontology_status`. Every tool goes through Whelk for consistency. `ontology_propose` is the agent entry point to the Migration Event — it drafts the PR a broker will approve.

</details>

---

## Agent skill domains (83 skills)

<details>
<summary><strong>Full skill breakdown</strong></summary>

Creative production · Research & synthesis · Knowledge codification · Governance & audit · Workflow discovery · Financial intelligence · Spatial/immersive · Identity & trust · Development & quality · Infrastructure & DevOps · Document processing.

![Skills ecosystem — 83 specialised Claude-Flow skill modules with RAFT consensus](./docs/diagrams/skills-ecosystem-art-detailed.png)

See the [agents catalog](docs/reference/agents-catalog.md) for the full skill taxonomy and the [Logseq ontology bridge](docs/explanation/ontology-pipeline.md) for how skills participate in the migration event.

</details>

---

## Tradeoffs — honest risks

This is a working existence proof at small-to-medium scale, not a universal solution. These are real:

**Operational complexity.** Running the full stack touches Rust, Node, Python, CUDA, Neo4j, PostgreSQL (RuVector + Vircadia), Redis, LiveKit (WebRTC), OpenSearch, and Solid Pod sidecars. Teams under five engineers will spend real time on DevOps. The single-command `docker-compose up` hides most of it; the breakage surface doesn't go away.

**O(n²) physics beyond ~5k nodes.** The current physicality and role cluster kernels iterate nodes against centroids in a loop that's fine at 2.5k nodes (~12 ms per tick) but breaks the 60 Hz frame budget at 10k. A centroid-approximation rewrite is documented as a required follow-on before scaling up. See [physics mapping §Performance envelope](docs/design/2026-04-18-insight-migration-loop/03-physics-mapping.md).

**V4 delta protocol is unstable.** The binary V4 variant causes position drift. Production uses V5 (full frame with 9-byte sequence header). See [KNOWN_ISSUES WS-001](docs/KNOWN_ISSUES.md).

**State fragmentation across stores.** Notes live in Logseq/GitHub, graph structure in Neo4j, agent memory in RuVector PostgreSQL, user data in Solid Pods. Race conditions on partial failure (e.g. GitHub PR merges but Neo4j write fails) are documented and handled, but the surface is large. See [ADR-034 on provenance beads as the rollback substrate](docs/adr/ADR-034-needle-bead-provenance.md).

**Pilot scope.** The first pilot is consultancy context (see [PRD personas](docs/prd-insight-migration-loop.md)) because consulting firms dogfood us on their own methodology. Regulated industry (pharma, finance) is v2. Research institutes are v2+. No claim of universal fit today.

---

## The platform in numbers

| Metric | Value | Conditions |
|:-------|-------:|:-----------|
| GPU physics speedup | 55× | vs single-threaded CPU |
| CUDA kernels | 92 | 6,585 LOC across 11 files |
| HNSW semantic search | 61 µs p50 | RuVector pgvector, 1.17M entries |
| WebSocket latency | 10 ms | Local network, V5 binary |
| Bandwidth reduction | 80% | Binary V5 vs JSON |
| Concurrent XR users | 250+ | Related immersive event |
| Frame size | 9-byte header + 36 bytes/node | V5 production |
| Physics convergence | ~600 frames (~10 s) | Typical graph at rest |
| DDD bounded contexts | 17 | with 114 CQRS handlers |
| Agent skills | 83 | Claude-Flow hive-mind |

---

## Enterprise roadmap

| Phase | Weeks | Deliverable | Exit criterion |
|:------|:------|:------------|:---------------|
| **0** Platform coherence | 1–6 | Node types, binary protocol, position flow, settings, ontology edges | No contradictions between story and behaviour |
| **1** Identity + Broker MVP | 7–14 | OIDC, roles, Broker Inbox, Decision Canvas | Broker can log in, review, decide on real cases |
| **2** Insight Ingestion + Migration | 15–24 | WorkflowProposal and MigrationCandidate lifecycles, GitHub connector, promotion | One shadow concept and one shadow workflow promoted live |
| **3** KPI + Governance | 25–32 | Six mesh KPIs, policy engine, exportable audit reports | Thesis KPIs measurable from production data |
| **4** Pilot | 33–44 | Consultancy pilot (Idris persona), connector hardening, success reporting | At least one paid pilot running |

Key architecture decisions: [ADR-040](docs/adr/ADR-040-enterprise-identity-strategy.md) · [ADR-041](docs/adr/ADR-041-judgment-broker-workbench.md) · [ADR-042](docs/adr/ADR-042-workflow-proposal-object-model.md) · [ADR-043](docs/adr/ADR-043-kpi-lineage-model.md) · [ADR-044](docs/adr/ADR-044-connector-governance-privacy.md) · [ADR-045](docs/adr/ADR-045-policy-engine-approach.md) · [ADR-046](docs/adr/ADR-046-enterprise-ui-architecture.md) · [ADR-047](docs/adr/ADR-047-wasm-visualization-components.md) · [ADR-048](docs/adr/ADR-048-dual-tier-identity-model.md) · [ADR-049](docs/adr/ADR-049-insight-migration-broker-workflow.md)

Sovereign-mesh ADRs (Wave 1–4): [ADR-028-ext](docs/adr/ADR-028-ext-nip98-optional-caller-aware.md) · [ADR-030-ext](docs/adr/ADR-030-ext-sovereign-schema-transitions-creds.md) · [ADR-050](docs/adr/ADR-050-sovereign-knowledge-graph-model.md) · [ADR-051](docs/adr/ADR-051-pod-first-ingest-saga.md) · [ADR-052](docs/adr/ADR-052-wac-knode-visibility-control.md) · [ADR-053](docs/adr/ADR-053-binary-v5-opaque-nodes.md) · [ADR-054](docs/adr/ADR-054-urn-solid-and-solid-apps-alignment.md)

---

## Technology stack

<details>
<summary><strong>Full technology breakdown</strong></summary>

| Layer | Technology | Detail |
|:------|:-----------|:-------|
| **Backend** | Rust 2021 · Actix-web | 427 files · hexagonal CQRS · 9 ports · 12 adapters · 114 handlers |
| **Frontend (desktop)** | React 19 · Three.js · R3F | TypeScript 5.9 · InstancedMesh · SAB zero-copy |
| **Frontend (XR)** | Babylon.js | Quest 3 foveated rendering · hand tracking |
| **WASM** | Rust → wasm-pack | scene-effects · drawer-fx crates · zero-copy Float32Array |
| **Graph DB** | Neo4j 5.13 | Primary store · Cypher · bolt protocol |
| **Vector Memory** | RuVector · pgvector | 1.17M entries · HNSW 384-dim · MiniLM-L6-v2 |
| **GPU** | CUDA 13.1 · cudarc | 92 kernels · 6,585 LOC · PTX ISA auto-downgrade |
| **Ontology** | OWL 2 EL · Whelk-rs | EL++ subsumption · consistency checking |
| **XR** | WebXR · Babylon.js | Meta Quest 3 · hand tracking |
| **Multi-User** | Vircadia World Server | Avatar sync · HRTF spatial · collaborative editing |
| **Voice** | LiveKit · turbo-whisper · Kokoro | CUDA STT · TTS · Opus 48 kHz |
| **Identity** | Nostr NIP-07/NIP-98 | Browser ext signing · Schnorr HTTP auth · scoped delegation |
| **User Data** | Solid Pods · JSS | Per-user sovereignty · WAC access control · JSON-LD |
| **Agents** | Claude-Flow · MCP · RAFT | 83 skills · 7 ontology tools · hive-mind consensus |
| **Build** | Vite 6 · Vitest · Playwright | Frontend build · unit/E2E tests |
| **Infra** | Docker Compose | 15+ services · multi-profile |

</details>

---

## Documentation

VisionClaw uses the [Diataxis](https://diataxis.fr/) framework — 144 markdown files across four categories, 54 with embedded Mermaid diagrams.

Entry points:

- [Full Documentation Hub](docs/README.md)
- [Insight Migration Loop — master design](docs/design/2026-04-18-insight-migration-loop/00-master.md) · [PRD](docs/prd-insight-migration-loop.md) · [explanation](docs/explanation/insight-migration-loop.md) · [tutorial](docs/tutorials/promoting-a-note-to-ontology.md)
- [System Overview](docs/explanation/system-overview.md) · [Architecture Self-Review](docs/architecture-self-review.md)
- [Deployment Guide](docs/how-to/deployment-guide.md) · [Quest 3 VR Setup](docs/how-to/xr-setup-quest3.md)
- [Agent Orchestration](docs/how-to/agent-orchestration.md) · [REST API](docs/reference/rest-api.md) · [WebSocket V5 Binary](docs/reference/websocket-binary.md)
- [Known Issues](docs/KNOWN_ISSUES.md) — read before debugging

---

## Development

### Prerequisites

| Tool | Version | Purpose |
|:-----|:--------|:--------|
| Rust | 2021 edition | Backend |
| Node.js | 20+ | Frontend |
| Docker + Docker Compose | — | Services |
| CUDA Toolkit | 13.1 | GPU acceleration (optional) |

### Build and test

```bash
cargo build --release --features gpu
cargo test --features gpu
cd client && npm install && npm run build && npm test
cargo test --test ontology_agent_integration_test
```

### System tiers

| Tier | CPU | RAM | GPU | Use case |
|:-----|:----|:----|:----|:---------|
| **Minimum** | 4-core 2.5 GHz | 8 GB | Integrated | Development · < 10K nodes |
| **Recommended** | 8-core 3.0 GHz | 16 GB | GTX 1060 / RX 580 | Production · < 50K nodes |
| **Enterprise** | 16+ cores | 32 GB+ | RTX 4080+ (16 GB) | Large graphs · multi-user XR |

**Platforms**: Linux (full GPU) · macOS (CPU only) · Windows (WSL2) · Meta Quest 3.

---

## Operational context

| Deployment | Context | Scale |
|:-----------|:--------|:------|
| **DreamLab Creative Hub** | Live creative-technology deployment | ~998 knowledge graph nodes, daily ontology mutations |
| **University of Salford** | Research collaboration — semantic force-directed layout | Multi-institution ontology |
| **THG World Record** | Large-scale immersive data visualisation event | 250+ concurrent XR users |

---

## Contributing

See the [Contributing Guide](docs/CONTRIBUTING.md). Before contributing check [Known Issues](docs/KNOWN_ISSUES.md) — ONT-001 and WS-001 are active P1/P2 bugs that may touch your area.

---

## License

[Mozilla Public License 2.0](LICENSE) — Use commercially, modify freely, share changes to MPL files.

---

<div align="center">

**VisionClaw is built by [DreamLab AI Consulting](https://www.dreamlab-ai.com).**

[Documentation](docs/README.md) · [Known Issues](docs/KNOWN_ISSUES.md) · [Discussions](https://github.com/DreamLab-AI/VisionClaw/discussions) · [Issues](https://github.com/DreamLab-AI/VisionClaw/issues)

</div>
