//! HTTP handler for ontology agent tools.
//!
//! Exposes the OntologyQueryService and OntologyMutationService as REST endpoints
//! that agents call via MCP tool routing. Each endpoint mirrors an MCP tool:
//!   POST /ontology-agent/discover
//!   POST /ontology-agent/read
//!   POST /ontology-agent/query
//!   POST /ontology-agent/traverse
//!   POST /ontology-agent/propose
//!   POST /ontology-agent/validate
//!   GET  /ontology-agent/status

use crate::services::ontology_mutation_service::OntologyMutationService;
use crate::services::ontology_query_service::OntologyQueryService;
use crate::types::ontology_tools::*;
use crate::{error_json, ok_json};
use actix_web::{web, Error, HttpResponse};
use log::{error, info};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// ---------- Request / Response DTOs ----------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverRequest {
    pub query: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
    pub domain: Option<String>,
}

fn default_limit() -> usize {
    20
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadNoteRequest {
    pub iri: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryRequest {
    pub cypher: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraverseRequest {
    pub start_iri: String,
    #[serde(default = "default_depth")]
    pub depth: usize,
    pub relationship_types: Option<Vec<String>>,
}

fn default_depth() -> usize {
    3
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposeRequest {
    pub proposal: ProposeInput,
    pub agent_context: AgentContext,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidateRequest {
    pub axioms: Vec<AxiomInput>,
}

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub service: String,
    pub status: String,
    pub capabilities: Vec<String>,
}

// ---------- Handlers ----------

/// POST /ontology-agent/discover — Semantic discovery via class hierarchy + Whelk
pub async fn discover(
    query_service: web::Data<Arc<OntologyQueryService>>,
    request: web::Json<DiscoverRequest>,
) -> Result<HttpResponse, Error> {
    let req = request.into_inner();
    info!("ontology-agent/discover: query='{}'", req.query);

    match query_service
        .discover(&req.query, req.limit, req.domain.as_deref())
        .await
    {
        Ok(results) => {
            ok_json!(serde_json::json!({
                "success": true,
                "results": results,
                "count": results.len()
            }))
        }
        Err(e) => {
            error!("ontology-agent/discover failed: {}", e);
            error_json!("Discovery failed", e)
        }
    }
}

/// POST /ontology-agent/read — Read note with full ontology context
pub async fn read_note(
    query_service: web::Data<Arc<OntologyQueryService>>,
    request: web::Json<ReadNoteRequest>,
) -> Result<HttpResponse, Error> {
    let req = request.into_inner();
    info!("ontology-agent/read: iri='{}'", req.iri);

    match query_service.read_note(&req.iri).await {
        Ok(note) => {
            ok_json!(serde_json::json!({
                "success": true,
                "note": note
            }))
        }
        Err(e) => {
            error!("ontology-agent/read failed: {}", e);
            error_json!("Read note failed", e)
        }
    }
}

/// POST /ontology-agent/query — Validated Cypher execution
pub async fn query(
    query_service: web::Data<Arc<OntologyQueryService>>,
    request: web::Json<QueryRequest>,
) -> Result<HttpResponse, Error> {
    let req = request.into_inner();
    info!(
        "ontology-agent/query: cypher='{}'",
        &req.cypher[..req.cypher.len().min(80)]
    );

    match query_service.validate_and_execute_cypher(&req.cypher).await {
        Ok(validation) => {
            ok_json!(serde_json::json!({
                "success": true,
                "validation": validation
            }))
        }
        Err(e) => {
            error!("ontology-agent/query failed: {}", e);
            error_json!("Query validation failed", e)
        }
    }
}

/// POST /ontology-agent/traverse — Walk ontology graph from a starting IRI
pub async fn traverse(
    query_service: web::Data<Arc<OntologyQueryService>>,
    request: web::Json<TraverseRequest>,
) -> Result<HttpResponse, Error> {
    let req = request.into_inner();
    info!(
        "ontology-agent/traverse: start='{}', depth={}",
        req.start_iri, req.depth
    );

    // Traverse by reading the start note and following relationships
    let result = build_traversal(
        &query_service,
        &req.start_iri,
        req.depth,
        req.relationship_types.as_deref(),
    )
    .await;

    match result {
        Ok(traversal) => {
            ok_json!(serde_json::json!({
                "success": true,
                "traversal": traversal
            }))
        }
        Err(e) => {
            error!("ontology-agent/traverse failed: {}", e);
            error_json!("Traversal failed", e)
        }
    }
}

/// POST /ontology-agent/propose — Propose new note or amendment
pub async fn propose(
    mutation_service: web::Data<Arc<OntologyMutationService>>,
    request: web::Json<ProposeRequest>,
) -> Result<HttpResponse, Error> {
    let req = request.into_inner();
    info!(
        "ontology-agent/propose: agent={}",
        req.agent_context.agent_id
    );

    let result = match req.proposal {
        ProposeInput::Create(proposal) => {
            mutation_service
                .propose_create(proposal, req.agent_context)
                .await
        }
        ProposeInput::Amend {
            target_iri,
            amendment,
        } => {
            mutation_service
                .propose_amend(&target_iri, amendment, req.agent_context)
                .await
        }
    };

    match result {
        Ok(proposal_result) => {
            ok_json!(serde_json::json!({
                "success": true,
                "proposal": proposal_result
            }))
        }
        Err(e) => {
            error!("ontology-agent/propose failed: {}", e);
            error_json!("Proposal failed", e)
        }
    }
}

/// POST /ontology-agent/validate — Check axioms for Whelk consistency
pub async fn validate(
    query_service: web::Data<Arc<OntologyQueryService>>,
    request: web::Json<ValidateRequest>,
) -> Result<HttpResponse, Error> {
    let req = request.into_inner();
    info!("ontology-agent/validate: {} axioms", req.axioms.len());

    // Build Cypher-like validation by checking each axiom subject/object against known classes
    let mut all_errors = Vec::new();
    let mut all_hints = Vec::new();

    for axiom in &req.axioms {
        // Validate subject exists — sanitise label to prevent Cypher injection (NEW-S3)
        let label = axiom.subject.split(':').last().unwrap_or(&axiom.subject);
        if !is_safe_cypher_label(label) {
            all_errors.push(format!("Invalid axiom subject: {}", axiom.subject));
            continue;
        }
        let subject_check = format!("MATCH (n:{}) RETURN n", label);
        if let Ok(validation) = query_service
            .validate_and_execute_cypher(&subject_check)
            .await
        {
            all_errors.extend(validation.errors);
            all_hints.extend(validation.hints);
        }
    }

    ok_json!(serde_json::json!({
        "success": true,
        "valid": all_errors.is_empty(),
        "errors": all_errors,
        "hints": all_hints,
        "axiom_count": req.axioms.len()
    }))
}

/// GET /ontology-agent/status — Service health and capability listing
pub async fn status() -> Result<HttpResponse, Error> {
    ok_json!(StatusResponse {
        service: "ontology-agent".to_string(),
        status: "healthy".to_string(),
        capabilities: vec![
            "ontology_discover".to_string(),
            "ontology_read".to_string(),
            "ontology_query".to_string(),
            "ontology_traverse".to_string(),
            "ontology_propose".to_string(),
            "ontology_validate".to_string(),
        ],
    })
}

// ---------- Helpers ----------

/// Returns `true` when `s` is safe to interpolate as a Neo4j node label.
/// Rejects empty strings, strings over 128 chars, and anything outside `[A-Za-z0-9_]`.
fn is_safe_cypher_label(s: &str) -> bool {
    !s.is_empty() && s.len() <= 128 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Build a traversal result by walking the ontology graph via read_note relationships.
async fn build_traversal(
    query_service: &OntologyQueryService,
    start_iri: &str,
    max_depth: usize,
    rel_filter: Option<&[String]>,
) -> Result<TraversalResult, String> {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut visited = std::collections::HashSet::new();
    let mut queue = std::collections::VecDeque::new();

    queue.push_back((start_iri.to_string(), 0usize));
    visited.insert(start_iri.to_string());

    while let Some((current_iri, depth)) = queue.pop_front() {
        if depth > max_depth {
            continue;
        }

        // Read the note to get relationships
        match query_service.read_note(&current_iri).await {
            Ok(note) => {
                nodes.push(TraversalNode {
                    iri: note.iri.clone(),
                    preferred_term: note.preferred_term.clone(),
                    domain: note.ontology_metadata.domain.clone(),
                    depth,
                });

                // Follow related notes
                for related in &note.related_notes {
                    let rel_type = &related.relationship_type;
                    let should_follow = rel_filter
                        .map(|types| types.iter().any(|t| t == rel_type))
                        .unwrap_or(true);

                    if should_follow {
                        edges.push(TraversalEdge {
                            source_iri: current_iri.clone(),
                            target_iri: related.iri.clone(),
                            relationship_type: rel_type.clone(),
                        });

                        if depth + 1 <= max_depth && !visited.contains(&related.iri) {
                            visited.insert(related.iri.clone());
                            queue.push_back((related.iri.clone(), depth + 1));
                        }
                    }
                }
            }
            Err(e) => {
                // Skip nodes that can't be read (may not exist)
                log::debug!("Traversal: skipping {} — {}", current_iri, e);
            }
        }
    }

    Ok(TraversalResult {
        start_iri: start_iri.to_string(),
        nodes,
        edges,
    })
}

// ---------- Route Configuration ----------

pub fn configure_ontology_agent_routes(cfg: &mut web::ServiceConfig) {
    use crate::middleware::RequireAuth;

    cfg.service(
        web::scope("/ontology-agent")
            // Every ontology-agent route performs reads/mutations against the
            // knowledge graph with side effects (proposals, validations,
            // traversals that touch private nodes). Require authentication
            // across the scope — anonymous callers get 401.
            .wrap(RequireAuth::authenticated())
            .route("/discover", web::post().to(discover))
            .route("/read", web::post().to(read_note))
            .route("/query", web::post().to(query))
            .route("/traverse", web::post().to(traverse))
            .route("/propose", web::post().to(propose))
            .route("/validate", web::post().to(validate))
            .route("/status", web::get().to(status)),
    );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ---- is_safe_cypher_label unit tests ----

    #[test]
    fn test_safe_label_alphanumeric() {
        assert!(is_safe_cypher_label("Person"));
        assert!(is_safe_cypher_label("OWL_Class"));
        assert!(is_safe_cypher_label("node123"));
        assert!(is_safe_cypher_label("A"));
    }

    #[test]
    fn test_safe_label_underscore_allowed() {
        assert!(is_safe_cypher_label("my_label"));
        assert!(is_safe_cypher_label("_leading"));
        assert!(is_safe_cypher_label("trailing_"));
    }

    #[test]
    fn test_unsafe_label_empty() {
        assert!(!is_safe_cypher_label(""));
    }

    #[test]
    fn test_unsafe_label_special_chars() {
        assert!(!is_safe_cypher_label("DROP;--"));
        assert!(!is_safe_cypher_label("label with spaces"));
        assert!(!is_safe_cypher_label("label\ttab"));
        assert!(!is_safe_cypher_label("label\nnewline"));
        assert!(!is_safe_cypher_label("label'quote"));
        assert!(!is_safe_cypher_label("label\"doublequote"));
        assert!(!is_safe_cypher_label("label{brace}"));
        assert!(!is_safe_cypher_label("label(paren)"));
    }

    #[test]
    fn test_unsafe_label_too_long() {
        let long_label = "a".repeat(129);
        assert!(!is_safe_cypher_label(&long_label));
        // Exactly 128 should be fine
        let max_label = "a".repeat(128);
        assert!(is_safe_cypher_label(&max_label));
    }

    #[test]
    fn test_unsafe_label_unicode() {
        assert!(!is_safe_cypher_label("cafe\u{0301}")); // unicode accent
        assert!(!is_safe_cypher_label("\u{4e16}\u{754c}")); // Chinese chars
    }

    #[test]
    fn test_unsafe_label_cypher_injection_attempts() {
        assert!(!is_safe_cypher_label("Person` DETACH DELETE n //"));
        assert!(!is_safe_cypher_label("Person OR 1=1"));
        assert!(!is_safe_cypher_label("MATCH (n) RETURN n"));
    }

    // ---- DiscoverRequest deserialization ----

    #[test]
    fn test_discover_request_defaults() {
        let json = r#"{"query": "neural network"}"#;
        let req: DiscoverRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.query, "neural network");
        assert_eq!(req.limit, 20); // default_limit()
        assert!(req.domain.is_none());
    }

    #[test]
    fn test_discover_request_with_domain() {
        let json = r#"{"query": "ai", "limit": 5, "domain": "technology"}"#;
        let req: DiscoverRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.limit, 5);
        assert_eq!(req.domain.as_deref(), Some("technology"));
    }

    // ---- QueryRequest deserialization ----

    #[test]
    fn test_query_request_deser() {
        let json = r#"{"cypher": "MATCH (n:Person) RETURN n LIMIT 10"}"#;
        let req: QueryRequest = serde_json::from_str(json).unwrap();
        assert!(req.cypher.contains("Person"));
    }

    // ---- ReadNoteRequest deserialization ----

    #[test]
    fn test_read_note_request_deser() {
        let json = r#"{"iri": "mv:Person"}"#;
        let req: ReadNoteRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.iri, "mv:Person");
    }

    // ---- TraverseRequest deserialization ----

    #[test]
    fn test_traverse_request_defaults() {
        let json = r#"{"startIri": "mv:Person"}"#;
        let req: TraverseRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.start_iri, "mv:Person");
        assert_eq!(req.depth, 3); // default_depth()
        assert!(req.relationship_types.is_none());
    }

    #[test]
    fn test_traverse_request_with_filters() {
        let json = r#"{"startIri": "mv:Tech", "depth": 5, "relationshipTypes": ["SUBCLASS_OF"]}"#;
        let req: TraverseRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.depth, 5);
        assert_eq!(req.relationship_types.as_ref().unwrap().len(), 1);
    }

    // ---- ValidateRequest deserialization ----

    #[test]
    fn test_validate_request_deser() {
        let json = r#"{"axioms": [{"subject": "mv:Person", "predicate": "rdfs:subClassOf", "object": "mv:Entity"}]}"#;
        let req: ValidateRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.axioms.len(), 1);
        assert_eq!(req.axioms[0].subject, "mv:Person");
    }

    // ---- StatusResponse serialization ----

    #[test]
    fn test_status_response_serialization() {
        let resp = StatusResponse {
            service: "ontology-agent".to_string(),
            status: "healthy".to_string(),
            capabilities: vec!["ontology_discover".to_string()],
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("ontology-agent"));
        assert!(json.contains("healthy"));
        assert!(json.contains("ontology_discover"));
    }

    // ---- status handler returns correct capabilities ----

    #[tokio::test]
    async fn test_status_handler_returns_all_capabilities() {
        let resp = status().await.unwrap();
        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);

        let body = actix_web::body::to_bytes(resp.into_body()).await.unwrap();
        let parsed: StatusResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(parsed.service, "ontology-agent");
        assert_eq!(parsed.status, "healthy");
        assert_eq!(parsed.capabilities.len(), 6);
        assert!(parsed
            .capabilities
            .contains(&"ontology_discover".to_string()));
        assert!(parsed.capabilities.contains(&"ontology_read".to_string()));
        assert!(parsed.capabilities.contains(&"ontology_query".to_string()));
        assert!(parsed
            .capabilities
            .contains(&"ontology_traverse".to_string()));
        assert!(parsed
            .capabilities
            .contains(&"ontology_propose".to_string()));
        assert!(parsed
            .capabilities
            .contains(&"ontology_validate".to_string()));
    }
}
