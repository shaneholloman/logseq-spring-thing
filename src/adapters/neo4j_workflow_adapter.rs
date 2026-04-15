// src/adapters/neo4j_workflow_adapter.rs
//! Neo4j Workflow Repository Adapter
//!
//! Implements the WorkflowRepository port using Neo4j for persistent storage
//! of workflow proposals and promoted patterns. Follows the flat-column projection
//! pattern established by neo4j_broker_adapter.rs.
//!
//! ## Schema Design
//!
//! - `:WorkflowProposal` nodes with all proposal fields (steps stored as JSON string)
//! - `:WorkflowPattern` nodes for deployed/promoted patterns
//! - `(:WorkflowProposal)-[:PROMOTED_TO]->(:WorkflowPattern)` edges

use async_trait::async_trait;
use log::{debug, info};
use neo4rs::{Graph, Query};
use std::collections::HashMap;
use std::sync::Arc;

use crate::models::enterprise::{
    WorkflowPattern, WorkflowProposal, WorkflowStatus, WorkflowStep,
};
use crate::ports::workflow_repository::{Result as WfResult, WorkflowError, WorkflowRepository};

/// Neo4j-backed workflow proposal and pattern repository.
///
/// Uses the shared `Arc<Graph>` connection pool from the Neo4j adapter
/// and stores proposals as `:WorkflowProposal` nodes, patterns as
/// `:WorkflowPattern` nodes with `[:PROMOTED_TO]` relationships.
pub struct Neo4jWorkflowRepository {
    graph: Arc<Graph>,
}

impl Neo4jWorkflowRepository {
    pub fn new(graph: Arc<Graph>) -> Self {
        Self { graph }
    }

    /// Ensure workflow schema indexes exist (called once at startup).
    pub async fn create_schema(&self) -> std::result::Result<(), WorkflowError> {
        let queries = [
            "CREATE CONSTRAINT wf_proposal_id IF NOT EXISTS FOR (p:WorkflowProposal) REQUIRE p.id IS UNIQUE",
            "CREATE CONSTRAINT wf_pattern_id IF NOT EXISTS FOR (p:WorkflowPattern) REQUIRE p.id IS UNIQUE",
            "CREATE INDEX wf_proposal_status IF NOT EXISTS FOR (p:WorkflowProposal) ON (p.status)",
            "CREATE INDEX wf_proposal_created IF NOT EXISTS FOR (p:WorkflowProposal) ON (p.created_at)",
            "CREATE INDEX wf_pattern_deployed IF NOT EXISTS FOR (p:WorkflowPattern) ON (p.deployed_at)",
        ];

        for q in &queries {
            if let Err(e) = self.graph.run(Query::new(q.to_string())).await {
                // Constraints/indexes may already exist; log and continue.
                debug!("Workflow schema DDL (may already exist): {}", e);
            }
        }

        info!("Workflow Neo4j schema indexes ensured");
        Ok(())
    }

