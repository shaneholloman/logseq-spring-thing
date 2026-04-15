//! Policy evaluation REST handler (ADR-045 / P4).
//!
//! Exposes `POST /api/policy/evaluate` for server-side policy decisions.
//! Requires Admin enterprise role.

use actix_web::{web, HttpRequest, Responder};
use log::info;
use serde_json::json;

use crate::events::enterprise_events::{PolicyEvaluatedEvent, emit_enterprise_event};
use crate::middleware::enterprise_auth::require_role;
use crate::models::enterprise::{EnterpriseRole, PolicyContext, PolicyOutcome};
use crate::{error_json, ok_json};
use crate::AppState;

/// POST /api/policy/evaluate
///
/// Accepts a [`PolicyContext`] and returns the aggregate decision plus
/// per-rule evaluations. Requires Admin role.
pub async fn evaluate_policy(
    req: HttpRequest,
    state: web::Data<AppState>,
    body: web::Json<PolicyContext>,
) -> impl Responder {
    if let Err(resp) = require_role(&req, EnterpriseRole::Admin) {
        return resp;
    }
    info!("POST /api/policy/evaluate");

    match state.policy_engine.evaluate(&body).await {
        Ok(evaluations) => {
            let decision = if evaluations
                .iter()
                .any(|e| e.outcome == PolicyOutcome::Deny)
            {
                PolicyOutcome::Deny
            } else if evaluations
                .iter()
                .any(|e| e.outcome == PolicyOutcome::Escalate)
            {
                PolicyOutcome::Escalate
            } else {
                PolicyOutcome::Allow
            };

            // Emit audit event
            emit_enterprise_event(&PolicyEvaluatedEvent {
                evaluation_id: format!("eval-{}", uuid::Uuid::new_v4()),
                context_action: format!("{:?}", body.action),
                outcome: decision.clone(),
                rule_count: evaluations.len(),
                actor_id: body.actor_id.clone(),
                timestamp: chrono::Utc::now(),
            });

            let decision_str = match &decision {
                PolicyOutcome::Deny => "deny",
                PolicyOutcome::Escalate => "escalate",
                PolicyOutcome::Allow => "allow",
                PolicyOutcome::Warn => "warn",
            };

            ok_json!(json!({
                "outcome": decision_str,
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
