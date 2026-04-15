//! Policy evaluation REST handler (ADR-045 / P4).
//!
//! Exposes `POST /api/policy/evaluate` for server-side policy decisions.

use actix_web::{web, Responder};
use log::info;
use serde_json::json;

use crate::models::enterprise::{PolicyContext, PolicyOutcome};
use crate::{error_json, ok_json};
use crate::AppState;

/// POST /api/policy/evaluate
///
/// Accepts a [`PolicyContext`] and returns the aggregate decision plus
/// per-rule evaluations.
pub async fn evaluate_policy(
    state: web::Data<AppState>,
    body: web::Json<PolicyContext>,
) -> impl Responder {
    info!("POST /api/policy/evaluate");

    match state.policy_engine.evaluate(&body).await {
        Ok(evaluations) => {
            let decision = if evaluations
                .iter()
                .any(|e| e.outcome == PolicyOutcome::Deny)
            {
                "deny"
            } else if evaluations
                .iter()
                .any(|e| e.outcome == PolicyOutcome::Escalate)
            {
                "escalate"
            } else {
                "allow"
            };

            ok_json!(json!({
                "outcome": decision,
                "evaluations": evaluations,
            }))
        }
        Err(e) => {
            error_json!(format!("Policy evaluation failed: {}", e))
        }
    }
}

/// Route configuration for the policy engine.
pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/policy")
            .wrap(crate::middleware::RequireAuth::authenticated())
            .route("/evaluate", web::post().to(evaluate_policy)),
    );
}
