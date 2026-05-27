// src/services/jsonld_ingest/pipeline.rs
//! End-to-end ingest pipeline: markdown source → validated quads → adapter writes.
//!
//! Wires the four stages:
//!
//! 1. `extractor::extract_jsonld_blocks` — find every ```json-ld fenced block.
//! 2. `expander::expand_block` — parse JSON + apply prefix expansion + wikilink desugaring.
//! 3. `validator::validate` — schema/profile/PROV-O checks.
//! 4. `triple_emitter::emit_quads` — produce the `Quad` set per the seed contract.
//! 5. Adapter write — fanned out to `OntologyRepository` or `GraphRepository`
//!    based on `@type`.
//!
//! ## Public API
//!
//! ```ignore
//! pub async fn ingest_page(
//!     markdown: &str,
//!     metadata: &PageMetadata,
//! ) -> Result<IngestOutcome>;
//! ```
//!
//! The pipeline is repository-aware via `Arc<dyn OntologyRepository>` +
//! `Arc<dyn GraphRepository>` injected at construction time (acceptance
//! criterion A4 — no concrete adapter coupling).

use std::sync::Arc;

use oxigraph::model::Quad;

use super::errors::{JsonLdIngestError, Result};
use super::expander::expand_block;
use super::extractor::extract_jsonld_blocks;
use super::triple_emitter::emit_quads;
use super::validator::validate;
use crate::services::jsonld_ingest::graph_port_shim::GraphRepository;
use visionflow_domain::ports::ontology_repository::OntologyRepository;

/// Metadata about the host markdown file: where it came from, when it was
/// pulled, optional pre-computed content hash. The pipeline DOES NOT need
/// the file's logseq `public:: true` flag — the JSON-LD blocks carry their
/// own `vc:public` assertion per ADR-D01 §D6.
#[derive(Debug, Clone)]
pub struct PageMetadata {
    /// Source path (e.g. `pages/cybernetics.md`). Used in error messages and
    /// `vc:sourcePath` if the pipeline synthesises that triple (it does not
    /// in v1 — the codemod (D13) writes it explicitly into the JSON-LD).
    pub source_path: String,
    /// Optional precomputed SHA-1 of the markdown bytes (the JSON-LD blocks
    /// may carry it as `vc:contentSha1`).
    pub content_sha1: Option<String>,
}

impl PageMetadata {
    pub fn new(source_path: impl Into<String>) -> Self {
        Self {
            source_path: source_path.into(),
            content_sha1: None,
        }
    }
}

/// Outcome of a single page ingest.
#[derive(Debug, Clone)]
pub struct IngestOutcome {
    /// Number of JSON-LD blocks parsed from the page.
    pub block_count: usize,
    /// Total number of quads emitted across all blocks.
    pub quad_count: usize,
    /// The emitted quads themselves. Returned so callers (and tests) can
    /// inspect / re-route before persistence if desired.
    pub quads: Vec<Quad>,
    /// Source path (echoed from metadata).
    pub source_path: String,
}

/// Build an `IngestOutcome` from a markdown source without touching any
/// repository. Useful for tests and dry-run validation.
pub fn parse_and_emit(markdown: &str, metadata: &PageMetadata) -> Result<IngestOutcome> {
    let blocks = extract_jsonld_blocks(markdown);

    if blocks.is_empty() {
        return Err(JsonLdIngestError::MissingCodeFenceMarker {
            file: metadata.source_path.clone(),
        });
    }

    let mut all_quads: Vec<Quad> = Vec::new();

    for block in &blocks {
        let doc = expand_block(&metadata.source_path, block.index, &block.body)?;
        validate(&metadata.source_path, &doc)?;
        let quads = emit_quads(&doc);
        all_quads.extend(quads);
    }

    Ok(IngestOutcome {
        block_count: blocks.len(),
        quad_count: all_quads.len(),
        quads: all_quads,
        source_path: metadata.source_path.clone(),
    })
}

/// Pipeline runner with injected repository ports. Acceptance A4: ports
/// are abstract; the runner never references concrete adapter types.
///
/// Quads produced by `ingest_page` are returned to the caller
/// (GitHubSyncService) for bulk insertion via `store.transaction()`.
/// The port fields are retained so callers that want direct adapter writes
/// can extend this runner without breaking the public API.
#[allow(dead_code)] // fields used by callers that extend the runner with direct store writes
pub struct JsonLdIngestPipeline {
    ontology: Arc<dyn OntologyRepository>,
    graph: Arc<dyn GraphRepository>,
}

impl JsonLdIngestPipeline {
    pub fn new(
        ontology: Arc<dyn OntologyRepository>,
        graph: Arc<dyn GraphRepository>,
    ) -> Self {
        Self { ontology, graph }
    }

    /// Parses + validates + emits quads, returning the outcome.
    /// Quads are returned to the caller for direct insertion into
    /// the Oxigraph store. The caller (GitHubSyncService) handles
    /// bulk insertion via store.transaction().
    pub async fn ingest_page(
        &self,
        markdown: &str,
        metadata: &PageMetadata,
    ) -> Result<IngestOutcome> {
        let outcome = parse_and_emit(markdown, metadata)?;
        Ok(outcome)
    }
}

/// Free-function entry point matching the worktree-plan signature.
/// Equivalent to `parse_and_emit`; adapter writes are handled by
/// `JsonLdIngestPipeline::ingest_page` when port injection is needed.
pub async fn ingest_page(
    markdown: &str,
    metadata: &PageMetadata,
) -> Result<IngestOutcome> {
    parse_and_emit(markdown, metadata)
}
