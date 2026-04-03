# PRD: Solid Protocol Integration Upgrade

## Overview
Expand VisionFlow's existing JSS sidecar integration to leverage new Solid capabilities for data sovereignty, cross-device sync, and federated knowledge governance.

## Problem Statement
Users configure graph views (camera, filters, physics, clusters) that are lost on page reload. Settings persist in Neo4j (shared) or localStorage (device-bound). There is no mechanism to save, name, share, or discover graph views across devices or users. Ontology edits replace entire documents. Agent decisions are opaque.

## Success Metrics
- Users can save/load named graph views in < 2 seconds
- Views sync across devices via WebSocket in < 500ms
- Ontology edits use SPARQL PATCH (no full-document replacement)
- Agent memory auditable via Pod browser

## Phases

### Phase 1: Pod-backed Graph Views (Sprint 1)
**User Stories:**
- As a user, I can save my current graph view (camera, filters, cluster settings) to my Pod
- As a user, I can load a saved view from any device
- As a user, I can share a view with another user via WAC
- As a user, I can say "save view as AI Research" in the command input

**Acceptance Criteria:**
- JSON-LD graph view documents stored in `/settings/graph-views/`
- WebSocket notification triggers sync on other tabs/devices
- CommandInput accepts "save view [name]" and "load view [name]"
- WAC: owner has Read/Write/Control, shared users get Read

### Phase 2: SPARQL PATCH for Ontology (Sprint 2)
**User Stories:**
- As a power user, I can make surgical ontology edits without replacing the full document
- As a user, I get conflict detection when two users edit the same ontology

**Acceptance Criteria:**
- JssOntologyService uses PATCH with SPARQL Update for edits
- N3 Patch with `solid:where` for optimistic concurrency
- Conflict returns 409 with merge guidance

### Phase 3: Type Index Discovery (Sprint 3)
**User Stories:**
- As a user, I can discover shared views from other users
- As a user, I can discover available agent capabilities across instances

**Acceptance Criteria:**
- Public Type Index registers graph views and agent capabilities
- Discovery UI in Pod tab shows shared resources from connected users

### Phase 4: Agent Memory in Pods (Sprint 4)
**User Stories:**
- As a user, I can inspect what my AI agents have learned and decided
- As a user, I can revoke agent access to my data via WAC

**Acceptance Criteria:**
- Agent memory patterns synced to Pod as JSON-LD
- Pod browser shows agent memory containers
- WAC controls per-agent access

## Non-Goals
- Full ActivityPub federation (future)
- HTTP 402 micropayments (future)
- Git-backed ontology branching (future)

## Dependencies
- JSS sidecar running with notifications enabled
- Nostr NIP-98 authentication working
- Existing SolidPodService.ts and solid_proxy_handler.rs
