// src/ports/workflow_repository.rs
//! Workflow Repository Port
//!
//! Manages workflow proposal storage and pattern promotion.
//! This port provides the lifecycle for proposals that can be promoted to reusable patterns.

use async_trait::async_trait;

use crate::models::enterprise::{WorkflowPattern, WorkflowProposal, WorkflowStatus};

pub type Result<T> = std::result::Result<T, WorkflowError>;

#[derive(Debug, thiserror::Error)]
pub enum WorkflowError {
    #[error("Proposal not found: {0}")]
    NotFound(String),

    #[error("Invalid status transition from {from:?} to {to:?}")]
    InvalidTransition {
        from: WorkflowStatus,
        to: WorkflowStatus,
    },

    #[error("Database error: {0}")]
    DatabaseError(String),
}

/// Port for workflow proposal storage
#[async_trait]
pub trait WorkflowRepository: Send + Sync {
    /// List proposals with optional status filter
    async fn list_proposals(
        &self,
        status: Option<WorkflowStatus>,
        limit: usize,
    ) -> Result<Vec<WorkflowProposal>>;

    /// Get a single proposal by ID
    async fn get_proposal(&self, proposal_id: &str) -> Result<Option<WorkflowProposal>>;

    /// Create a new proposal
    async fn create_proposal(&self, proposal: &WorkflowProposal) -> Result<()>;

    /// Update proposal status
    async fn update_proposal_status(
        &self,
        proposal_id: &str,
        status: WorkflowStatus,
    ) -> Result<()>;

    /// Get all promoted workflow patterns
    async fn get_patterns(&self, limit: usize) -> Result<Vec<WorkflowPattern>>;

    /// Promote a proposal to a reusable pattern
    async fn promote_to_pattern(
        &self,
        proposal_id: &str,
        pattern: &WorkflowPattern,
    ) -> Result<()>;
}
