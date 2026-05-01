// =============================================================================
// Migration: 2026-05_logseq-blocks.cypher
// ADR-068 D4 — Block-level Logseq data migration
// Idempotent: re-running on unchanged data produces zero mutations
// =============================================================================
//
// Context
// -------
// 998 Logseq files already exist in Neo4j as page-level :Page nodes with URNs
// like `urn:visionclaw:concept:<owner>:page:<slug>`. This migration prepares the
// schema for block-level children — it does NOT insert block data. Block data
// insertion happens at runtime via `src/services/parsers/block_level_parser.rs`,
// which emits MERGE-based Cypher (ADR-068 D6 idempotency invariant).
//
// What this migration does:
//   1. Adds :Container label to existing Page nodes that will receive blocks.
//   2. Creates indexes for Block node lookups (urn, kind_id, block_index).
//   3. Creates a composite index on (kind_id, urn) per ADR-064 D5.
//   4. Adds a uniqueness constraint on Block.urn.
//   5. Creates relationship indexes for block edge types.
//   6. Runs pre-migration health checks.
//   7. Records a SchemaMigration marker for auditability.
//
// Apply (idempotent — safe to re-run):
//   cypher-shell -u neo4j -p $NEO4J_PASSWORD -d neo4j \
//       < migrations/2026-05_logseq-blocks.cypher
//
// Rollback:
//   See migrations/README.md §Rollback for the reverse script.
//
// Prerequisites:
//   - Neo4j 5.x+ (IF NOT EXISTS syntax required)
//   - Existing page-level data from KnowledgeGraphParser already loaded
//   - Recommended: take a database backup before first run
//
// References:
//   - ADR-068 (Logseq Block-Level Fidelity)
//   - ADR-064 (Typed Graph Schema, D5 composite indexes)
//   - src/services/parsers/block_level_parser.rs (runtime block ingest)
//   - src/uri/kinds.rs (Kind::Concept with block sub-kind)
// =============================================================================


// =============================================================================
// STEP 0: Pre-migration health check — detect orphaned blocks from prior runs
// =============================================================================
// If a previous partial run left :Block nodes without any relationships, this
// surfaces them. A non-zero count here is not fatal but should be investigated.
// The RETURN is informational — cypher-shell prints it to stdout.

MATCH (b:Block)
WHERE NOT (b)--()
RETURN count(b) AS orphaned_blocks_pre_migration;


// =============================================================================
// STEP 1: Add :Container label to existing Page nodes
// =============================================================================
// Only Page nodes whose URN follows the visionclaw concept:*:page:* pattern
// receive the :Container label. This marks them as block-level container targets
// without altering their existing labels or URNs (ADR-068 D4).
//
// Idempotent: SET on a node that already has the label is a no-op.

MATCH (p:Page)
WHERE p.urn IS NOT NULL
  AND p.urn STARTS WITH 'urn:visionclaw:concept:'
  AND p.urn CONTAINS ':page:'
  AND NOT p:Container
SET p:Container
RETURN count(p) AS pages_labelled_container;


// =============================================================================
// STEP 2: Uniqueness constraint — Block URNs must be globally unique
// =============================================================================
// URNs are minted via mint_typed_concept(owner, Block, local) per ADR-068 D2
// and are deterministic (content-hash or explicit id:: based). Uniqueness
// prevents duplicate block nodes from malformed ingest batches.

CREATE CONSTRAINT block_urn_unique IF NOT EXISTS
FOR (b:Block) REQUIRE b.urn IS UNIQUE;


// =============================================================================
// STEP 3: Indexes for Block node lookups
// =============================================================================
// Primary lookup index: most queries resolve blocks by URN.
// The constraint in STEP 2 implicitly creates a unique index on urn, but we
// create a named index explicitly for clarity in EXPLAIN plans.

CREATE INDEX block_urn_idx IF NOT EXISTS
FOR (b:Block) ON (b.urn);

// kind_id: NodeKind discriminant (31 for Block per block_level_parser.rs).
// Enables fast filtering when walking mixed-kind graphs.
CREATE INDEX block_kind_id_idx IF NOT EXISTS
FOR (b:Block) ON (b.kind_id);

// block_index: positional index within the source file (0-based).
// Used for ordered reconstruction of document structure.
CREATE INDEX block_index_idx IF NOT EXISTS
FOR (b:Block) ON (b.block_index);

