# ADR-054: URN-Solid + Solid-Apps Ecosystem Alignment

## Status

Ratified 2026-04-19

## Context

Two adjacent Solid-ecosystem projects cover the same ground VisionClaw touches
at the intersection of ontology identity and Pod-hosted content:

- **URN-Solid** (`https://urn-solid.github.io/`) — a registry of
  location-independent identifiers of the form `urn:solid:<Name>` that map,
  via `owl:sameAs`, to established RDF vocabularies (foaf, schema.org,
  dcterms, vcard, activitystreams, prov, ldp, solid). Serves two audiences:
  humans (term pages with inline JSON-LD as spec) and LLMs/agents (single
  `/corpus.jsonl` fetch plus `llms.txt` and `SKILL.md` integration files).
  Solves the registry-drift problem: stable names that survive across
  transport, storage, and evolving deployments.
- **Solid-Apps** (`https://solid-apps.github.io/`) — a curated platform
  for single-HTML-file Solid applications built on an integrated stack:
  LION (JSON-LD canonical format), urn-solid (type naming),
  solid-schema (JSON Schema shape constraints), solid-panes (UI routing
  manifests), and LOSOS (runtime engine for rendering + pod sync). Each
  app handles one or more `urn:solid:` types and auto-generates forms
  from the corresponding JSON Schema; schema-driven where possible,
  bespoke where warranted.

VisionClaw already has the data infrastructure that both projects assume:

- A dual-tier identity model (ADR-048) — `:KGNode` records live alongside
  `:OntologyClass` vocabulary terms joined by `BRIDGE_TO` edges.
- A default-private Pod layout (ADR-052) with a first-class `./public/kg/`
  container for published content.
- A canonical IRI scheme (ADR-050) —
  `visionclaw:owner:{npub}/kg/{sha256(relative_path)}` — that plays exactly
  the role URN-Solid's `urn:solid:<Name>` plays for vocab terms, but
  namespaced per-owner rather than per-registry.
- A `solid-pod-rs` crate (ADR-053) under active development that will be
  the shared Rust Solid server for VisionClaw and community-forum-rs.

The crossover with URN-Solid is semantic: their `urn:solid:<Name>` is a
registry-backed canonical name for vocab terms; our `visionclaw:owner:…`
is an owner-namespaced canonical name for KG records. The two spaces
should interoperate rather than duplicate. The crossover with Solid-Apps
is architectural: third-party LOSOS applications built on their stack
should be able to read and write our published KG content at
`/public/kg/{slug}` without any custom VisionClaw client code. That's
the free distribution surface every sovereignty-first project needs.

Neither project is a dependency. Neither is a replacement for our
infrastructure. Both are alignment opportunities: same problem space,
compatible philosophies, zero lock-in.

## Decision

Ecosystem alignment across four surfaces, all additive.

### 1. OntologyClass emits `owl:sameAs urn:solid:<Name>`

Where an entry on `:OntologyClass` has a well-known vocabulary equivalent
covered by URN-Solid's registry (e.g. `bc:Person` → `urn:solid:Person`,
`bc:Document` → `urn:solid:Document`), the class gets an `owl:sameAs`
predicate pointing at the URN-Solid term. The mapping table is maintained
in a new `docs/reference/urn-solid-mapping.md` with provenance per entry
and is consumed by `src/services/ontology_enrichment_service.rs` at
ingest time. Unknown mappings are simply not emitted — no speculative
aliases. The existing IRI remains canonical; URN-Solid aliases are for
ecosystem discoverability, not replacement.

### 2. Per-user `corpus.jsonl` published at `./public/kg/corpus.jsonl`

For any user with at least one `visibility: Public` KG node, the ingest
saga (ADR-051) also writes a `corpus.jsonl` file to their Pod's
`./public/kg/` container. Each line is a JSON-LD document describing
one public KG node: canonical IRI, label, `owl:sameAs` aliases (including
any URN-Solid equivalents), and the page's Pod URI. Regenerated on every
publish/unpublish transition. LLM and LOSOS-app consumption pattern
parallel to URN-Solid's own corpus file. This is also the primary
federation-readiness artefact: downstream tooling can crawl a user's
entire public KG with one HTTP request.

### 3. JSON-LD content negotiation in `solid-pod-rs`

Our Phase 1 scaffold (ADR-053) wires Turtle via sophia; Phase 2 adds
JSON-LD request/response handling. `GET /public/kg/{slug}`  and
`PUT /public/kg/{slug}` both honour `Accept: application/ld+json` and
`Content-Type: application/ld+json`. Internally we still store resources
in whatever native format the backend chose, but the wire format is
negotiable. This is what Solid-Apps's LION layer expects by default.

