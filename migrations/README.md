# Neo4j Cypher Migrations

Schema and data migrations for the VisionClaw Neo4j graph database.

## Migration Index

Two migration directories exist in this repository:

| Directory | Database | Purpose |
|-----------|----------|---------|
| `queries/migrations/` | Neo4j | Ontology-tier schema (ADR-048) |
| `migrations/` | Neo4j | Block-level + typed schema (ADR-064, ADR-068) |

### All migrations in order

| ID | File | ADR | Status | Description |
|----|------|-----|--------|-------------|
| 0042 | `queries/migrations/0042_bridge_to.cypher` | ADR-048 | Applied | Dual-tier identity: KGNode / OntologyClass + BRIDGE_TO edges |
| 0043 | `queries/migrations/0043_render_ontology_tier.cypher` | ADR-048 | Applied | Seed render props on OntologyClass; backfill BRIDGE_TO edges |
| 0044 | `queries/migrations/0044_ontology_tier_layout.cypher` | ADR-048 | Applied | Fibonacci-sphere OntologyClass placement, mass separation |
| 0045 | `queries/migrations/0045_owlclass_to_ontologyclass.cypher` | ADR-048 | Applied | OwlClass to OntologyClass label reconciliation |
| 2026-05 | `migrations/2026-05_logseq-blocks.cypher` | ADR-068 | Pending | Block-level schema: Container label, Block indexes + constraints |

Planned (not yet created):

| ID | ADR | Description |
|----|-----|-------------|
| 2026-05_typed-schema | ADR-064 | Typed graph schema: kind property, dual-label coexistence |
| ADR-064a | ADR-064 | Cleanup: drop legacy-only labels after soak window |


## How to Run

### Prerequisites

- Neo4j 5.x or later (required for `IF NOT EXISTS` syntax on constraints/indexes)
- `cypher-shell` on PATH, or `neo4j-admin` for offline operations
- Database backup taken before first run of any migration

### Using cypher-shell (recommended)

```bash
# Set credentials
export NEO4J_PASSWORD='your-password'
export NEO4J_URI='bolt://localhost:7687'

# Run a specific migration
cypher-shell -u neo4j -p "$NEO4J_PASSWORD" -a "$NEO4J_URI" -d neo4j \
    < migrations/2026-05_logseq-blocks.cypher

# Run with verbose output (shows RETURN results)
cypher-shell -u neo4j -p "$NEO4J_PASSWORD" -a "$NEO4J_URI" -d neo4j \
    --format verbose \
    < migrations/2026-05_logseq-blocks.cypher
```

### Using neo4j-admin (offline, single-instance only)

```bash
# Stop Neo4j first
sudo systemctl stop neo4j

# Load via cypher-shell after restart, or use neo4j-admin database import
# for large bulk operations. For schema-only migrations like
# 2026-05_logseq-blocks.cypher, cypher-shell is preferred.

sudo systemctl start neo4j
```

### Using Docker

```bash
# If Neo4j runs in a container:
docker exec -i neo4j-container cypher-shell \
    -u neo4j -p "$NEO4J_PASSWORD" -d neo4j \
    < migrations/2026-05_logseq-blocks.cypher
```

### Verifying a migration was applied

Every migration writes a `:SchemaMigration` marker node. Query the log:

```cypher
MATCH (m:SchemaMigration)
RETURN m.id AS migration, m.applied_at AS applied, m.adr AS adr, m.description AS description
ORDER BY m.applied_at;
```


## Idempotency

All migrations in this project are idempotent. Re-running a migration on unchanged
data produces zero mutations. This is achieved through:

- `IF NOT EXISTS` on all `CREATE CONSTRAINT` and `CREATE INDEX` statements
- `MERGE` (not `CREATE`) for all node/edge mutations
- `ON CREATE SET` / `ON MATCH SET` for conditional property updates
- `WHERE NOT p:Label` guards before `SET p:Label` to avoid unnecessary writes

