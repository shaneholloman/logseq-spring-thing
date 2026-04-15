use actix_web::{web, Responder};
use log::info;
use serde_json::json;

use crate::ok_json;
use crate::AppState;

/// GET /api/mesh-metrics
/// Returns current organisational KPI snapshots.
pub async fn get_metrics(_state: web::Data<AppState>) -> impl Responder {
    info!("GET /api/mesh-metrics");
    ok_json!(json!({
        "kpis": {
            "mesh_velocity": {
                "value": null,
                "unit": "hours",
                "description": "Time from first discovery signal to approved reusable workflow",
                "status": "not_computed"
            },
            "augmentation_ratio": {
                "value": null,
                "unit": "ratio",
                "description": "Proportion of decision volume resolved without escalation",
                "status": "not_computed"
            },
            "trust_variance": {
                "value": null,
                "unit": "sigma",
                "description": "Rolling variance in decision quality across workflows",
                "status": "not_computed"
            },
            "hitl_precision": {
                "value": null,
                "unit": "percentage",
                "description": "Percentage of escalations where human intervention changed outcome",
                "status": "not_computed"
            }
        },
        "computed_at": null,
        "message": "KPI computation not yet active. Requires broker decisions and workflow data."
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
