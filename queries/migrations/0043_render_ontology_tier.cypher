// queries/migrations/0043_render_ontology_tier.cypher
// =============================================================================
// ADR-048 — Render the ontology tier in the force-directed graph
// =============================================================================
//
// Completes the ADR-048 rollout for the visualiser:
//
//   1. Seeds render-time properties (x/y/z, size, color, mass, metadata_id)
//      on :OwlClass nodes so they can participate in the shared GPU physics
//      pipeline alongside :KGNode.
//   2. Creates idempotent BRIDGE_TO{kind:'colocated'} edges between (KGNode,
//      OwlClass) pairs that share an IRI (strict) or a case-insensitive label
//      (fallback).
//   3. Ensures KGNode nodes have an `iri` property aliased from
//      `owl_class_iri` where missing, so future BRIDGE_TO backfill by IRI
//      works without label-match fallback.
//
// Neo4j label note: ADR-048 spells the ontology label `OntologyClass`. The
// corpus currently carries `:OwlClass`. A rename would touch 15+ Rust files
// and is out of scope for this migration; the loader in
// `neo4j_adapter.rs` treats `:OwlClass` as the ontology tier.
//
// Apply (idempotent):
//   cypher-shell -u neo4j -p $NEO4J_PASSWORD -d neo4j < queries/migrations/0043_render_ontology_tier.cypher
// =============================================================================

// -- STEP 1: Seed render properties on :OwlClass ------------------------------
// A deterministic pseudo-random placement based on id keeps the layout stable
// between runs; the physics loop will relax from there.
MATCH (o:OwlClass)
WHERE o.x IS NULL OR o.y IS NULL OR o.z IS NULL
WITH o,
     toFloat(((o.id * 73856093) % 2000) - 1000) / 10.0 AS px,
     toFloat(((o.id * 19349663) % 2000) - 1000) / 10.0 AS py,
     toFloat(((o.id * 83492791) % 2000) - 1000) / 10.0 AS pz
SET o.x = px, o.y = py, o.z = pz,
    o.vx = 0.0, o.vy = 0.0, o.vz = 0.0,
    o.sim_x = px, o.sim_y = py, o.sim_z = pz
RETURN count(o) AS owlclass_positions_seeded;

MATCH (o:OwlClass)
WHERE o.mass IS NULL OR o.size IS NULL OR o.color IS NULL OR o.weight IS NULL
SET o.mass = coalesce(o.mass, 1.0),
    o.size = coalesce(o.size, 1.2),
    o.color = coalesce(o.color, '#9B59B6'),
    o.weight = coalesce(o.weight, 1.0)
RETURN count(o) AS owlclass_render_props_seeded;

// OwlClass render identity: metadata_id (falls back to iri), label (falls back
// to preferred_term or the local name from the IRI).
MATCH (o:OwlClass)
WHERE o.metadata_id IS NULL OR o.label IS NULL OR o.label = ''
SET o.metadata_id = coalesce(o.metadata_id, o.iri, toString(o.id)),
    o.label = coalesce(o.label, o.preferred_term,
              CASE WHEN o.iri CONTAINS ':'
                   THEN substring(o.iri, size(split(o.iri, ':')[0]) + 1)
                   ELSE o.iri END,
              toString(o.id))
RETURN count(o) AS owlclass_identity_seeded;

// OwlClass node_type + owl_class_iri (for loader-side uniform handling).
MATCH (o:OwlClass)
WHERE o.node_type IS NULL OR o.owl_class_iri IS NULL
SET o.node_type = coalesce(o.node_type, 'owl_class'),
    o.owl_class_iri = coalesce(o.owl_class_iri, o.iri)
RETURN count(o) AS owlclass_types_seeded;

// -- STEP 2: Ensure KGNode.iri is populated from owl_class_iri ---------------
// Needed for the IRI-strict BRIDGE_TO match in STEP 3.
MATCH (k:KGNode)
WHERE k.iri IS NULL AND k.owl_class_iri IS NOT NULL
SET k.iri = k.owl_class_iri
RETURN count(k) AS kgnode_iris_backfilled;

// -- STEP 3a: BRIDGE_TO colocated edges — IRI-strict (preferred) -------------
MATCH (k:KGNode), (o:OwlClass)
WHERE k.iri IS NOT NULL AND o.iri IS NOT NULL AND k.iri = o.iri
MERGE (k)-[b:BRIDGE_TO]->(o)
  ON CREATE SET
    b.kind = 'colocated',
    b.confidence = 1.0,
    b.created_at = datetime(),
    b.created_by = 'system',
    b.match_method = 'iri',
    b.edge_type = 'bridge'
  ON MATCH SET
    b.kind = coalesce(b.kind, 'colocated'),
    b.edge_type = coalesce(b.edge_type, 'bridge')
RETURN count(b) AS iri_bridges;

// -- STEP 3b: BRIDGE_TO candidate edges — label fallback ---------------------
// Case-insensitive label match; weaker confidence, flagged for broker review.
MATCH (k:KGNode), (o:OwlClass)
WHERE k.label IS NOT NULL AND o.label IS NOT NULL
  AND toLower(trim(k.label)) = toLower(trim(o.label))
  AND NOT EXISTS { MATCH (k)-[:BRIDGE_TO]->(o) }
MERGE (k)-[b:BRIDGE_TO]->(o)
  ON CREATE SET
    b.kind = 'candidate',
    b.confidence = 0.7,
    b.created_at = datetime(),
    b.created_by = 'system',
    b.match_method = 'label',
    b.edge_type = 'bridge'
RETURN count(b) AS label_bridges;

// -- STEP 4: Migration marker ------------------------------------------------
MERGE (m:SchemaMigration {id: '0043_render_ontology_tier'})
  ON CREATE SET
    m.applied_at = datetime(),
    m.adr = 'ADR-048',
    m.description = 'Seed render props on :OwlClass; backfill BRIDGE_TO colocated + candidate edges.';
