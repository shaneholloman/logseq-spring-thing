// queries/migrations/0044_ontology_tier_layout.cypher
// =============================================================================
// ADR-048 — Spatial + mass separation for the ontology tier
// =============================================================================
//
// Places :OntologyClass nodes on a Fibonacci-spiral sphere at radius ~600 so they
// form a shell well outside the KG's ~100-unit working volume. Pumps mass to
// 10× so the ontology tier is effectively static while KG physics settles.
//
// Rationale: when the KG and ontology tiers share one physics pipeline
// (no per-tier force parameters yet), spatial + mass separation keeps each
// tier stable. Reversible via `MATCH (o:OntologyClass) REMOVE o.sim_x, o.x, ...`.
//
// Apply (idempotent):
//   cypher-shell -u neo4j -p $NEO4J_PASSWORD -d neo4j < queries/migrations/0044_ontology_tier_layout.cypher
// =============================================================================

// -- STEP 1: Fibonacci-sphere placement for :OntologyClass -------------------------
// Uses node rank (ordered by id) to pick an index into the 2811-point sphere.
// Radius 600 keeps them far from KG's workspace (~[-100, 100]).
MATCH (o:OntologyClass)
WITH collect(o) AS classes, 600.0 AS radius, 3.14159265358979 AS pi
WITH classes, radius, pi, size(classes) AS n,
     (3.14159265358979 - sqrt(5.0)) AS ga  // golden angle in radians
UNWIND range(0, size(classes) - 1) AS idx
WITH classes[idx] AS o, radius, pi, ga, idx, size(classes) AS n
WITH o,
     radius,
     acos(1.0 - 2.0 * toFloat(idx + 1) / (toFloat(n) + 1.0)) AS theta,
     (ga * toFloat(idx)) AS phi
SET o.x     = radius * sin(theta) * cos(phi),
    o.y     = radius * sin(theta) * sin(phi),
    o.z     = radius * cos(theta),
    o.sim_x = radius * sin(theta) * cos(phi),
    o.sim_y = radius * sin(theta) * sin(phi),
    o.sim_z = radius * cos(theta),
    o.vx    = 0.0,
    o.vy    = 0.0,
    o.vz    = 0.0
RETURN count(o) AS owl_positions_reseated;

// -- STEP 2: Heavy mass + tier group marker ----------------------------------
MATCH (o:OntologyClass)
SET o.mass       = 10.0,
    o.size       = COALESCE(o.size, 1.4),
    o.color      = COALESCE(o.color, '#9B59B6'),
    o.group_name = 'ontology'
RETURN count(o) AS owl_mass_set;

// -- STEP 3: Ensure KGNode group_name ------------------------------------------
MATCH (k:KGNode)
WHERE k.group_name IS NULL
SET k.group_name = 'knowledge'
RETURN count(k) AS kg_group_set;

// -- STEP 4: Migration marker --------------------------------------------------
MERGE (m:SchemaMigration {id: '0044_ontology_tier_layout'})
  ON CREATE SET
    m.applied_at = datetime(),
    m.adr = 'ADR-048',
    m.description = 'Fibonacci-sphere OwlClass placement at r=600, mass=10, group=ontology.';