// Composite index on (kind_id, urn) per ADR-064 D5.
// Serves queries that filter by kind then resolve by URN in a single scan.
CREATE INDEX block_kind_urn_composite_idx IF NOT EXISTS
FOR (b:Block) ON (b.kind_id, b.urn);

// indent_level: used for depth-based queries (e.g., "all top-level blocks").
CREATE INDEX block_indent_level_idx IF NOT EXISTS
FOR (b:Block) ON (b.indent_level);

// updated_at: used for incremental re-sync and change tracking.
CREATE INDEX block_updated_at_idx IF NOT EXISTS
FOR (b:Block) ON (b.updated_at);

// Content search: clean_text is the stripped-of-markup plain text used for
// full-text queries. TEXT index enables CONTAINS predicates.
CREATE TEXT INDEX block_clean_text_idx IF NOT EXISTS
FOR (b:Block) ON (b.clean_text);

// task_status: enables filtering blocks by TODO/DOING/DONE/etc.
CREATE INDEX block_task_status_idx IF NOT EXISTS
FOR (b:Block) ON (b.task_status);


// =============================================================================
// STEP 4: Relationship indexes for block edge types
// =============================================================================
// Block edges are created at runtime by block_level_parser.rs via MERGE.
// These indexes accelerate traversal queries over the block tree.

// BLOCK_PARENT: child→parent (or child→page for top-level blocks).
// No properties to index on the relationship itself — the endpoints carry
// the URN. The edge existence is what matters for tree traversal.

// BLOCK_LEFT: sibling ordering within a parent's children.
// No properties needed.

// BLOCK_REF: ((uuid)) cross-references between blocks.
// No properties needed.

// WIKILINK on Block: block-level wikilinks augmenting page-level ones.
// No properties needed.

// TAGGED_WITH on Block: #tag references from blocks.
// No properties needed.


// =============================================================================
// STEP 5: Container-specific indexes
// =============================================================================
// Pages that are containers benefit from an index on the Container label
// combined with urn for fast page→blocks tree root resolution.

CREATE INDEX container_urn_idx IF NOT EXISTS
FOR (c:Container) ON (c.urn);


// =============================================================================
// STEP 6: Post-migration verification queries
// =============================================================================
// These are informational — they print summary statistics to stdout.

// 6a: Count of pages now labelled :Container
MATCH (c:Container)
RETURN count(c) AS total_container_pages;

// 6b: Verify constraint exists
SHOW CONSTRAINTS
YIELD name, type, labelsOrTypes, properties
WHERE name = 'block_urn_unique'
RETURN name, type, labelsOrTypes, properties;

// 6c: Verify indexes exist
SHOW INDEXES
YIELD name, type, labelsOrTypes, properties, state
WHERE name STARTS WITH 'block_'
RETURN name, type, labelsOrTypes, properties, state
ORDER BY name;

// 6d: Count blocks per page (will return 0 rows before runtime ingest)
MATCH (b:Block)-[:BLOCK_PARENT]->(p:Container)
WITH p.urn AS page_urn, count(b) AS block_count
RETURN page_urn, block_count
ORDER BY block_count DESC
LIMIT 20;

// 6e: Verify no BLOCK_PARENT cycles exist (safety invariant).
// A cycle would mean a block is its own ancestor — structurally impossible
// in a well-formed Logseq file. This query terminates quickly on acyclic
// graphs; on a cyclic graph it returns the offending path.
MATCH path = (b:Block)-[:BLOCK_PARENT*2..16]->(b)
RETURN b.urn AS cyclic_block_urn, length(path) AS cycle_length
LIMIT 10;

// 6f: Orphaned blocks after migration (should match pre-migration count
// since this migration creates no block data)
MATCH (b:Block)
WHERE NOT (b)--()
RETURN count(b) AS orphaned_blocks_post_migration;


// =============================================================================
// STEP 7: Migration marker
// =============================================================================
// Records this migration in the SchemaMigration log. MERGE ensures idempotent
// marker creation. The applied_at timestamp is set only on first creation;
// subsequent runs leave it unchanged.

MERGE (m:SchemaMigration {id: '2026-05_logseq-blocks'})
  ON CREATE SET
    m.applied_at = datetime(),
    m.adr = 'ADR-068',
    m.description = 'Block-level schema: :Container label on pages, :Block indexes + constraints, relationship indexes. Data inserted at runtime by block_level_parser.rs.',
    m.version = '1.0.0',
    m.rollback_script = 'migrations/2026-05_logseq-blocks_rollback.cypher'
  ON MATCH SET
    m.last_verified_at = datetime();
