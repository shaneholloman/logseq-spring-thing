---
title: VisionClaw Documentation
description: Complete documentation for VisionClaw — the governed agentic mesh for real-time 3D knowledge graph exploration with GPU-accelerated physics, OWL 2 ontology reasoning, and multi-agent AI orchestration
updated-date: 2026-04-09
---

# VisionClaw Documentation

> **Real-time 3D knowledge graph exploration** powered by Rust, CUDA GPU physics, OWL 2 ontology reasoning, and a multi-agent AI mesh.

[← Back to Project](../README.md) | [Quick Start](#quick-start) | [API Reference](reference/rest-api.md) | [Architecture](explanation/system-overview.md)

---

## Quick Start

```bash
git clone https://github.com/DreamLab-AI/VisionClaw.git
cd VisionClaw && cp .env.example .env
docker-compose --profile dev up -d
```

Open [http://localhost:3001](http://localhost:3001) for the 3D graph interface, [http://localhost:4000/api](http://localhost:4000/api) for the REST API, and [http://localhost:7474](http://localhost:7474) for the Neo4j browser.

Full setup details: [Deployment Guide](how-to/deployment-guide.md)

> **Known Issues**: Before debugging unexpected behaviour, check [KNOWN_ISSUES.md](KNOWN_ISSUES.md) — it tracks active P1/P2 bugs including the Ontology Edge Gap (ONT-001) and V4 delta instability (WS-001).

---

## Documentation Map

```mermaid
graph LR
    subgraph "Tutorials"
        T1[Installation]
        T2[First Graph]
        T3[Neo4j Basics]
    end
    subgraph "How-To"
        H1[Deployment Guide]
        H2[Development Guide]
        H3[Agent Orchestration]
        H4[Features]
        H5[Operations]
    end
    subgraph "Explanation"
        E1[System Overview]
        E2[Backend CQRS]
        E3[Actor Hierarchy]
        E4[Client Architecture]
        E5[Ontology Pipeline]
        E6[Physics/GPU Engine]
        E7[XR Architecture]
    end
    subgraph "Reference"
        R1[REST API]
        R2[WebSocket Binary]
        R3[Neo4j Schema]
        R4[Agents Catalog]
        R5[Config & Env]
    end
```

---

## Tutorials

Step-by-step lessons that teach VisionClaw by doing.

| Tutorial | Description |
|----------|-------------|
| [What is VisionClaw?](tutorials/overview.md) | Platform overview and key concepts |
| [Installation](tutorials/installation.md) | Docker and native setup from zero |
| [Creating Your First Graph](tutorials/first-graph.md) | Build and explore your first knowledge graph |
| [Neo4j Basics](tutorials/neo4j-basics.md) | Query and navigate the graph database |

---

## How-To Guides

Practical task-oriented instructions. See [how-to/README.md](how-to/README.md) for the full index.

### Deployment & Infrastructure

| Guide | Description |
|-------|-------------|
| [Deployment Guide](how-to/deployment-guide.md) | Docker Compose production deployment with NVIDIA GPU |
| [Performance Profiling](how-to/performance-profiling.md) | GPU physics, WebSocket, render, and Neo4j bottleneck detection |
| [Quest 3 VR Setup](how-to/xr-setup-quest3.md) | Connect a Meta Quest 3 to VisionClaw's immersive XR mode |
| [Infrastructure Overview](how-to/infrastructure/goalie-integration.md) | Goalie integration and infra architecture |
| [Port Configuration](how-to/infrastructure/port-configuration.md) | Service port mapping and networking |
| [Infrastructure Tools](how-to/infrastructure/tools.md) | Container management and diagnostic tools |
| [Infrastructure Troubleshooting](how-to/infrastructure/troubleshooting.md) | Container and networking issues |

### Development

| Guide | Description |
|-------|-------------|
| [Development Guide](how-to/development-guide.md) | Rust/React setup, project structure, testing workflow |
| [REST API Integration Guide](how-to/rest-api-guide.md) | NIP-98 auth, common API workflows, WebSocket combination patterns |

### Agent Orchestration

| Guide | Description |
|-------|-------------|
| [Agent Orchestration](how-to/agent-orchestration.md) | Deploy, configure, and coordinate the multi-agent AI system |

### Features

| Guide | Description |
|-------|-------------|
| [Navigation Guide](how-to/navigation-guide.md) | 3D interface controls and spatial navigation |
| [Filtering Nodes](how-to/features/filtering-nodes.md) | Graph node and edge filtering |
| [Intelligent Pathfinding](how-to/features/intelligent-pathfinding.md) | Semantic shortest-path traversal |
| [Natural Language Queries](how-to/features/natural-language-queries.md) | Plain-English graph search |
| [Semantic Forces](how-to/features/stress-majorization-guide.md) | Stress-majorisation layout algorithm |
| [Voice Routing](how-to/features/voice-routing.md) | 4-plane voice architecture with LiveKit |
| [Voice Integration](how-to/features/voice-integration.md) | STT/TTS pipeline setup |
| [Nostr Auth](how-to/features/nostr-auth.md) | NIP-07/NIP-98 authentication |
| [Auth & User Settings](how-to/features/auth-user-settings.md) | User settings and session management |
| [Ontology Parser](how-to/features/ontology-parser.md) | OWL 2 parsing from Logseq Markdown |
| [Hierarchy Integration](how-to/features/hierarchy-integration.md) | Class hierarchy visualisation |
| [Local File Sync](how-to/features/local-file-sync-strategy.md) | GitHub-to-local file synchronisation |
| [ComfyUI Setup](how-to/comfyui-sam3d-setup.md) | ComfyUI SAM3D integration setup |

### Operations & Integration

| Guide | Description |
|-------|-------------|
| [Configuration](how-to/operations/configuration.md) | Environment variables and runtime settings |
| [Troubleshooting](how-to/operations/troubleshooting.md) | Common issues and diagnostic steps |
| [Security](how-to/operations/security.md) | Authentication, secrets management, and hardening |
| [Telemetry & Logging](how-to/operations/telemetry-logging.md) | Observability and log configuration |
| [Pipeline Admin API](how-to/operations/pipeline-admin-api.md) | Admin endpoints for pipeline management |
| [Operator Runbook](how-to/operations/pipeline-operator-runbook.md) | Production operations playbook |
| [Maintenance](how-to/operations/maintenance.md) | Routine maintenance and upkeep tasks |
| [Neo4j Integration](how-to/integration/neo4j-integration.md) | Neo4j database connection and migration |
| [Solid Integration](how-to/integration/solid-integration.md) | Solid Pod integration overview |
| [Solid Pod Creation](how-to/integration/solid-pod-creation.md) | Creating and managing user Solid Pods |
| [ComfyUI Service](how-to/integration/comfyui-service-integration.md) | ComfyUI Docker service integration |

---

## Explanation

Conceptual deep-dives that build understanding of how and why VisionClaw works.

| Document | What it explains |
|----------|-----------------|
| [System Overview](explanation/system-overview.md) | End-to-end architectural blueprint — all layers and their interactions |
| [Backend CQRS Pattern](explanation/backend-cqrs-pattern.md) | Hexagonal architecture with 9 ports, 12 adapters, 114 command/query handlers |
| [Actor Hierarchy](explanation/actor-hierarchy.md) | 21-actor Actix supervision tree — roles, message protocols, failure strategies |
| [Client Architecture](explanation/client-architecture.md) | React + Three.js component hierarchy, WebGL rendering pipeline, WASM integration |
| [DDD Bounded Contexts](explanation/ddd-bounded-contexts.md) | Domain-Driven Design context map and aggregate boundaries |
| [DDD Identity Contexts](explanation/ddd-identity-contexts.md) | DID/Nostr + PodKey + Passkey identity bounded contexts |
| [DDD Semantic Pipeline](explanation/ddd-semantic-pipeline.md) | Semantic pipeline domain model and context boundaries |
| [Ontology Pipeline](explanation/ontology-pipeline.md) | GitHub Markdown → OWL 2 → Whelk reasoning → Neo4j → GPU constraints |
| [Physics & GPU Engine](explanation/physics-gpu-engine.md) | CUDA force-directed physics, semantic forces, 55× GPU speedup |
| [XR Architecture](explanation/xr-architecture.md) | WebXR / Babylon.js immersive mode, Vircadia multi-user integration |
| [Security Model](explanation/security-model.md) | Nostr DID auth, Solid Pod sovereignty, CQRS authorization, audit trail |
| [Solid Sidecar Architecture](explanation/solid-sidecar-architecture.md) | JSON Solid Server sidecar for user Pod storage |
| [User-Agent Pod Design](explanation/user-agent-pod-design.md) | Per-user Solid Pod isolation for agent memory |
| [Technology Choices](explanation/technology-choices.md) | Rationale for Rust, CUDA, Neo4j, OWL 2, and Three.js selections |
| [RuVector Integration](explanation/ruvector-integration.md) | RuVector PostgreSQL as AI agent memory substrate |
| [Blender MCP Architecture](explanation/blender-mcp-unified-architecture.md) | Blender remote-control via WebSocket RPC + MCP tools |
| [Deployment Topology](explanation/deployment-topology.md) | Multi-container service map, network architecture, dependency chain, scaling |
| [Agent-Physics Bridge](explanation/agent-physics-bridge.md) | How AI agent lifecycle states synchronise to the 3D physics simulation |

---

## Reference

Technical specifications for APIs, schemas, protocols, and configuration.

Full reference index: [reference/INDEX.md](reference/INDEX.md)

| Reference | Contents |
|-----------|----------|
| [REST API](reference/rest-api.md) | All HTTP endpoints — graph, settings, ontology, auth, pathfinding, Solid |
| [WebSocket Binary Protocol](reference/websocket-binary.md) | Binary V2/V3/V4 message formats, connection lifecycle, client implementation |
| [Neo4j Schema](reference/neo4j-schema-unified.md) | Graph node/edge types, ontology nodes, Solid Pod records, indexes |
| [Agents Catalog](reference/agents-catalog.md) | Complete catalog of specialist agent skills by domain |
| [Error Codes](reference/error-codes.md) | AP-E, DB-E, GR-E, GP-E, WS-E error code hierarchy with solutions |
| [Glossary](reference/glossary.md) | Definitions for domain terms used throughout the documentation |
| [Performance Benchmarks](reference/performance-benchmarks.md) | GPU physics, WebSocket, and API performance metrics |
| [Environment Variables](reference/configuration/environment-variables.md) | All `.env` variables with types, defaults, and descriptions |
| [Docker Compose Options](reference/configuration/docker-compose-options.md) | Service profiles, volumes, and compose file structure |
| [MCP Protocol](reference/protocols/mcp-protocol.md) | Model Context Protocol specification for agent orchestration |
| [Protocol Matrix](reference/protocols/protocol-matrix.md) | Transport protocol comparison — WebSocket, REST, MCP |
| [Cargo Commands](reference/cli/cargo-commands.md) | Rust build, test, and lint commands |
| [Docker Commands](reference/cli/docker-commands.md) | Docker and docker-compose operational commands |

---

## Architecture Decision Records

Design decisions recorded as ADRs in [docs/adr/](adr/).

| ADR | Title |
|-----|-------|
| [ADR-011](adr/ADR-011-auth-enforcement.md) | Authentication Enforcement |
| [ADR-012](adr/ADR-012-websocket-store-decomposition.md) | WebSocket Store Decomposition |
| [ADR-013](adr/ADR-013-render-performance.md) | Render Performance Strategy |
| [ADR-014](adr/ADR-014-semantic-pipeline-unification.md) | Semantic Pipeline Unification |
| [ADR-027](adr/ADR-027-pod-backed-graph-views.md) | Pod-Backed Graph Views |
| [ADR-028](adr/ADR-028-sparql-patch-ontology.md) | SPARQL PATCH for Ontology Mutations |
| [ADR-029](adr/ADR-029-type-index-discovery.md) | Type Index Discovery |
| [ADR-030](adr/ADR-030-agent-memory-pods.md) | Agent Memory Pods |
| [RVF Integration AFD](adr/rvf-integration-afd.md) | RuVector Federation Architecture Feature Design |
| [RVF Integration DDD](adr/rvf-integration-ddd.md) | RuVector Federation Domain-Driven Design |
| [RVF Integration PRD](adr/rvf-integration-prd.md) | RuVector Federation Product Requirements |

---

## Design Documents

Exploratory design documents in [docs/design/](design/).

- [Nostr Relay Integration](design/nostr-relay-integration.md) — Architecture for VisionClaw ↔ Nostr relay bridging
- [Nostr Solid Browser Extension](design/nostr-solid-browser-extension.md) — Browser extension design for Nostr + Solid identity

---

## Other

| Document | Description |
|----------|-------------|
| [Testing Guide](testing/TESTING_GUIDE.md) | Unit, integration, and E2E testing strategy |
| [Security](security.md) | Security model, threat surface, and hardening guidance |
| [Infrastructure Inventory](infrastructure-inventory.md) | Container services, ports, and environment inventory |
| [Contributing](CONTRIBUTING.md) | Contribution workflow, branching conventions, code standards |
| [Changelog](CHANGELOG.md) | Version history and release notes |
| [Use Cases](use-cases/README.md) | Industry use cases and case studies |
| [Git Support](git-support.md) | Git workflow and branching strategy |

---

*Maintained by DreamLab AI — [Issues](https://github.com/DreamLab-AI/VisionClaw/issues) | [Discussions](https://github.com/DreamLab-AI/VisionClaw/discussions)*
