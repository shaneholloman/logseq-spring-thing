# PRD: Bead Provenance System Radical Upgrade

**Status**: Draft
**Priority**: P1 — Zero test coverage, fire-and-forget with no failure handling, no lifecycle management
**Affects**: `nostr_bead_publisher.rs`, `nostr_bridge.rs`, `briefing_handler.rs`, `main.rs`, Neo4j schema
**Inspired By**: [jedarden/NEEDLE](https://github.com/jedarden/NEEDLE) — deterministic task orchestrator with exhaustive outcome FSM
**ADR**: ADR-034

---

## 1. Problem Statement

The bead provenance system is the audit backbone of VisionClaw — every brief→debrief cycle emits a cryptographic Nostr event persisted to Neo4j. Despite this critical role, the implementation has significant gaps:

| # | Finding | Impact |
|---|---------|--------|
| 1 | Fire-and-forget publishing — relay failures are logged but not surfaced or retried | Provenance records silently lost on network issues |
| 2 | Zero test coverage — no unit or integration tests for any bead code | Regressions undetectable; refactoring high-risk |
| 3 | No bead lifecycle — beads are created once with no state tracking | Cannot query bead status, cannot detect stale/failed beads |
| 4 | Single-key signing — one hardcoded keypair, no rotation | Key compromise requires full redeployment |
| 5 | Hardcoded 5-second relay timeout — no retry, no backoff | Transient network issues cause permanent provenance loss |
| 6 | No bead archival — beads accumulate indefinitely in Neo4j | Unbounded storage growth, no lifecycle management |
| 7 | Bridge reconnection is fixed 30s — no exponential backoff | Relay outages cause reconnection storms |
| 8 | No health monitoring — relay liveness unknown until publish fails | No early warning of infrastructure issues |
| 9 | No outcome classification — success and all failure modes treated identically | Cannot distinguish transient from permanent failures |
| 10 | No learning capture — agent decisions that produce beads leave no structured trace | Audit trail lacks reasoning provenance |

These compound: without tests, upgrading is risky. Without lifecycle tracking, failures are invisible. Without outcome classification, recovery is impossible.

---

## 2. Goals

| Goal | Measurable Target |
|------|-------------------|
| Exhaustive outcome classification | Every publish attempt produces a typed `BeadOutcome` — no silent failures |
| Bead lifecycle FSM | Beads traverse `Created → Publishing → Published → Bridged → Archived` with explicit error states |
| Retry with exponential backoff | Configurable retry (default 3 attempts, 1s/2s/4s backoff) before marking permanent failure |
| Full test coverage | >= 80% line coverage across all bead modules; CI gate enforced |
| Health monitoring | Relay liveness check every 60s; structured health status queryable via `/api/health/beads` |
| Learning capture | Post-bead structured retrospective stored in Neo4j as `(:BeadLearning)` nodes |
| Bead archival policy | Beads older than configurable TTL (default 90 days) transition to `Archived` with optional Neo4j cleanup |
| Key rotation support | `BeadKeyring` supporting multiple signing keys with graceful rotation |
| Bridge backoff | Exponential backoff (30s → 60s → 120s → 300s cap) on bridge reconnection |
| BeadStore trait | Abstract storage interface (inspired by NEEDLE) enabling future backend swaps |

---

## 3. Non-Goals

- Replacing the Nostr relay infrastructure (JSS + forum relay remain as-is).
- Implementing NEEDLE's full task orchestration (beads remain provenance records, not work units).
- Migrating from Neo4j to another graph database.
- Adding a REST API for bead CRUD (beads are system-created, never user-created).
- Implementing NEEDLE's mitosis (bead splitting) in this phase — future consideration.
- Budget management for agent costs (separate concern from provenance).

---

## 4. User Stories

### Platform Operator

- As a platform operator, I can query bead health status via `/api/health/beads` so I know if provenance is functioning before issues are reported.
- As a platform operator, I can see structured bead outcomes (Success, RetryExhausted, RelayUnreachable, SigningFailed, Neo4jWriteFailed) in logs and Neo4j, so I can diagnose failures without reading raw relay logs.
- As a platform operator, I can configure retry policy and archival TTL via environment variables without code changes.

### Auditor

- As an auditor, I can query the full lifecycle of any bead (`Created → Publishing → Published → Bridged`) via Neo4j, so I can verify provenance completeness.
- As an auditor, I can see structured learning entries attached to beads, so I know what reasoning led to each agent decision.
- As an auditor, I can verify that archived beads retain their cryptographic signatures even after Neo4j cleanup.

### Developer

- As a developer, I can run `cargo test` and get comprehensive bead system tests — unit tests for types, publisher, bridge, lifecycle, and integration tests for the full flow.
- As a developer, I can implement a new `BeadStore` backend by implementing a trait, without modifying the publisher or bridge.

---

## 5. Technical Requirements

### 5.1 Bead Types (`bead_types.rs`)

```rust
/// Exhaustive bead lifecycle states — inspired by NEEDLE's 12-state worker FSM.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BeadState {
    Created,           // Bead constructed, not yet published
    Publishing,        // Publish attempt in progress
    Published,         // Relay accepted the event
    Neo4jPersisted,    // Neo4j write confirmed
    Bridged,           // Forum relay forwarding confirmed
    Archived,          // Past TTL, marked for cleanup
    Failed(BeadFailure), // Terminal failure with classified cause
}

/// Exhaustive outcome classification — every publish attempt gets one.
/// Inspired by NEEDLE's deterministic outcome handling.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BeadOutcome {
    Success,                          // Relay accepted, Neo4j written
    RelayTimeout { attempts: u8 },    // All retry attempts timed out
    RelayRejected { reason: String }, // Relay returned OK=false
    RelayUnreachable { error: String }, // WebSocket connection failed
    SigningFailed { error: String },   // Nostr event signing failed
    Neo4jWriteFailed { error: String },// Graph write failed (bead still on relay)
    BridgeFailed { error: String },    // Forum relay forwarding failed
}

/// Classified failure causes — no wildcard, every variant has a handler.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BeadFailure {
    Transient(String),  // Retryable — network timeout, temporary relay issue
    Permanent(String),  // Non-retryable — bad key, relay rejection, schema error
}

/// Extended bead metadata for lifecycle tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeadMetadata {
    pub bead_id: String,
    pub brief_id: String,
    pub debrief_path: String,
    pub user_pubkey: Option<String>,
    pub state: BeadState,
    pub outcome: Option<BeadOutcome>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub published_at: Option<chrono::DateTime<chrono::Utc>>,
    pub bridged_at: Option<chrono::DateTime<chrono::Utc>>,
    pub archived_at: Option<chrono::DateTime<chrono::Utc>>,
    pub retry_count: u8,
    pub nostr_event_id: Option<String>,
}

/// Post-bead learning entry — inspired by NEEDLE's structured retrospectives.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeadLearning {
    pub bead_id: String,
    pub what_worked: Option<String>,
    pub what_failed: Option<String>,
    pub reusable_pattern: Option<String>,
    pub confidence: f32,
    pub recorded_at: chrono::DateTime<chrono::Utc>,
}
```

### 5.2 BeadStore Trait (`bead_store.rs`)

Inspired by NEEDLE's `BeadStore` async trait with 15 methods:

```rust
#[async_trait]
pub trait BeadStore: Send + Sync {
    // Lifecycle
    async fn create(&self, metadata: &BeadMetadata) -> Result<(), BeadStoreError>;
    async fn update_state(&self, bead_id: &str, state: BeadState) -> Result<(), BeadStoreError>;
    async fn update_outcome(&self, bead_id: &str, outcome: BeadOutcome) -> Result<(), BeadStoreError>;

    // Query
    async fn get(&self, bead_id: &str) -> Result<Option<BeadMetadata>, BeadStoreError>;
    async fn list_by_state(&self, state: BeadState) -> Result<Vec<BeadMetadata>, BeadStoreError>;
    async fn list_failed(&self) -> Result<Vec<BeadMetadata>, BeadStoreError>;
    async fn count_by_state(&self) -> Result<std::collections::HashMap<String, u64>, BeadStoreError>;

    // Learning
    async fn store_learning(&self, learning: &BeadLearning) -> Result<(), BeadStoreError>;
    async fn get_learnings(&self, bead_id: &str) -> Result<Vec<BeadLearning>, BeadStoreError>;

    // Maintenance
    async fn archive_before(&self, cutoff: chrono::DateTime<chrono::Utc>) -> Result<u64, BeadStoreError>;
    async fn health_check(&self) -> Result<BeadHealthStatus, BeadStoreError>;
}
```

### 5.3 Retry Configuration

```rust
pub struct BeadRetryConfig {
    pub max_attempts: u8,         // Default: 3
    pub base_delay_ms: u64,       // Default: 1000
    pub max_delay_ms: u64,        // Default: 10000
    pub backoff_multiplier: f64,  // Default: 2.0
}
```

Configured via environment: `BEAD_RETRY_MAX_ATTEMPTS`, `BEAD_RETRY_BASE_DELAY_MS`, `BEAD_RETRY_BACKOFF_MULTIPLIER`.

### 5.4 Neo4j Schema Extensions

```cypher
// Extended :Bead node with lifecycle fields
MERGE (b:Bead {bead_id: $bead_id})
SET b.state = $state,
    b.outcome = $outcome,
    b.created_at = $created_at,
    b.published_at = $published_at,
    b.bridged_at = $bridged_at,
    b.retry_count = $retry_count

// New :BeadLearning node
CREATE (l:BeadLearning {
    bead_id: $bead_id,
    what_worked: $what_worked,
    what_failed: $what_failed,
    reusable_pattern: $reusable_pattern,
    confidence: $confidence,
    recorded_at: $recorded_at
})
WITH l
MATCH (b:Bead {bead_id: $bead_id})
MERGE (b)-[:HAS_LEARNING]->(l)

// Unique constraint
CREATE CONSTRAINT bead_learning_id IF NOT EXISTS
  FOR (l:BeadLearning) REQUIRE l.bead_id IS NOT NULL;
```

### 5.5 Health Endpoint

```
GET /api/health/beads
{
    "relay_connected": true,
    "last_publish_at": "2026-04-13T10:30:00Z",
    "last_publish_outcome": "Success",
    "beads_by_state": {
        "Published": 1247,
        "Bridged": 1189,
        "Failed": 3,
        "Archived": 456
    },
    "relay_latency_ms": 42,
    "bridge_connected": true,
    "neo4j_connected": true
}
```

---

## 6. Architecture

```
                         ┌─────────────────────────────────────────┐
                         │          BeadLifecycleOrchestrator      │
                         │  (coordinates publisher, bridge, store) │
                         └────────┬──────────┬──────────┬─────────┘
                                  │          │          │
                    ┌─────────────┘    ┌─────┘    ┌─────┘
                    ▼                  ▼          ▼
          ┌─────────────────┐  ┌───────────┐  ┌──────────────┐
          │ NostrBeadPublisher│ │ BeadStore  │  │  NostrBridge  │
          │ (retry + outcome)│ │ (Neo4j impl)│ │ (health + bo) │
          └────────┬────────┘  └─────┬──────┘  └──────┬───────┘
                   │                 │                 │
            ┌──────┘          ┌──────┘          ┌──────┘
            ▼                 ▼                 ▼
     ┌─────────────┐  ┌─────────────┐  ┌─────────────────┐
     │  JSS Relay   │  │   Neo4j     │  │  Forum Relay    │
     │ (kind 30001) │  │ :Bead nodes │  │  (kind 9 msgs)  │
     └──────────────┘  └─────────────┘  └─────────────────┘
```

The `BeadLifecycleOrchestrator` is the new coordination layer. It replaces the fire-and-forget `tokio::spawn` in `briefing_handler.rs` with a deterministic state machine that:

1. Creates bead metadata in store (state: `Created`)
2. Delegates to publisher with retry policy (state: `Publishing`)
3. On success, updates state to `Published`, records outcome
4. Bridge subscription updates to `Bridged` when forwarded
5. Archival worker periodically transitions old beads to `Archived`
6. Every failure path produces a typed `BeadOutcome`

---

## 7. Success Metrics

| Metric | Current | Target |
|--------|---------|--------|
| Test coverage (bead modules) | 0% | >= 80% |
| Silent provenance failures | Unknown (no tracking) | 0 (every failure classified) |
| Bead state queryable | No | Yes (via BeadStore + health endpoint) |
| Mean time to detect relay failure | Unknown | < 120s (health check interval) |
| Retry recovery rate | 0% (no retry) | >= 90% of transient failures recovered |
| Learning entries per bead | 0 | >= 1 for debriefs with agent reasoning |

---

## 8. Milestones

| Phase | Deliverable | Files |
|-------|------------|-------|
| 1 | Bead types + BeadStore trait + Neo4j impl | `bead_types.rs`, `bead_store.rs` |
| 2 | Publisher upgrade (retry, outcomes, keyring) | `nostr_bead_publisher.rs` |
| 3 | Bridge upgrade (backoff, health, telemetry) | `nostr_bridge.rs` |
| 4 | Lifecycle orchestrator + handler integration | `bead_lifecycle.rs`, `briefing_handler.rs` |
| 5 | Test suite (unit + integration) | `tests/bead_*.rs` |
| 6 | Documentation update | `docs/reference/neo4j-schema-unified.md`, `docs/reference/rest-api.md` |

---

## 9. NEEDLE Patterns Adopted

| NEEDLE Pattern | Adaptation |
|---------------|------------|
| 12-state worker FSM | 7-state bead lifecycle FSM (lighter, provenance-specific) |
| Exhaustive outcome classification | `BeadOutcome` enum with no wildcard arms |
| `BeadStore` async trait | Same pattern, Neo4j backend instead of SQLite |
| Structured retrospectives | `BeadLearning` with what_worked/what_failed/reusable_pattern |
| Configurable retry | `BeadRetryConfig` from environment |
| Health heartbeat | Relay liveness check on configurable interval |

### NEEDLE Patterns Deferred

| Pattern | Reason |
|---------|--------|
| Mitosis (bead splitting) | Provenance beads are immutable — splitting applies to task beads, not audit records |
| Strand waterfall escalation | Over-engineered for provenance; beads have simple linear lifecycle |
| Canary deployment | Agent binary management is out of scope for provenance |
| Budget management | Agent cost tracking belongs in orchestration layer, not provenance |
| NATO worker naming | Single publisher instance; parallelism not needed for provenance writes |
