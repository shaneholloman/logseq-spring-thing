// src/services/jsonld_ingest/mod.rs
//! JSON-LD ingest pipeline (Migration Sprint Phase 2 M1).
//!
//! Bridges Logseq markdown source files carrying embedded ```json-ld``` blocks
//! to RDF quad sets routed through the Phase 1 Oxigraph repository adapters
//! (`OntologyRepository` and `GraphRepository` ports).
//!
//! ## Pipeline stages
//!
//! ```text
//! markdown ──▶ extractor ──▶ expander ──▶ validator ──▶ triple_emitter ──▶ adapters
//!              (find         (parse +     (schema +     (Vec<Quad>           (M2)
//!              fenced        prefix-      profile +     in seed
//!              blocks)        expand)     PROV-O)        contract)
//! ```
//!
//! Each stage owns a single concern; see the module docs on `extractor.rs`,
//! `expander.rs`, `validator.rs`, `triple_emitter.rs`, and `pipeline.rs`.
//!
//! ## Public API
//!
//! The two entry points the caller cares about:
//!
//! - [`pipeline::ingest_page`] — free async function matching the
//!   worktree-plan signature `ingest_page(markdown, metadata) -> IngestOutcome`.
//! - [`pipeline::JsonLdIngestPipeline`] — port-injected runner used when the
//!   caller wants to attach the `OntologyRepository` and `GraphRepository`
//!   ports for the M2 adapter-write phase.
//!
//! Both produce an [`pipeline::IngestOutcome`] carrying `Vec<oxigraph::model::Quad>`
//! that callers can persist via the adapters.
//!
//! ## Why not a third-party json-ld crate
//!
//! See `expander.rs` module docs — TL;DR: ADR-D01 §R3 directs us to embed the
//! canonical `@context` inline and do manual term expansion. Available
//! `json-ld` / `sophia_jsonld` crates pull large dep trees that the seed
//! emission contract does not exercise.
//!
//! ## Feature gating
//!
//! Compiled only when `persistence-oxigraph` is enabled (this module imports
//! `oxigraph::model::Quad`).

pub mod errors;
pub mod expander;
pub mod extractor;
pub mod pipeline;
pub mod triple_emitter;
pub mod validator;

pub use errors::{JsonLdIngestError, Result};
pub use pipeline::{ingest_page, IngestOutcome, JsonLdIngestPipeline, PageMetadata};
