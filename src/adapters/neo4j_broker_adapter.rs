//! Neo4j Broker Repository Adapter
//!
//! Implements the BrokerRepository port using Neo4j for persistent storage
//! of broker cases and decisions. Follows the flat-column projection pattern
//! established by neo4j_adapter.rs to avoid Bolt node extraction issues.

use async_trait::async_trait;
use log::{debug, info};
use neo4rs::{Graph, Query};
use std::sync::Arc;

use crate::models::enterprise::*;
use crate::ports::broker_repository::{BrokerError, BrokerRepository, Result};

/// Neo4j-backed broker case repository.
///
/// Uses the shared `Arc<Graph>` connection pool from the Neo4j adapter
/// and stores cases as `:BrokerCase` nodes, decisions as `:BrokerDecision`
/// nodes with `[:DECIDES]` relationships.
pub struct Neo4jBrokerRepository {
    graph: Arc<Graph>,
}

impl Neo4jBrokerRepository {
    pub fn new(graph: Arc<Graph>) -> Self {
        Self { graph }
    }

    /// Ensure broker schema indexes exist (called once at startup).
    pub async fn create_schema(&self) -> std::result::Result<(), BrokerError> {
        let queries = [
            "CREATE CONSTRAINT broker_case_id IF NOT EXISTS FOR (c:BrokerCase) REQUIRE c.id IS UNIQUE",
            "CREATE CONSTRAINT broker_decision_id IF NOT EXISTS FOR (d:BrokerDecision) REQUIRE d.id IS UNIQUE",
            "CREATE INDEX broker_case_status IF NOT EXISTS FOR (c:BrokerCase) ON (c.status)",
            "CREATE INDEX broker_decision_case IF NOT EXISTS FOR (d:BrokerDecision) ON (d.case_id)",
        ];

        for q in &queries {
            if let Err(e) = self.graph.run(Query::new(q.to_string())).await {
                // Constraints/indexes may already exist; log and continue.
                debug!("Broker schema DDL (may already exist): {}", e);
            }
        }

        info!("Broker Neo4j schema indexes ensured");
        Ok(())
    }

    /// Serialize an enum via serde_json to its snake_case string representation.
    fn enum_to_str<T: serde::Serialize>(val: &T) -> std::result::Result<String, BrokerError> {
        let json = serde_json::to_value(val)
            .map_err(|e| BrokerError::ValidationError(e.to_string()))?;
        Ok(json.as_str().unwrap_or("unknown").to_string())
    }

    /// Deserialize a snake_case string into an enum, falling back to a default.
    fn str_to_enum<T: serde::de::DeserializeOwned>(s: &str, default: T) -> T {
        serde_json::from_value(serde_json::Value::String(s.to_string())).unwrap_or(default)
    }
}

