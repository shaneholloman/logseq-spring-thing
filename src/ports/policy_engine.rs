// src/ports/policy_engine.rs
//! Policy Engine Port
//!
//! Evaluates policies against a given context to produce allow/deny/escalate decisions.
//! This port abstracts the specific policy evaluation implementation.

use async_trait::async_trait;

use crate::models::enterprise::{PolicyContext, PolicyEvaluation, PolicyOutcome};

pub type Result<T> = std::result::Result<T, PolicyError>;

#[derive(Debug, thiserror::Error)]
pub enum PolicyError {
    #[error("Policy evaluation failed: {0}")]
    EvaluationFailed(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),
}

/// Port for policy evaluation
#[async_trait]
pub trait PolicyEngine: Send + Sync {
    /// Evaluate all matching policies for the given context
    async fn evaluate(&self, context: &PolicyContext) -> Result<Vec<PolicyEvaluation>>;

    /// Get the aggregate decision (first Deny wins, then first Escalate, else Allow)
    async fn decide(&self, context: &PolicyContext) -> Result<PolicyOutcome>;
}
