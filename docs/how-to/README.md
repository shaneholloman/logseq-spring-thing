---
title: How-To Guides
description: Practical task-oriented instructions for deploying, developing, operating, and extending VisionClaw
category: how-to
tags: [guides, how-to, deployment, development, operations, features]
updated-date: 2026-04-09
---

# How-To Guides

Practical instructions for specific tasks. Each guide assumes you already have VisionClaw running; if not, start with [Installation](../tutorials/installation.md).

[← Documentation Hub](../README.md)

---

## Deployment & Infrastructure

| Guide | Description |
|-------|-------------|
| [Deployment Guide](deployment-guide.md) | Docker Compose production deployment with NVIDIA GPU, environment configuration, and service profiles |
| [Goalie Integration](infrastructure/goalie-integration.md) | Goalie reverse proxy integration and infrastructure architecture |
| [Port Configuration](infrastructure/port-configuration.md) | Service port mapping, firewall rules, and networking |
| [Infrastructure Tools](infrastructure/tools.md) | Container management and diagnostic tooling |
| [Infrastructure Troubleshooting](infrastructure/troubleshooting.md) | Container crashes, networking issues, and GPU detection failures |

---

## Development

| Guide | Description |
|-------|-------------|
| [Development Guide](development-guide.md) | Rust/React local setup, project structure, testing workflow, and adding new features |

---

## Agent Orchestration

| Guide | Description |
|-------|-------------|
| [Agent Orchestration](agent-orchestration.md) | Deploy, configure, and coordinate the multi-agent AI system via Docker, MCP tools, and the WebUI |

---

## Features

| Guide | Description |
|-------|-------------|
| [Navigation Guide](navigation-guide.md) | 3D interface controls, camera movement, and spatial navigation |
| [Filtering Nodes](features/filtering-nodes.md) | Filter graph nodes and edges by type, label, or property |
| [Intelligent Pathfinding](features/intelligent-pathfinding.md) | Semantic shortest-path traversal between graph nodes |
| [Natural Language Queries](features/natural-language-queries.md) | Plain-English search over the knowledge graph |
| [Stress Majorisation](features/stress-majorization-guide.md) | Stress-majorisation graph layout algorithm guide |
| [Voice Routing](features/voice-routing.md) | 4-plane voice architecture with LiveKit SFU |
| [Voice Integration](features/voice-integration.md) | STT/TTS pipeline configuration |
| [Nostr Auth](features/nostr-auth.md) | NIP-07/NIP-98 browser extension authentication |
| [Auth & User Settings](features/auth-user-settings.md) | User settings and session management |
| [Ontology Parser](features/ontology-parser.md) | OWL 2 parsing configuration and Logseq Markdown conventions |
| [Hierarchy Integration](features/hierarchy-integration.md) | Class hierarchy tree visualisation |
| [Local File Sync](features/local-file-sync-strategy.md) | GitHub-to-local file synchronisation strategy |
| [ComfyUI Setup](comfyui-sam3d-setup.md) | ComfyUI SAM3D integration setup |

---

## Operations & Integration

| Guide | Description |
|-------|-------------|
| [Configuration](operations/configuration.md) | Environment variables, runtime settings, and YAML config |
| [Troubleshooting](operations/troubleshooting.md) | Common errors, diagnostic commands, and known issues |
| [Security](operations/security.md) | Authentication hardening, secrets management, and SSRF mitigations |
| [Telemetry & Logging](operations/telemetry-logging.md) | Structured logging, metrics, and observability setup |
| [Pipeline Admin API](operations/pipeline-admin-api.md) | Admin REST endpoints for pipeline lifecycle management |
| [Operator Runbook](operations/pipeline-operator-runbook.md) | Production operations playbook for on-call engineers |
| [Maintenance](operations/maintenance.md) | Routine maintenance tasks, backups, and Neo4j housekeeping |
| [Neo4j Integration](integration/neo4j-integration.md) | Neo4j connection, Cypher conventions, and migration |
| [Solid Integration](integration/solid-integration.md) | Solid Pod integration overview and LDP operations |
| [Solid Pod Creation](integration/solid-pod-creation.md) | Creating and managing per-user Solid Pods |
| [ComfyUI Service](integration/comfyui-service-integration.md) | ComfyUI Docker service integration and API bridge |