### 4. `urn:solid:KGNode` type manifest registration

We publish a JSON Schema at `./public/schema/kg-node.schema.json` on each
Pod describing the `:KGNode` shape (the same shape documented in
`docs/reference/neo4j-schema-unified.md`) plus a minimal type manifest
at `./public/schema/manifest.jsonld` declaring
`urn:solid:KGNode` → our schema URL. A LOSOS app that handles
`urn:solid:KGNode` can then read/write any user's KG content directly
from their Pod using the same schema every VisionClaw instance publishes.

## Consequences

### Positive

- **Zero-friction ecosystem read**: any LOSOS app or URN-Solid-aware
  crawler can consume public KG content from any user's Pod without
  VisionClaw-specific code
- **Discoverability**: `owl:sameAs urn:solid:<Name>` links make our
  OntologyClass entries findable via generic RDF tooling
- **LLM-ready corpus**: `corpus.jsonl` per user is a single fetch that
  feeds an agent the entire public surface of a user's graph in
  canonical form
- **Sovereignty preserved**: all alignment happens at the `/public/`
  container boundary — private content is untouched and never surfaces
  in corpus.jsonl or the type manifest
- **Low implementation cost**: ~10 LOC in WAC to accept `urn:solid:`
  scheme, ~50 LOC in solid-pod-rs for JSON-LD round-tripping via
  oxigraph, plus docs + mapping table
- **Strategic positioning**: aligns VisionClaw with the pragmatic slice
  of the Solid ecosystem that's actually building real apps, not the
  spec-bodies-only part

### Negative

- **Registry dependency risk (minor)**: URN-Solid is community-operated;
  term drift or shutdown requires us to maintain our own snapshot. We
  vendor a pinned copy of the mapping table in-tree so this is tolerable
- **JSON-LD surface expansion**: every Pod endpoint now needs JSON-LD
  content negotiation; complicates solid-pod-rs by a fixed amount
- **Corpus size**: for users with large public graphs, `corpus.jsonl`
  can be large. Mitigation: streaming writer, generated lazily

### Neutral

- `corpus.jsonl` content overlaps with Solid Notifications streams but
  does not replace them — notifications are change events, corpus is
  a snapshot

## Non-Goals (v1)

- Running any Solid-Apps LOSOS application inside VisionClaw itself
- Adopting solid-panes UI routing (we use React Three Fiber,
  orthogonal paradigm)
- Writing URN-Solid registry entries upstream (ecosystem contribution
  path, not a VisionClaw responsibility)
- NIP-44 encryption of `corpus.jsonl` (public-by-definition — no
  encryption needed)

## Compliance Criteria

- [ ] `docs/reference/urn-solid-mapping.md` exists with vocabulary ↔
      URN-Solid term mappings, each with provenance
- [ ] `ontology_enrichment_service` emits `owl:sameAs urn:solid:<Name>`
      for every mapped `:OntologyClass` on ingest
- [ ] Publish saga writes `./public/kg/corpus.jsonl` to every user's
      Pod with at least one public node; regenerates on
      publish/unpublish transitions
- [ ] `solid-pod-rs` negotiates `application/ld+json` for GET and PUT
      on `./public/` resources
- [ ] Each Pod publishes `./public/schema/kg-node.schema.json` and
      `./public/schema/manifest.jsonld` with the `urn:solid:KGNode`
      type binding
- [ ] WAC accepts `urn:solid:` scheme in agent identifiers
- [ ] Integration test: fetch `corpus.jsonl` from a test Pod, validate
      each line against `kg-node.schema.json`

## Rollback

- Feature flag `URN_SOLID_ALIGNMENT=true|false` gates the entire
  behaviour set
- With the flag off: no `owl:sameAs urn:solid:*` triples emitted, no
  `corpus.jsonl` written, no schema manifest published
- Mapping table is inert data and can be deleted if needed
- No external or backwards-compatibility effect beyond those
  additional emissions

## Related Documents

- ADR-048 — Dual-tier identity model (KGNode ↔ OntologyClass)
- ADR-050 — Pod-backed KGNode schema (visibility, pod_url, opaque_id)
- ADR-051 — Visibility transitions (publish/unpublish saga)
- ADR-052 — Pod default WAC + public container model
- ADR-053 — solid-pod-rs crate extraction + JSON-LD Phase 2 scope

## References

- URN-Solid registry: https://urn-solid.github.io/
- Solid-Apps platform: https://solid-apps.github.io/
- JSON-LD 1.1 specification
- Solid Protocol 0.11
