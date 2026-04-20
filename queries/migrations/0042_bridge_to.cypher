// queries/migrations/0042_bridge_to.cypher
// =============================================================================
// ADR-048 Dual-Tier Identity Model — BRIDGE_TO migration
// =============================================================================
//
// Introduces the separation between narrative (:KGNode) and vocabulary
// (:OntologyClass) populations. A directed BRIDGE_TO edge carries promotion
// state between them; a transient BRIDGE_CANDIDATE edge surfaces the
// migration broker queue (ADR-049, ADR-051).
//
// Canonical IRI scheme for both labels: `vc:{domain}/{slug}`.
//
// Apply (idempotent):
//   cypher-shell -u neo4j -p $NEO4J_PASSWORD < queries/migrations/0042_bridge_to.cypher
//
// Roll-forward only. Use 0043_bridge_to_revoke.cypher for reversion.
// =============================================================================

// -- STEP 1: Uniqueness constraints on canonical IRI ---------------------------

CREATE CONSTRAINT kg_node_iri_unique IF NOT EXISTS
FOR (k:KGNode) REQUIRE k.iri IS UNIQUE;

CREATE CONSTRAINT ontology_class_iri_unique IF NOT EXISTS
FOR (o:OntologyClass) REQUIRE o.iri IS UNIQUE;

// Secondary canonical_iri constraint (alias used by older ingest paths).
CREATE CONSTRAINT kg_node_canonical_iri_unique IF NOT EXISTS
FOR (k:KGNode) REQUIRE k.canonical_iri IS UNIQUE;

CREATE CONSTRAINT ontology_class_canonical_iri_unique IF NOT EXISTS
FOR (o:OntologyClass) REQUIRE o.canonical_iri IS UNIQUE;

// -- STEP 2: BRIDGE_TO edge indexes (promoted/colocated/revoked) --------------

CREATE INDEX bridge_to_kind_idx IF NOT EXISTS
FOR ()-[r:BRIDGE_TO]-() ON (r.kind);

CREATE INDEX bridge_to_confidence_idx IF NOT EXISTS
FOR ()-[r:BRIDGE_TO]-() ON (r.confidence);

CREATE INDEX bridge_to_promoted_at_idx IF NOT EXISTS
FOR ()-[r:BRIDGE_TO]-() ON (r.promoted_at);

CREATE INDEX bridge_to_created_at_idx IF NOT EXISTS
FOR ()-[r:BRIDGE_TO]-() ON (r.created_at);

// -- STEP 3: BRIDGE_CANDIDATE edge indexes (pre-promotion queue) --------------

CREATE INDEX bridge_candidate_status_idx IF NOT EXISTS
FOR ()-[r:BRIDGE_CANDIDATE]-() ON (r.status);

CREATE INDEX bridge_candidate_confidence_idx IF NOT EXISTS
FOR ()-[r:BRIDGE_CANDIDATE]-() ON (r.confidence);

CREATE INDEX bridge_candidate_last_updated_idx IF NOT EXISTS
FOR ()-[r:BRIDGE_CANDIDATE]-() ON (r.last_updated_at);

// -- STEP 4: Node-type indexes for fast population queries --------------------

CREATE INDEX kg_node_type_idx IF NOT EXISTS
FOR (k:KGNode) ON (k.node_type);

CREATE INDEX ontology_class_owl_kind_idx IF NOT EXISTS
FOR (o:OntologyClass) ON (o.owl_kind);

CREATE INDEX kg_node_visibility_idx IF NOT EXISTS
FOR (k:KGNode) ON (k.visibility);

// -- STEP 5: Colocation backfill ---------------------------------------------
// For every (KGNode, OntologyClass) pair sharing the same canonical IRI,
// emit a system-owned BRIDGE_TO{kind:'colocated'} edge. Safe to run repeatedly.
MATCH (k:KGNode), (o:OntologyClass)
WHERE k.iri = o.iri AND k.iri IS NOT NULL
MERGE (k)-[b:BRIDGE_TO]->(o)
  ON CREATE SET
    b.kind = 'colocated',
    b.confidence = 1.0,
    b.created_at = datetime(),
    b.created_by = 'system',
    b.promoted_at = datetime()
  ON MATCH SET
    b.kind = coalesce(b.kind, 'colocated');

// -- STEP 6: Migration marker ------------------------------------------------

MERGE (m:SchemaMigration {id: '0042_bridge_to'})
  ON CREATE SET
    m.applied_at = datetime(),
    m.adr = 'ADR-048',
    m.description = 'Dual-tier identity: KGNode / OntologyClass + BRIDGE_TO';
