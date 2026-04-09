---
title: VisionClaw Reference Index
description: Master index for all VisionClaw reference documentation — APIs, schemas, protocols, configuration, and system specifications
category: reference
tags: [index, reference, api, schema, protocol, configuration]
updated-date: 2026-04-09
---

# VisionClaw Reference Index

Quick-lookup index for all reference documentation. For prose explanations see [docs/explanation/](../explanation/). For task guides see [docs/how-to/](../how-to/).

[← Documentation Hub](../README.md)

---

## API Reference

| Reference | Description |
|-----------|-------------|
| [REST API](rest-api.md) | All HTTP endpoints — graph data, settings, authentication, ontology, pathfinding, and Solid Pod operations |
| [WebSocket Binary Protocol](websocket-binary.md) | Binary V2/V3/V4 message formats, 48-byte position frame layout, connection lifecycle, and client implementation guide |

---

## Database

| Reference | Description |
|-----------|-------------|
| [Neo4j Schema](neo4j-schema-unified.md) | Unified schema for all Neo4j node and relationship types — graph nodes/edges, OWL classes/axioms, Solid Pod records, user settings, and Cypher indexes |

---

## Protocols

| Reference | Description |
|-----------|-------------|
| [WebSocket Binary Protocol](websocket-binary.md) | Binary WebSocket V2/V3/V4 wire formats, type flags, delta encoding |
| [MCP Protocol](protocols/mcp-protocol.md) | Model Context Protocol JSON-RPC 2.0 specification for agent tool calls |
| [Protocol Matrix](protocols/protocol-matrix.md) | Side-by-side comparison of all transport protocols (REST, WebSocket, MCP) |

---

## Agents

| Reference | Description |
|-----------|-------------|
| [Agents Catalog](agents-catalog.md) | Complete catalog of 83 specialist agent skills organised by domain with invocation patterns |
| [Skill MCP Classification](protocols/skill-mcp-classification.md) | Routing classification mapping agent skills to their MCP server implementations |

---

## Configuration

| Reference | Description |
|-----------|-------------|
| [Environment Variables](configuration/environment-variables.md) | All `.env` variables — Neo4j, GPU, auth, network, feature flags — with types, defaults, and descriptions |
| [Docker Compose Options](configuration/docker-compose-options.md) | Service profiles, named volumes, compose file structure, and multi-profile deployment |

---

## CLI

| Reference | Description |
|-----------|-------------|
| [Cargo Commands](cli/cargo-commands.md) | Rust `cargo build`, `cargo test`, `cargo clippy`, and release commands |
| [Docker Commands](cli/docker-commands.md) | `docker` and `docker-compose` operational commands for all service profiles |

---

## System

| Reference | Description |
|-----------|-------------|
| [Error Codes](error-codes.md) | Complete error code hierarchy — AP-E (API), DB-E (database), GR-E (graph/ontology), GP-E (GPU/physics), WS-E (WebSocket) — with solutions |
| [Glossary](glossary.md) | Definitions for all domain-specific terms used in VisionClaw documentation |
| [Performance Benchmarks](performance-benchmarks.md) | GPU physics speedup, WebSocket latency, binary protocol bandwidth savings, and API response times |
