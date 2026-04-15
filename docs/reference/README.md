---
title: Reference Documentation
description: Technical reference for APIs, configuration, protocols, and database schemas
category: reference
difficulty-level: intermediate
tags:
  - reference
  - api
  - configuration
  - protocols
  - database
updated-date: 2025-01-29
---

# VisionClaw Reference Documentation

Complete technical reference documentation for VisionClaw APIs, protocols, configurations, database schemas, and error codes.

---

## Reference Sections

| Section | Description |
|---------|-------------|
| **[REST API](./rest-api.md)** | All REST endpoints with request/response schemas |
| **[WebSocket Binary Protocol](./websocket-binary.md)** | V2/V3/V4 binary protocol specification |
| **[Neo4j Schema](./neo4j-schema-unified.md)** | Unified graph database schema across all bounded contexts |
| **[Agents Catalog](./agents-catalog.md)** | All 54 agent skills with invocation patterns |
| **[Configuration](./configuration/README.md)** | Environment variables, Docker Compose options |
| **[Protocol Reference](./protocols/README.md)** | MCP protocol, skill classification |
| **[CLI Reference](./cli/README.md)** | Cargo and Docker command reference |
| **[Error Codes](./error-codes.md)** | Complete error code reference with solutions |
| **[Glossary](./glossary.md)** | Technical term definitions |

---

## Quick Access

### API Reference

| Document | Description |
|----------|-------------|
| [REST API](./rest-api.md) | All REST endpoints — Nostr NIP-98 auth, graph, settings, ontology, Solid |
| [WebSocket Binary Protocol](./websocket-binary.md) | V2 (36-byte), V3 analytics, V4 compact — V1 JSON is removed |

### Configuration

| Document | Description |
|----------|-------------|
| [Environment Variables](./configuration/environment-variables.md) | All env var options |
| [Docker Compose](./configuration/docker-compose-options.md) | Container configuration |

### Protocols

| Document | Description |
|----------|-------------|
| [Binary WebSocket](./websocket-binary.md) | V2/V3/V4 wire formats |
| [MCP Protocol](./protocols/mcp-protocol.md) | Agent orchestration protocol |

### Database

| Document | Description |
|----------|-------------|
| [Neo4j Schema (Unified)](./neo4j-schema-unified.md) | Graph database schema including ontology storage |

### CLI

| Document | Description |
|----------|-------------|
| [Cargo Commands](./cli/cargo-commands.md) | Rust build, test, run |
| [Docker Commands](./cli/docker-commands.md) | Docker Compose commands |

---

## Additional Documentation

### Specialized References

| Document | Description |
|----------|-------------|
| [Performance Benchmarks](./performance-benchmarks.md) | Performance metrics and targets |
| Implementation Status | Feature implementation status (see main README) |
| Code Quality | Code quality metrics (see main README) |

### API Deep Dives

| Document | Description |
|----------|-------------|
| [REST API](./rest-api.md) | All REST endpoints including pathfinding, semantic features, and Solid pod integration |

---

## Documentation Standards

### Frontmatter Format

All reference documents use standardised frontmatter:

```yaml
---
title: Document Title
description: Brief description
category: reference
difficulty-level: intermediate
updated-date: 2025-01-29
---
```

### Difficulty Levels

| Level | Audience |
|-------|----------|
| `beginner` | New users |
| `intermediate` | Experienced users |
| `advanced` | System architects, contributors |

---

## Related Documentation

### Guides

- [Configuration Guide](../how-to/operations/configuration.md) - Practical examples
- [Deployment Guide](../how-to/deployment-guide.md) - Production deployment
- [Troubleshooting Guide](../how-to/operations/troubleshooting.md) - Common issues

### Concepts

- [Architecture Overview](../explanation/system-overview.md) - System architecture
- [Data Flow](../explanation/backend-cqrs-pattern.md) - Data flow diagrams

### Getting Started

- [Installation Guide](../tutorials/installation.md) - Setup instructions
- [First Graph](../tutorials/first-graph.md) - Quick start tutorial

---

**Last Updated**: January 29, 2025