#[async_trait]
impl BrokerRepository for Neo4jBrokerRepository {
    async fn list_cases(
        &self,
        status: Option<CaseStatus>,
        limit: usize,
    ) -> Result<Vec<BrokerCase>> {
        let (query_str, has_param) = match &status {
            Some(s) => {
                let status_str = Self::enum_to_str(s)?;
                debug!("list_cases: filtering by status={}", status_str);
                (
                    format!(
                        "MATCH (c:BrokerCase {{status: $status}}) \
                         RETURN c.id AS id, c.title AS title, c.description AS description, \
                                c.priority AS priority, c.source AS source, c.status AS status, \
                                c.created_at AS created_at, c.updated_at AS updated_at, \
                                c.assigned_to AS assigned_to \
                         ORDER BY c.created_at DESC LIMIT {}",
                        limit
                    ),
                    Some(status_str),
                )
            }
            None => (
                format!(
                    "MATCH (c:BrokerCase) \
                     RETURN c.id AS id, c.title AS title, c.description AS description, \
                            c.priority AS priority, c.source AS source, c.status AS status, \
                            c.created_at AS created_at, c.updated_at AS updated_at, \
                            c.assigned_to AS assigned_to \
                     ORDER BY c.created_at DESC LIMIT {}",
                    limit
                ),
                None,
            ),
        };

        let mut query = Query::new(query_str);
        if let Some(ref status_val) = has_param {
            query = query.param("status", status_val.as_str());
        }

        let mut result = self.graph.execute(query).await.map_err(|e| {
            BrokerError::DatabaseError(format!("Failed to list cases: {}", e))
        })?;

        let mut cases = Vec::new();
        while let Ok(Some(row)) = result.next().await {
            let id: String = row.get("id").unwrap_or_default();
            let title: String = row.get("title").unwrap_or_default();
            let description: String = row.get("description").unwrap_or_default();
            let priority_str: String = row.get("priority").unwrap_or_else(|_| "medium".to_string());
            let source_str: String =
                row.get("source").unwrap_or_else(|_| "manual_submission".to_string());
            let status_str: String = row.get("status").unwrap_or_else(|_| "open".to_string());
            let created_at: String = row.get("created_at").unwrap_or_default();
            let updated_at: String = row.get("updated_at").unwrap_or_default();
            let assigned_to: Option<String> = row.get::<String>("assigned_to").ok().filter(|s| !s.is_empty());

            cases.push(BrokerCase {
                id,
                title,
                description,
                priority: Self::str_to_enum(&priority_str, CasePriority::Medium),
                source: Self::str_to_enum(&source_str, EscalationSource::ManualSubmission),
                status: Self::str_to_enum(&status_str, CaseStatus::Open),
                created_at,
                updated_at,
                assigned_to,
                evidence: vec![],
                metadata: std::collections::HashMap::new(),
            });
        }

        debug!("list_cases: returned {} cases", cases.len());
        Ok(cases)
    }

    async fn get_case(&self, case_id: &str) -> Result<Option<BrokerCase>> {
        let query = Query::new(
            "MATCH (c:BrokerCase {id: $id}) \
             RETURN c.id AS id, c.title AS title, c.description AS description, \
                    c.priority AS priority, c.source AS source, c.status AS status, \
                    c.created_at AS created_at, c.updated_at AS updated_at, \
                    c.assigned_to AS assigned_to"
                .to_string(),
        )
        .param("id", case_id);

        let mut result = self.graph.execute(query).await.map_err(|e| {
            BrokerError::DatabaseError(format!("Failed to get case: {}", e))
        })?;

        if let Ok(Some(row)) = result.next().await {
            let id: String = row.get("id").unwrap_or_default();
            let title: String = row.get("title").unwrap_or_default();
            let description: String = row.get("description").unwrap_or_default();
            let priority_str: String = row.get("priority").unwrap_or_else(|_| "medium".to_string());
            let source_str: String =
                row.get("source").unwrap_or_else(|_| "manual_submission".to_string());
            let status_str: String = row.get("status").unwrap_or_else(|_| "open".to_string());
            let created_at: String = row.get("created_at").unwrap_or_default();
            let updated_at: String = row.get("updated_at").unwrap_or_default();
            let assigned_to: Option<String> = row.get::<String>("assigned_to").ok().filter(|s| !s.is_empty());

            Ok(Some(BrokerCase {
                id,
                title,
                description,
                priority: Self::str_to_enum(&priority_str, CasePriority::Medium),
                source: Self::str_to_enum(&source_str, EscalationSource::ManualSubmission),
                status: Self::str_to_enum(&status_str, CaseStatus::Open),
                created_at,
                updated_at,
                assigned_to,
                evidence: vec![],
                metadata: std::collections::HashMap::new(),
            }))
        } else {
            Ok(None)
        }
    }

