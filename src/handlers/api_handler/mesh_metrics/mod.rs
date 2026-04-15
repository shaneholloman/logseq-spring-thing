use actix_web::{web, Responder};
use log::info;
use serde::Deserialize;
use serde_json::json;

use crate::ok_json;
use crate::AppState;
use crate::services::kpi_computation_service::KpiComputationService;

#[derive(Debug, Deserialize)]
pub struct MetricsQuery {
    pub time_window: Option<String>,
}

/// GET /api/mesh-metrics
/// Returns current organisational KPI snapshots computed from live data.
pub async fn get_metrics(
    state: web::Data<AppState>,
    query: web::Query<MetricsQuery>,
) -> impl Responder {
    info!("GET /api/mesh-metrics");

    let time_window = query.time_window.as_deref().unwrap_or("7d");

    let kpi_service = KpiComputationService::new(
        state.broker_repository.clone(),
        state.workflow_repository.clone(),
    );
    let snapshot = kpi_service.compute(time_window).await;

    ok_json!(json!({
        "kpis": {
            "mesh_velocity": snapshot.mesh_velocity,
            "augmentation_ratio": snapshot.augmentation_ratio,
            "trust_variance": snapshot.trust_variance,
            "hitl_precision": snapshot.hitl_precision,
        },
        "computed_at": snapshot.computed_at,
        "time_window": snapshot.time_window,
    }))
}

/// Route configuration for mesh metrics.
pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/mesh-metrics")
            .wrap(crate::middleware::RequireAuth::authenticated())
            .route("", web::get().to(get_metrics)),
    );
}
