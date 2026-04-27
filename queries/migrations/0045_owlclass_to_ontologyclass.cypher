// queries/migrations/0045_owlclass_to_ontologyclass.cypher
// =============================================================================
// ADR-048 — Reconcile ontology-tier label: :OwlClass → :OntologyClass
// =============================================================================
//
// The historical corpus carried `:OwlClass`. ADR-048 names the ontology label
// `:OntologyClass`, and `bridge_edge.rs` (lines 281, 312, 364, 400, 493) plus
// migration 0042 constraints/indexes already reference `:OntologyClass`.
// While the writer wrote `:OwlClass`, every BRIDGE_TO MERGE in `bridge_edge.rs`
// silently created an `:OntologyClass`-only orphan stub (only an `iri`
// property) instead of attaching to the real `:OwlClass` row.
//
// The matching code-side rename ships in `neo4j_ontology_repository.rs`,
// `neo4j_adapter.rs` (incl. an idempotent label-add at startup), and tests.
// This migration is the one-shot cleanup ops should run AGAINST EXISTING
// PRODUCTION DATA before deploying the new code (or whenever the startup
// `OntologyClass migration: label-add failed` warning surfaces).
//
// Rollback: 0046_revert_owlclass_label.cypher (TODO when needed).
//
// Apply (idempotent — safe to re-run):
//   cypher-shell -u neo4j -p $NEO4J_PASSWORD -d neo4j \
//       < queries/migrations/0045_owlclass_to_ontologyclass.cypher
// =============================================================================

// -- STEP 1: Redirect BRIDGE_TO edges from orphan :OntologyClass stubs --------
// to the real :OwlClass row that shares the same iri. The stub was created
// by older bridge_edge.rs MERGE-ing (o:OntologyClass {iri: $iri}) against a
// label that no real ontology row carried. By copying the orphan's edges to
// the real row keyed on iri, we preserve all promotion/colocation history
// before deleting the stub.
MATCH (k:KGNode)-[r:BRIDGE_TO]->(stub:OntologyClass)
WHERE NOT stub:OwlClass
MATCH (real:OwlClass {iri: stub.iri})
MERGE (k)-[r2:BRIDGE_TO]->(real)
  ON CREATE SET r2 = properties(r)
  ON MATCH SET r2.kind = coalesce(r2.kind, r.kind)
RETURN count(r2) AS edges_redirected;

// -- STEP 2: Delete orphan :OntologyClass stubs ------------------------------
// These have no :OwlClass label and (after STEP 1) no incoming BRIDGE_TO.
// DETACH DELETE catches any straggler relationships.
MATCH (stub:OntologyClass)
WHERE NOT stub:OwlClass
DETACH DELETE stub
RETURN count(*) AS orphan_stubs_removed;

// -- STEP 3: Add :OntologyClass label to every :OwlClass that lacks it -------
MATCH (c:OwlClass) WHERE NOT c:OntologyClass
SET c:OntologyClass
RETURN count(c) AS owlclass_labels_added;

// -- STEP 4 (deferred — run only after a soak window) ------------------------
// Strip :OwlClass from nodes that now also carry :OntologyClass. Comment out
// during the soak; uncomment when ready to fully retire the legacy label.
// MATCH (c:OntologyClass) WHERE c:OwlClass
// REMOVE c:OwlClass
// RETURN count(c) AS owlclass_labels_stripped;

// -- STEP 5: Migration marker -------------------------------------------------
MERGE (m:SchemaMigration {id: '0045_owlclass_to_ontologyclass'})
  ON CREATE SET
    m.applied_at = datetime(),
    m.adr = 'ADR-048',
    m.description = 'OwlClass→OntologyClass label add + bridge_edge orphan-stub redirect/cleanup.';