    /// Serialize a WorkflowStatus to its snake_case string form.
    fn status_to_str(status: &WorkflowStatus) -> String {
        serde_json::to_value(status)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| "draft".to_string())
    }

    /// Parse a status string back into WorkflowStatus.
    fn str_to_status(s: &str) -> WorkflowStatus {
        serde_json::from_value(serde_json::Value::String(s.to_string()))
            .unwrap_or(WorkflowStatus::Draft)
    }

    /// Validate that a status transition is allowed.
    fn validate_transition(
        from: &WorkflowStatus,
        to: &WorkflowStatus,
    ) -> Result<(), WorkflowError> {
        let allowed = match from {
            WorkflowStatus::Draft => {
                matches!(to, WorkflowStatus::Submitted | WorkflowStatus::Archived)
            }
            WorkflowStatus::Submitted => {
                matches!(to, WorkflowStatus::UnderReview | WorkflowStatus::Archived)
            }
            WorkflowStatus::UnderReview => {
                matches!(
                    to,
                    WorkflowStatus::Approved | WorkflowStatus::Draft | WorkflowStatus::Archived
                )
            }
            WorkflowStatus::Approved => {
                matches!(to, WorkflowStatus::Deployed | WorkflowStatus::Archived)
            }
            WorkflowStatus::Deployed => {
                matches!(to, WorkflowStatus::RolledBack | WorkflowStatus::Archived)
            }
            WorkflowStatus::Archived => false,
            WorkflowStatus::RolledBack => {
                matches!(to, WorkflowStatus::Draft | WorkflowStatus::Archived)
            }
        };

        if allowed {
            Ok(())
        } else {
            Err(WorkflowError::InvalidTransition {
                from: from.clone(),
                to: to.clone(),
            })
        }
    }

    /// Build a WorkflowProposal from a Neo4j row with flat-column projection.
    fn row_to_proposal(row: &neo4rs::Row) -> Result<WorkflowProposal, WorkflowError> {
        let id: String = row.get("id").unwrap_or_default();
        let title: String = row.get("title").unwrap_or_default();
        let description: String = row.get("description").unwrap_or_default();
        let status_str: String = row.get("status").unwrap_or_else(|_| "draft".to_string());
        let version: i64 = row.get("version").unwrap_or(1);
        let steps_json: String = row.get("steps_json").unwrap_or_else(|_| "[]".to_string());
        let source_insight_id: String = row.get::<String>("source_insight_id").unwrap_or_default();
        let submitted_by: String = row.get("submitted_by").unwrap_or_default();
        let created_at: String = row.get("created_at").unwrap_or_default();
        let updated_at: String = row.get("updated_at").unwrap_or_default();
        let risk_score: f64 = row.get::<f64>("risk_score").unwrap_or(-1.0);
        let expected_benefit: String = row.get::<String>("expected_benefit").unwrap_or_default();
        let metadata_json: String = row
            .get::<String>("metadata_json")
            .unwrap_or_else(|_| "{}".to_string());

        let status = Self::str_to_status(&status_str);

        let steps: Vec<WorkflowStep> = serde_json::from_str(&steps_json).map_err(|e| {
            WorkflowError::DatabaseError(format!("Failed to parse steps JSON: {}", e))
        })?;

        let metadata: HashMap<String, String> =
            serde_json::from_str(&metadata_json).unwrap_or_default();

        Ok(WorkflowProposal {
            id,
            title,
            description,
            status,
            version: version as u32,
            steps,
            source_insight_id: if source_insight_id.is_empty() {
                None
            } else {
                Some(source_insight_id)
            },
            submitted_by,
            created_at,
            updated_at,
            risk_score: if risk_score < 0.0 {
                None
            } else {
                Some(risk_score)
            },
            expected_benefit: if expected_benefit.is_empty() {
                None
            } else {
                Some(expected_benefit)
            },
            metadata,
        })
    }

    /// Build a WorkflowPattern from a Neo4j row with flat-column projection.
    fn row_to_pattern(row: &neo4rs::Row) -> Result<WorkflowPattern, WorkflowError> {
        let id: String = row.get("id").unwrap_or_default();
        let title: String = row.get("title").unwrap_or_default();
        let description: String = row.get("description").unwrap_or_default();
        let active_version_id: String = row.get("active_version_id").unwrap_or_default();
        let deployed_at: String = row.get("deployed_at").unwrap_or_default();
        let deployed_by: String = row.get("deployed_by").unwrap_or_default();
        let adoption_count: i64 = row.get("adoption_count").unwrap_or(0);
        let rollback_target_id: String = row
            .get::<String>("rollback_target_id")
            .unwrap_or_default();

        Ok(WorkflowPattern {
            id,
            title,
            description,
            active_version_id,
            deployed_at,
            deployed_by,
            adoption_count: adoption_count as u32,
            rollback_target_id: if rollback_target_id.is_empty() {
                None
            } else {
                Some(rollback_target_id)
            },
        })
    }
}

