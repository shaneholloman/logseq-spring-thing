use actix_web::{web, HttpResponse, Result};
use crate::layout::types::*;
use crate::AppState;
use crate::ok_json;

pub async fn get_layout_modes(_data: web::Data<AppState>) -> Result<HttpResponse> {
    ok_json!(serde_json::json!({
        "current": "forceDirected",
        "available": ["forceDirected", "hierarchical", "radial", "spectral", "temporal", "clustered"],
        "transitioning": false
    }))
}

pub async fn set_layout_mode(
    _data: web::Data<AppState>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse> {
    let mode = body.get("mode").and_then(|m| m.as_str()).unwrap_or("forceDirected");
    let transition_ms = body.get("transitionMs").and_then(|t| t.as_u64()).unwrap_or(500);

    // TODO: Send ChangeLayoutMode message to ForceComputeActor
    // For now, store in AppState and trigger physics reheat

    ok_json!(serde_json::json!({
        "success": true,
        "mode": mode,
        "transitionMs": transition_ms
    }))
}

pub async fn get_layout_status(_data: web::Data<AppState>) -> Result<HttpResponse> {
    ok_json!(LayoutStatus {
        current_mode: LayoutMode::ForceDirected,
        transitioning: false,
        transition_progress: 1.0,
        iterations: 0,
        converged: false,
        kinetic_energy: 0.0,
        available_modes: vec![
            LayoutMode::ForceDirected,
            LayoutMode::Hierarchical,
            LayoutMode::Radial,
            LayoutMode::Spectral,
            LayoutMode::Temporal,
            LayoutMode::Clustered,
        ],
    })
}

pub async fn set_zones(
    _data: web::Data<AppState>,
    body: web::Json<Vec<ConstraintZone>>,
) -> Result<HttpResponse> {
    // TODO: Forward zones to ForceComputeActor
    ok_json!(serde_json::json!({
        "success": true,
        "zones": body.into_inner().len()
    }))
}

pub async fn get_zones(_data: web::Data<AppState>) -> Result<HttpResponse> {
    ok_json!(serde_json::json!({
        "zones": []
    }))
}

pub async fn reset_layout(_data: web::Data<AppState>) -> Result<HttpResponse> {
    // TODO: Send ResetPositions message to ForceComputeActor
    ok_json!(serde_json::json!({
        "success": true,
        "message": "Layout reset triggered"
    }))
}

pub fn configure_layout_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/layout")
            .route("/modes", web::get().to(get_layout_modes))
            .route("/mode", web::post().to(set_layout_mode))
            .route("/status", web::get().to(get_layout_status))
            .route("/zones", web::post().to(set_zones))
            .route("/zones", web::get().to(get_zones))
            .route("/reset", web::post().to(reset_layout))
    );
}
