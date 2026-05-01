//! Code Analysis Actor — integrates Epic B code extractors into the actor mesh.
//!
//! Receives source file content (post-sync) and runs the appropriate
//! language-specific extractor from `graph_cognition_extract::code`. Accumulated
//! [`TypedNode`]s and [`TypedEdge`]s are stored in an in-memory [`TypedGraph`].
//! Neo4j projection is a separate concern handled downstream.
//!
//! # Messages
//!
//! | Message           | Description                              |
//! |-------------------|------------------------------------------|
//! | `AnalyzeFile`     | Extract graph data from a single file    |
//! | `AnalyzeBatch`    | Extract graph data from multiple files   |
//! | `GetAnalysisStats`| Return accumulated node/edge counts      |

use actix::prelude::*;
use log::{debug, info, warn};

use graph_cognition_extract::code::extractor_for_path;
use graph_cognition_extract::code::ExtractionResult;

// Core typed-graph schema (ADR-064). Only TypedGraph is referenced directly
// as a field type; TypedNode/TypedEdge flow through ExtractionResult.
use graph_cognition_core::TypedGraph;

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

/// Analyze a single source file and accumulate the extracted graph data.
#[derive(Message, Debug)]
#[rtype(result = "AnalyzeFileResult")]
pub struct AnalyzeFile {
    /// Project-relative file path (e.g. `src/actors/graph.rs`).
    pub file_path: String,
    /// Full file contents.
    pub content: String,
    /// Hex-encoded owner public key for URN scoping.
    pub owner_hex: String,
}

/// Result returned from [`AnalyzeFile`].
#[derive(Debug, Clone, MessageResponse)]
pub struct AnalyzeFileResult {
    pub nodes_added: usize,
    pub edges_added: usize,
    pub errors: Vec<String>,
}

/// Analyze a batch of source files in one message.
#[derive(Message, Debug)]
#[rtype(result = "AnalyzeBatchResult")]
pub struct AnalyzeBatch {
    /// Each tuple is `(file_path, content)`.
    pub files: Vec<(String, String)>,
    /// Hex-encoded owner public key for URN scoping.
    pub owner_hex: String,
}

/// Result returned from [`AnalyzeBatch`].
#[derive(Debug, Clone, MessageResponse)]
pub struct AnalyzeBatchResult {
    pub files_processed: usize,
    pub files_skipped: usize,
    pub total_nodes_added: usize,
    pub total_edges_added: usize,
    pub errors: Vec<String>,
}

/// Query the accumulated extraction statistics.
#[derive(Message, Debug)]
#[rtype(result = "AnalysisStats")]
pub struct GetAnalysisStats;

/// Statistics snapshot returned from [`GetAnalysisStats`].
#[derive(Debug, Clone, MessageResponse)]
pub struct AnalysisStats {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub files_analyzed: usize,
}

// ---------------------------------------------------------------------------
// Actor
// ---------------------------------------------------------------------------

/// Accumulates typed graph data extracted from source files by the
/// `graph-cognition-extract` code pipeline.
pub struct CodeAnalysisActor {
    graph: TypedGraph,
    files_analyzed: usize,
}

impl CodeAnalysisActor {
    pub fn new() -> Self {
        Self {
            graph: TypedGraph::new(),
            files_analyzed: 0,
        }
    }

    /// Read-only access to the accumulated graph.
    pub fn graph(&self) -> &TypedGraph {
        &self.graph
    }

    /// Run extraction on a single file and merge into the accumulated graph.
    /// Returns the extraction result (nodes, edges, errors).
    fn analyze_single(&mut self, file_path: &str, content: &str) -> ExtractionResult {
        let extractor = match extractor_for_path(file_path) {
            Some(ext) => ext,
            None => {
                debug!(
                    "CodeAnalysisActor: no extractor for '{}', skipping",
                    file_path
                );
                let mut result = ExtractionResult::empty();
                result
                    .errors
                    .push(format!("unsupported file extension: {}", file_path));
                return result;
            }
        };

        let result = extractor.extract(content, file_path);

        if !result.errors.is_empty() {
            for err in &result.errors {
                warn!(
                    "CodeAnalysisActor: extraction warning for '{}': {}",
                    file_path, err
                );
            }
        }

        debug!(
            "CodeAnalysisActor: '{}' -> {} nodes, {} edges (lang={})",
            file_path,
            result.nodes.len(),
            result.edges.len(),
            extractor.language().as_str(),
        );

        // Merge into accumulated graph
        for node in &result.nodes {
            self.graph.add_node(node.clone());
        }
        for edge in &result.edges {
            self.graph.add_edge(edge.clone());
        }

        self.files_analyzed += 1;
        result
    }
}