#[async_trait]
impl WorkflowRepository for Neo4jWorkflowRepository {
    async fn list_proposals(
        &self,
        status: Option<WorkflowStatus>,
        limit: usize,
    ) -> WfResult<Vec<WorkflowProposal>> {
        let (cypher, params) = match &status {
            Some(s) => {
                let status_str = Self::status_to_str(s);
                debug!("list_proposals: filtering by status={}", status_str);
                (
                    format!(
                        "MATCH (p:WorkflowProposal {{status: $status}}) \
                         RETURN p.id AS id, p.title AS title, p.description AS description, \
                                p.status AS status, p.version AS version, p.steps_json AS steps_json, \
                                p.source_insight_id AS source_insight_id, p.submitted_by AS submitted_by, \
                                p.created_at AS created_at, p.updated_at AS updated_at, \
                                p.risk_score AS risk_score, p.expected_benefit AS expected_benefit, \
                                p.metadata_json AS metadata_json \
                         ORDER BY p.created_at DESC LIMIT {}",
                        limit
                    ),
                    Some(status_str),
                )
            }
            None => (
                format!(
                    "MATCH (p:WorkflowProposal) \
                     RETURN p.id AS id, p.title AS title, p.description AS description, \
                            p.status AS status, p.version AS version, p.steps_json AS steps_json, \
                            p.source_insight_id AS source_insight_id, p.submitted_by AS submitted_by, \
                            p.created_at AS created_at, p.updated_at AS updated_at, \
                            p.risk_score AS risk_score, p.expected_benefit AS expected_benefit, \
                            p.metadata_json AS metadata_json \
                     ORDER BY p.created_at DESC LIMIT {}",
                    limit
                ),
                None,
            ),
        };

        let q = if let Some(status_str) = params {
            Query::new(cypher).param("status", status_str)
        } else {
            Query::new(cypher)
        };

        let mut result = self.graph.execute(q).await.map_err(|e| {
            WorkflowError::DatabaseError(format!("Failed to list proposals: {}", e))
        })?;

        let mut proposals = Vec::new();
        while let Some(row) = result.next().await.map_err(|e| {
            WorkflowError::DatabaseError(format!("Failed to fetch row: {}", e))
        })? {
            proposals.push(Self::row_to_proposal(&row)?);
        }

        debug!("Listed {} proposals", proposals.len());
        Ok(proposals)
    }

    async fn get_proposal(&self, proposal_id: &str) -> WfResult<Option<WorkflowProposal>> {
        let cypher = "MATCH (p:WorkflowProposal {id: $id}) \
                      RETURN p.id AS id, p.title AS title, p.description AS description, \
                             p.status AS status, p.version AS version, p.steps_json AS steps_json, \
                             p.source_insight_id AS source_insight_id, p.submitted_by AS submitted_by, \
                             p.created_at AS created_at, p.updated_at AS updated_at, \
                             p.risk_score AS risk_score, p.expected_benefit AS expected_benefit, \
                             p.metadata_json AS metadata_json";

        let q = Query::new(cypher.to_string()).param("id", proposal_id.to_string());

        let mut result = self.graph.execute(q).await.map_err(|e| {
            WorkflowError::DatabaseError(format!("Failed to get proposal: {}", e))
        })?;

        match result.next().await.map_err(|e| {
            WorkflowError::DatabaseError(format!("Failed to fetch row: {}", e))
        })? {
            Some(row) => Ok(Some(Self::row_to_proposal(&row)?)),
            None => Ok(None),
        }
    }

    async fn create_proposal(&self, proposal: &WorkflowProposal) -> WfResult<()> {
        let steps_json = serde_json::to_string(&proposal.steps).map_err(|e| {
            WorkflowError::DatabaseError(format!("Failed to serialize steps: {}", e))
        })?;
        let metadata_json = serde_json::to_string(&proposal.metadata).map_err(|e| {
            WorkflowError::DatabaseError(format!("Failed to serialize metadata: {}", e))
        })?;

        let cypher = "CREATE (p:WorkflowProposal { \
                          id: $id, title: $title, description: $description, \
                          status: $status, version: $version, steps_json: $steps_json, \
                          source_insight_id: $source_insight_id, submitted_by: $submitted_by, \
                          created_at: $created_at, updated_at: $updated_at, \
                          risk_score: $risk_score, expected_benefit: $expected_benefit, \
                          metadata_json: $metadata_json \
                      })";

        let q = Query::new(cypher.to_string())
            .param("id", proposal.id.clone())
            .param("title", proposal.title.clone())
            .param("description", proposal.description.clone())
            .param("status", Self::status_to_str(&proposal.status))
            .param("version", proposal.version as i64)
            .param("steps_json", steps_json)
            .param(
                "source_insight_id",
                proposal
                    .source_insight_id
                    .clone()
                    .unwrap_or_default(),
            )
            .param("submitted_by", proposal.submitted_by.clone())
            .param("created_at", proposal.created_at.clone())
            .param("updated_at", proposal.updated_at.clone())
            .param("risk_score", proposal.risk_score.unwrap_or(-1.0))
            .param(
                "expected_benefit",
                proposal
                    .expected_benefit
                    .clone()
                    .unwrap_or_default(),
            )
            .param("metadata_json", metadata_json);

        self.graph.run(q).await.map_err(|e| {
            WorkflowError::DatabaseError(format!("Failed to create proposal: {}", e))
        })?;

        info!("Created workflow proposal: {}", proposal.id);
        Ok(())
    }

