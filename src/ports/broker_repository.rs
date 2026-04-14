// src/ports/broker_repository.rs
//! Broker Repository Port
//!
//! Manages broker case storage and retrieval for the enterprise decision-support layer.
//! This port provides case lifecycle management and decision recording.

use async_trait::async_trait;

use crate::models::enterprise::{BrokerCase, BrokerDecision, CaseStatus};

pub type Result<T> = std::result::Result<T, BrokerError>;

#[derive(Debug, thiserror::Error)]
pub enum BrokerError {
    #[error("Case not found: {0}")]
    NotFound(String),

    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("Validation error: {0}")]
    ValidationError(String),
}

/// Port for broker case storage and retrieval
#[async_trait]
pub trait BrokerRepository: Send + Sync {
    /// Get all open cases, ordered by priority
    async fn list_cases(
        &self,
        status: Option<CaseStatus>,
        limit: usize,
    ) -> Result<Vec<BrokerCase>>;

    /// Get a single case by ID
    async fn get_case(&self, case_id: &str) -> Result<Option<BrokerCase>>;

    /// Create a new case
    async fn create_case(&self, case: &BrokerCase) -> Result<()>;

    /// Update case status
    async fn update_case_status(&self, case_id: &str, status: CaseStatus) -> Result<()>;

    /// Record a decision
    async fn record_decision(&self, decision: &BrokerDecision) -> Result<()>;

    /// Get decisions for a case
    async fn get_decisions(&self, case_id: &str) -> Result<Vec<BrokerDecision>>;
}