impl Default for CodeAnalysisActor {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Actix Actor impl
// ---------------------------------------------------------------------------

impl Actor for CodeAnalysisActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        info!("CodeAnalysisActor started");
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        info!(
            "CodeAnalysisActor stopped (accumulated {} nodes, {} edges from {} files)",
            self.graph.node_count(),
            self.graph.edge_count(),
            self.files_analyzed,
        );
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

impl Handler<AnalyzeFile> for CodeAnalysisActor {
    type Result = AnalyzeFileResult;

    fn handle(&mut self, msg: AnalyzeFile, _ctx: &mut Self::Context) -> Self::Result {
        let result = self.analyze_single(&msg.file_path, &msg.content);

        AnalyzeFileResult {
            nodes_added: result.nodes.len(),
            edges_added: result.edges.len(),
            errors: result.errors,
        }
    }
}

impl Handler<AnalyzeBatch> for CodeAnalysisActor {
    type Result = AnalyzeBatchResult;

    fn handle(&mut self, msg: AnalyzeBatch, _ctx: &mut Self::Context) -> Self::Result {
        let mut total_nodes = 0usize;
        let mut total_edges = 0usize;
        let mut files_processed = 0usize;
        let mut files_skipped = 0usize;
        let mut all_errors = Vec::new();

        for (file_path, content) in &msg.files {
            if extractor_for_path(file_path).is_none() {
                files_skipped += 1;
                continue;
            }

            let result = self.analyze_single(file_path, content);
            total_nodes += result.nodes.len();
            total_edges += result.edges.len();
            files_processed += 1;

            all_errors.extend(result.errors);
        }

        info!(
            "CodeAnalysisActor: batch complete — {} processed, {} skipped, {} nodes, {} edges",
            files_processed, files_skipped, total_nodes, total_edges,
        );

        AnalyzeBatchResult {
            files_processed,
            files_skipped,
            total_nodes_added: total_nodes,
            total_edges_added: total_edges,
            errors: all_errors,
        }
    }
}

impl Handler<GetAnalysisStats> for CodeAnalysisActor {
    type Result = AnalysisStats;

    fn handle(&mut self, _msg: GetAnalysisStats, _ctx: &mut Self::Context) -> Self::Result {
        AnalysisStats {
            total_nodes: self.graph.node_count(),
            total_edges: self.graph.edge_count(),
            files_analyzed: self.files_analyzed,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use actix::prelude::*;

    #[actix_rt::test]
    async fn analyze_single_rust_file() {
        let actor = CodeAnalysisActor::new().start();

        let result = actor
            .send(AnalyzeFile {
                file_path: "src/lib.rs".into(),
                content: "pub fn hello() {}\npub struct Foo;".into(),
                owner_hex: "deadbeef".into(),
            })
            .await
            .expect("mailbox");

        assert!(result.nodes_added > 0, "should extract at least one node");
        assert!(result.edges_added > 0, "should extract at least one edge");
        assert!(result.errors.is_empty(), "no extraction errors expected");
    }

    #[actix_rt::test]
    async fn unsupported_extension_returns_error() {
        let actor = CodeAnalysisActor::new().start();

        let result = actor
            .send(AnalyzeFile {
                file_path: "README.md".into(),
                content: "# Hello".into(),
                owner_hex: "deadbeef".into(),
            })
            .await
            .expect("mailbox");

        assert_eq!(result.nodes_added, 0);
        assert_eq!(result.edges_added, 0);
        assert!(!result.errors.is_empty());
    }

    #[actix_rt::test]
    async fn batch_processes_mixed_files() {
        let actor = CodeAnalysisActor::new().start();

        let result = actor
            .send(AnalyzeBatch {
                files: vec![
                    ("src/a.rs".into(), "pub fn a() {}".into()),
                    ("docs/readme.md".into(), "# Docs".into()),
                    ("src/b.rs".into(), "pub struct B;".into()),
                ],
                owner_hex: "cafebabe".into(),
            })
            .await
            .expect("mailbox");

        assert_eq!(result.files_processed, 2);
        assert_eq!(result.files_skipped, 1);
        assert!(result.total_nodes_added > 0);
    }

    #[actix_rt::test]
    async fn stats_accumulate_across_messages() {
        let actor = CodeAnalysisActor::new().start();

        actor
            .send(AnalyzeFile {
                file_path: "src/one.rs".into(),
                content: "pub fn one() {}".into(),
                owner_hex: "aa".into(),
            })
            .await
            .expect("mailbox");

        actor
            .send(AnalyzeFile {
                file_path: "src/two.rs".into(),
                content: "pub fn two() {}".into(),
                owner_hex: "aa".into(),
            })
            .await
            .expect("mailbox");

        let stats = actor.send(GetAnalysisStats).await.expect("mailbox");

        assert_eq!(stats.files_analyzed, 2);
        assert!(stats.total_nodes >= 4, "at least 2 module + 2 fn nodes");
        assert!(stats.total_edges >= 2, "at least 2 contains edges");
    }
}