    async fn create_case(&self, case: &BrokerCase) -> Result<()> {
        let priority_str = Self::enum_to_str(&case.priority)?;
        let source_str = Self::enum_to_str(&case.source)?;
        let status_str = Self::enum_to_str(&case.status)?;
        let assigned_to = case.assigned_to.as_deref().unwrap_or("");

        let query = Query::new(
            "CREATE (c:BrokerCase {
                id: $id,
                title: $title,
                description: $description,
                priority: $priority,
                source: $source,
                status: $status,
                created_at: $created_at,
                updated_at: $updated_at,
                assigned_to: $assigned_to
            })"
            .to_string(),
        )
        .param("id", case.id.as_str())
        .param("title", case.title.as_str())
        .param("description", case.description.as_str())
        .param("priority", priority_str.as_str())
        .param("source", source_str.as_str())
        .param("status", status_str.as_str())
        .param("created_at", case.created_at.as_str())
        .param("updated_at", case.updated_at.as_str())
        .param("assigned_to", assigned_to);

        self.graph.run(query).await.map_err(|e| {
            BrokerError::DatabaseError(format!("Failed to create case: {}", e))
        })?;

        info!("Created BrokerCase {} in Neo4j", case.id);
        Ok(())
    }

    async fn update_case_status(&self, case_id: &str, status: CaseStatus) -> Result<()> {
        let status_str = Self::enum_to_str(&status)?;
        let now = chrono::Utc::now().to_rfc3339();

        let query = Query::new(
            "MATCH (c:BrokerCase {id: $id}) \
             SET c.status = $status, c.updated_at = $updated_at"
                .to_string(),
        )
        .param("id", case_id)
        .param("status", status_str.as_str())
        .param("updated_at", now.as_str());

        self.graph.run(query).await.map_err(|e| {
            BrokerError::DatabaseError(format!("Failed to update case status: {}", e))
        })?;

        debug!("Updated BrokerCase {} status to {}", case_id, status_str);
        Ok(())
    }

    async fn record_decision(&self, decision: &BrokerDecision) -> Result<()> {
        let action_str = Self::enum_to_str(&decision.action)?;

        let query = Query::new(
            "MATCH (c:BrokerCase {id: $case_id}) \
             CREATE (d:BrokerDecision {
                 id: $id,
                 case_id: $case_id,
                 action: $action,
                 reasoning: $reasoning,
                 decided_by: $decided_by,
                 decided_at: $decided_at
             }) \
             CREATE (d)-[:DECIDES]->(c) \
             SET c.status = 'decided', c.updated_at = $decided_at"
                .to_string(),
        )
        .param("id", decision.id.as_str())
        .param("case_id", decision.case_id.as_str())
        .param("action", action_str.as_str())
        .param("reasoning", decision.reasoning.as_str())
        .param("decided_by", decision.decided_by.as_str())
        .param("decided_at", decision.decided_at.as_str());

        self.graph.run(query).await.map_err(|e| {
            BrokerError::DatabaseError(format!("Failed to record decision: {}", e))
        })?;

        info!(
            "Recorded BrokerDecision {} for case {}",
            decision.id, decision.case_id
        );
        Ok(())
    }

    async fn get_decisions(&self, case_id: &str) -> Result<Vec<BrokerDecision>> {
        let query = Query::new(
            "MATCH (d:BrokerDecision {case_id: $case_id}) \
             RETURN d.id AS id, d.case_id AS case_id, d.action AS action, \
                    d.reasoning AS reasoning, d.decided_by AS decided_by, \
                    d.decided_at AS decided_at \
             ORDER BY d.decided_at DESC"
                .to_string(),
        )
        .param("case_id", case_id);

        let mut result = self.graph.execute(query).await.map_err(|e| {
            BrokerError::DatabaseError(format!("Failed to get decisions: {}", e))
        })?;

        let mut decisions = Vec::new();
        while let Ok(Some(row)) = result.next().await {
            let id: String = row.get("id").unwrap_or_default();
            let cid: String = row.get("case_id").unwrap_or_default();
            let action_str: String = row.get("action").unwrap_or_else(|_| "approve".to_string());
            let reasoning: String = row.get("reasoning").unwrap_or_default();
            let decided_by: String = row.get("decided_by").unwrap_or_default();
            let decided_at: String = row.get("decided_at").unwrap_or_default();

            decisions.push(BrokerDecision {
                id,
                case_id: cid,
                action: Self::str_to_enum(&action_str, DecisionAction::Approve),
                reasoning,
                decided_by,
                decided_at,
                provenance_event_id: None,
            });
        }

        debug!("get_decisions({}): returned {} decisions", case_id, decisions.len());
        Ok(decisions)
    }
}