Idempotency means migrations are safe to include in startup scripts or CI pipelines
without risk of duplication or data corruption.


## Rollback Procedure

### 2026-05_logseq-blocks (ADR-068)

This migration only creates schema artifacts (labels, indexes, constraints) and a
marker node. No block data is inserted — that happens at runtime. Rollback removes
the schema artifacts:

```cypher
// =============================================================================
// Rollback: 2026-05_logseq-blocks
// Reverses the schema changes from the block-level migration.
// Safe to run even if the forward migration was never applied.
// =============================================================================

// Step 1: Drop indexes (order does not matter; IF EXISTS prevents errors)
DROP INDEX block_urn_idx IF EXISTS;
DROP INDEX block_kind_id_idx IF EXISTS;
DROP INDEX block_index_idx IF EXISTS;
DROP INDEX block_kind_urn_composite_idx IF EXISTS;
DROP INDEX block_indent_level_idx IF EXISTS;
DROP INDEX block_updated_at_idx IF EXISTS;
DROP INDEX block_clean_text_idx IF EXISTS;
DROP INDEX block_task_status_idx IF EXISTS;
DROP INDEX container_urn_idx IF EXISTS;

// Step 2: Drop uniqueness constraint
DROP CONSTRAINT block_urn_unique IF EXISTS;

// Step 3: Remove :Container label from pages
// This does NOT delete the page nodes — only strips the added label.
MATCH (p:Container)
REMOVE p:Container
RETURN count(p) AS container_labels_removed;

// Step 4: Delete any :Block nodes that were inserted at runtime
// WARNING: This removes all block data. Only run if you intend to fully
// revert to page-level-only operation.
// Uncomment the following lines to execute:
//
// MATCH (b:Block)
// DETACH DELETE b
// RETURN count(*) AS blocks_deleted;

// Step 5: Remove migration marker
MATCH (m:SchemaMigration {id: '2026-05_logseq-blocks'})
DELETE m
RETURN count(*) AS marker_removed;
```

Save the above as `migrations/2026-05_logseq-blocks_rollback.cypher` and run via
cypher-shell if rollback is needed.

### Older migrations (queries/migrations/004x)

Each migration file in `queries/migrations/` contains rollback notes in its header
comments. General pattern:

| Migration | Rollback |
|-----------|----------|
| 0042_bridge_to | Drop BRIDGE_TO indexes + constraints; delete BRIDGE_TO edges |
| 0043_render_ontology_tier | Remove seeded render properties from OntologyClass |
| 0044_ontology_tier_layout | `MATCH (o:OntologyClass) REMOVE o.sim_x, o.x, o.y, o.z, o.sim_y, o.sim_z` |
| 0045_owlclass_to_ontologyclass | Commented-out STEP 4 in the migration file; remove :OntologyClass label |


## Conventions

### Naming

- Ontology-tier migrations: `queries/migrations/NNNN_description.cypher` (4-digit sequential)
- Feature migrations: `migrations/YYYY-MM_description.cypher` (date-based)

### Structure

Every migration file follows this template:

```cypher
// =============================================================================
// Migration: <id>
// <ADR reference> — <one-line summary>
// Idempotent: re-running on unchanged data produces zero mutations
// =============================================================================

// STEP 0: Pre-migration health checks (informational RETURN)
// STEP 1..N: Schema changes (IF NOT EXISTS, MERGE, idempotent SET)
// STEP N+1: Post-migration verification queries (informational RETURN)
// STEP N+2: Migration marker (MERGE into :SchemaMigration)
```

### Review checklist

Before merging a new migration:

1. All CREATE/DROP statements use `IF NOT EXISTS` / `IF EXISTS`
2. All data mutations use MERGE, not CREATE
3. Pre-migration health check query is present
4. Post-migration verification query is present
5. SchemaMigration marker node is written
6. Rollback procedure is documented
7. The migration has been tested against a Neo4j 5.x instance
8. ADR reference is correct and the ADR status is "Implementing" or "Accepted"