    async fn update_proposal_status(
        &self,
        proposal_id: &str,
        status: WorkflowStatus,
    ) -> WfResult<()> {
        // Fetch current status for transition validation
        let current = self
            .get_proposal(proposal_id)
            .await?
            .ok_or_else(|| WorkflowError::NotFound(proposal_id.to_string()))?;

        Self::validate_transition(&current.status, &status)?;

        let now = chrono::Utc::now().to_rfc3339();
        let cypher = "MATCH (p:WorkflowProposal {id: $id}) \
                      SET p.status = $status, p.updated_at = $updated_at";

        let q = Query::new(cypher.to_string())
            .param("id", proposal_id.to_string())
            .param("status", Self::status_to_str(&status))
            .param("updated_at", now);

        self.graph.run(q).await.map_err(|e| {
            WorkflowError::DatabaseError(format!("Failed to update proposal status: {}", e))
        })?;

        info!(
            "Updated proposal {} status: {:?} -> {:?}",
            proposal_id, current.status, status
        );
        Ok(())
    }

    async fn get_patterns(&self, limit: usize) -> WfResult<Vec<WorkflowPattern>> {
        let cypher = format!(
            "MATCH (p:WorkflowPattern) \
             RETURN p.id AS id, p.title AS title, p.description AS description, \
                    p.active_version_id AS active_version_id, p.deployed_at AS deployed_at, \
                    p.deployed_by AS deployed_by, p.adoption_count AS adoption_count, \
                    p.rollback_target_id AS rollback_target_id \
             ORDER BY p.deployed_at DESC LIMIT {}",
            limit
        );

        let mut result = self
            .graph
            .execute(Query::new(cypher))
            .await
            .map_err(|e| {
                WorkflowError::DatabaseError(format!("Failed to list patterns: {}", e))
            })?;

        let mut patterns = Vec::new();
        while let Some(row) = result.next().await.map_err(|e| {
            WorkflowError::DatabaseError(format!("Failed to fetch row: {}", e))
        })? {
            patterns.push(Self::row_to_pattern(&row)?);
        }

        debug!("Listed {} workflow patterns", patterns.len());
        Ok(patterns)
    }

    async fn promote_to_pattern(
        &self,
        proposal_id: &str,
        pattern: &WorkflowPattern,
    ) -> WfResult<()> {
        // Verify proposal exists and is in Approved or Deployed status
        let proposal = self
            .get_proposal(proposal_id)
            .await?
            .ok_or_else(|| WorkflowError::NotFound(proposal_id.to_string()))?;

        if proposal.status != WorkflowStatus::Approved
            && proposal.status != WorkflowStatus::Deployed
        {
            return Err(WorkflowError::InvalidTransition {
                from: proposal.status,
                to: WorkflowStatus::Deployed,
            });
        }

        let now = chrono::Utc::now().to_rfc3339();

        // Create the pattern node and link it to the proposal in a single query
        let cypher = "MATCH (prop:WorkflowProposal {id: $proposal_id}) \
                      CREATE (pat:WorkflowPattern { \
                          id: $id, title: $title, description: $description, \
                          active_version_id: $active_version_id, deployed_at: $deployed_at, \
                          deployed_by: $deployed_by, adoption_count: $adoption_count, \
                          rollback_target_id: $rollback_target_id \
                      }) \
                      CREATE (prop)-[:PROMOTED_TO]->(pat) \
                      SET prop.status = 'deployed', prop.updated_at = $updated_at";

        let q = Query::new(cypher.to_string())
            .param("proposal_id", proposal_id.to_string())
            .param("id", pattern.id.clone())
            .param("title", pattern.title.clone())
            .param("description", pattern.description.clone())
            .param("active_version_id", pattern.active_version_id.clone())
            .param("deployed_at", pattern.deployed_at.clone())
            .param("deployed_by", pattern.deployed_by.clone())
            .param("adoption_count", pattern.adoption_count as i64)
            .param(
                "rollback_target_id",
                pattern
                    .rollback_target_id
                    .clone()
                    .unwrap_or_default(),
            )
            .param("updated_at", now);

        self.graph.run(q).await.map_err(|e| {
            WorkflowError::DatabaseError(format!("Failed to promote to pattern: {}", e))
        })?;

        info!(
            "Promoted proposal {} to pattern {}",
            proposal_id, pattern.id
        );
        Ok(())
    }
}
