//! HTTP entry for outbound user-interaction events (ADR-059 §3, Phase 3).
//!
//! Client posts interaction events here. Server broadcasts them on
//! `/wss/agent-events` to subscribed agentbox sessions. Events are
//! transient — never persisted to Neo4j.
//!
//! Endpoint: `POST /api/v1/agent-events/user-interaction`
//!
//! Phase 4 will add server-side ownership validation; Phase 5 fail-closed.

use crate::agent_events::UserInteractionEvent;
use crate::handlers::agent_events_ws_handler::{
    broadcast_user_interaction, AgentEventsBroadcaster,
};
use actix_web::{web, HttpResponse};
use log::debug;

pub async fn post_user_interaction(
    body: web::Json<UserInteractionEvent>,
    broadcaster: web::Data<AgentEventsBroadcaster>,
) -> HttpResponse {
    let evt = body.into_inner();
    debug!(
        "[user-interaction] kind={:?} target_node_id={} duration_ms={}",
        evt.kind, evt.target_node_id, evt.duration_ms
    );
    broadcast_user_interaction(broadcaster.get_ref(), evt);
    HttpResponse::Accepted().json(serde_json::json!({ "ok": true }))
}

pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::resource("/agent-events/user-interaction")
            .route(web::post().to(post_user_interaction)),
    );
}
