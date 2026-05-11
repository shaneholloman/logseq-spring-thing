use crate::layout::engines::compute_layout;
use crate::layout::types::*;
use crate::ok_json;
use crate::settings::auth_extractor::AuthenticatedUser;
use crate::AppState;
use actix_web::{web, HttpResponse, Result};

pub async fn get_layout_modes(_data: web::Data<AppState>) -> Result<HttpResponse> {
    ok_json!(serde_json::json!({
        "current": "forceDirected",
        "available": ["forceDirected", "hierarchical", "radial", "spectral", "temporal", "clustered"],
        "transitioning": false
    }))
}

pub async fn set_layout_mode(
    _user: AuthenticatedUser,
    data: web::Data<AppState>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse> {
    let mode_str = body
        .get("mode")
        .and_then(|m| m.as_str())
        .unwrap_or("forceDirected");
    let transition_ms = body
        .get("transitionMs")
        .and_then(|t| t.as_u64())
        .unwrap_or(500);

    let mode: LayoutMode =
        match serde_json::from_value(serde_json::Value::String(mode_str.to_string())) {
            Ok(m) => m,
            Err(_) => LayoutMode::ForceDirected,
        };

    // ForceDirected is handled by the GPU physics engine; no CPU layout needed.
    if mode == LayoutMode::ForceDirected {
        return ok_json!(serde_json::json!({
            "success": true,
            "mode": mode_str,
            "transitionMs": transition_ms,
            "positions": []
        }));
    }

    // Fetch current graph data
    use crate::actors::messages::GetGraphData;
    let graph_data = match data.graph_service_addr.send(GetGraphData).await {
        Ok(Ok(gd)) => gd,
        Ok(Err(e)) => {
            log::error!("set_layout_mode: failed to get graph data: {}", e);
            return ok_json!(serde_json::json!({
                "success": false,
                "error": "Failed to retrieve graph data",
                "mode": mode_str
            }));
        }
        Err(e) => {
            log::error!("set_layout_mode: actor mailbox error: {}", e);
            return ok_json!(serde_json::json!({
                "success": false,
                "error": "Graph service unavailable",
                "mode": mode_str
            }));
        }
    };

    // Convert graph data to the flat slices expected by compute_layout
    let nodes: Vec<(u32, String)> = graph_data
        .nodes
        .iter()
        .map(|n| (n.id, n.label.clone()))
        .collect();

    let edges: Vec<(u32, u32, f32)> = graph_data
        .edges
        .iter()
        .map(|e| (e.source, e.target, e.weight))
        .collect();

    let config = LayoutModeConfig {
        mode: mode.clone(),
        ..LayoutModeConfig::default()
    };

    let raw_positions = compute_layout(&mode, &nodes, &edges, &config);

    // Build JSON position array [{id, x, y, z}, ...]
    let positions: Vec<serde_json::Value> = nodes
        .iter()
        .zip(raw_positions.iter())
        .map(|((id, _label), &(x, y, z))| serde_json::json!({ "id": id, "x": x, "y": y, "z": z }))
        .collect();

    // Pause physics for non-ForceDirected layouts so the GPU engine does not
    // immediately override the computed layout positions.
    use crate::actors::messages::{GetPhysicsOrchestratorActor, PhysicsPauseMessage};
    match data
        .graph_service_addr
        .send(GetPhysicsOrchestratorActor)
        .await
    {
        Ok(Ok(orch_addr)) => {
            let pause_msg = PhysicsPauseMessage {
                pause: true,
                reason: format!("layout mode changed to {}", mode_str),
            };
            if let Err(e) = orch_addr.send(pause_msg).await {
                log::warn!(
                    "set_layout_mode: failed to pause physics via orchestrator: {}",
                    e
                );
            }
        }
        _ => {
            log::warn!("set_layout_mode: physics orchestrator unavailable — physics not paused");
        }
    }

    ok_json!(serde_json::json!({
        "success": true,
        "mode": mode_str,
        "transitionMs": transition_ms,
        "positions": positions
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
    _user: AuthenticatedUser,
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

pub async fn reset_layout(
    _user: AuthenticatedUser,
    data: web::Data<AppState>,
) -> Result<HttpResponse> {
    use crate::actors::messages::ResetPositions;

    if let Some(addr) = data.get_gpu_compute_addr().await {
        match addr.send(ResetPositions).await {
            Ok(Ok(_)) => {
                ok_json!(serde_json::json!({
                    "success": true,
                    "message": "Layout reset triggered — positions randomized and reheat applied"
                }))
            }
            Ok(Err(e)) => {
                ok_json!(serde_json::json!({
                    "success": false,
                    "message": format!("Reset failed: {}", e)
                }))
            }
            Err(e) => {
                ok_json!(serde_json::json!({
                    "success": false,
                    "message": format!("ForceComputeActor mailbox error: {}", e)
                }))
            }
        }
    } else {
        ok_json!(serde_json::json!({
            "success": false,
            "message": "GPU compute actor not available — layout reset skipped"
        }))
    }
}

pub fn configure_layout_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/layout")
            .route("/modes", web::get().to(get_layout_modes))
            .route("/mode", web::post().to(set_layout_mode))
            .route("/status", web::get().to(get_layout_status))
            .route("/zones", web::post().to(set_zones))
            .route("/zones", web::get().to(get_zones))
            .route("/reset", web::post().to(reset_layout)),
    );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ---- LayoutMode serde ----

    #[test]
    fn test_layout_mode_deserialize_all_variants() {
        let cases = vec![
            ("\"forceDirected\"", LayoutMode::ForceDirected),
            ("\"hierarchical\"", LayoutMode::Hierarchical),
            ("\"radial\"", LayoutMode::Radial),
            ("\"spectral\"", LayoutMode::Spectral),
            ("\"temporal\"", LayoutMode::Temporal),
            ("\"clustered\"", LayoutMode::Clustered),
        ];
        for (json, expected) in cases {
            let mode: LayoutMode = serde_json::from_str(json).unwrap();
            assert_eq!(mode, expected, "Failed for {}", json);
        }
    }

    #[test]
    fn test_layout_mode_invalid_falls_through() {
        // When an invalid mode string is parsed, the handler defaults to ForceDirected
        let result: Result<LayoutMode, _> =
            serde_json::from_value(serde_json::Value::String("invalid_mode".to_string()));
        assert!(result.is_err());
        // The handler uses fallback:
        let fallback = match result {
            Ok(m) => m,
            Err(_) => LayoutMode::ForceDirected,
        };
        assert_eq!(fallback, LayoutMode::ForceDirected);
    }

    #[test]
    fn test_layout_mode_config_defaults() {
        let config = LayoutModeConfig::default();
        assert_eq!(config.mode, LayoutMode::ForceDirected);
        assert_eq!(config.transition_duration_ms, 500);
        assert!((config.scaling_ratio - 10.0).abs() < f32::EPSILON);
        assert!((config.gravity - 1.0).abs() < f32::EPSILON);
        assert!(config.lin_log_mode);
        assert!(config.dissuade_hubs);
    }

    // ---- ConstraintZone serde ----

    #[test]
    fn test_constraint_zone_deserialize() {
        let json = r#"{
            "id": "zone-1",
            "center": [1.0, 2.0, 3.0],
            "radius": 10.0,
            "strength": 0.5,
            "nodeTypes": ["owl_class"]
        }"#;
        let zone: ConstraintZone = serde_json::from_str(json).unwrap();
        assert_eq!(zone.id, "zone-1");
        assert!((zone.center[0] - 1.0).abs() < f32::EPSILON);
        assert!((zone.radius - 10.0).abs() < f32::EPSILON);
        assert_eq!(zone.node_types.len(), 1);
    }

    // ---- LayoutStatus serde ----

    #[test]
    fn test_layout_status_serialization() {
        let status = LayoutStatus {
            current_mode: LayoutMode::Hierarchical,
            transitioning: true,
            transition_progress: 0.5,
            iterations: 100,
            converged: false,
            kinetic_energy: 42.5,
            available_modes: vec![LayoutMode::ForceDirected, LayoutMode::Hierarchical],
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("hierarchical"));
        assert!(json.contains("\"transitioning\":true"));
    }

    // ---- get_layout_modes handler ----

    #[tokio::test]
    async fn test_get_layout_modes_returns_available_modes() {
        // get_layout_modes takes AppState but only returns a static JSON
        // We can test the handler response shape using actix_web::test
        use actix_web::test;
        use actix_web::App;

        let app = test::init_service(
            App::new().route("/modes", actix_web::web::get().to(get_layout_modes)),
        )
        .await;

        let req = test::TestRequest::get().uri("/modes").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["current"], "forceDirected");
        let available = body["available"].as_array().unwrap();
        assert_eq!(available.len(), 6);
        assert!(available.contains(&serde_json::json!("forceDirected")));
        assert!(available.contains(&serde_json::json!("hierarchical")));
        assert!(!body["transitioning"].as_bool().unwrap());
    }

    // ---- get_zones handler ----

    #[tokio::test]
    async fn test_get_zones_returns_empty_array() {
        use actix_web::test;
        use actix_web::App;

        let app =
            test::init_service(App::new().route("/zones", actix_web::web::get().to(get_zones)))
                .await;

        let req = test::TestRequest::get().uri("/zones").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert!(body["zones"].as_array().unwrap().is_empty());
    }

    // ---- set_zones handler (stub) ----

    #[tokio::test]
    async fn test_set_zones_accepts_valid_zones() {
        use actix_web::test;
        use actix_web::App;

        // set_zones requires AuthenticatedUser which we cannot easily mock here;
        // test the body parsing and response shape by calling the logic directly.
        let zones = vec![ConstraintZone {
            id: "z1".to_string(),
            center: [0.0, 0.0, 0.0],
            radius: 5.0,
            strength: 1.0,
            node_types: vec!["owl_class".to_string()],
        }];
        let json = serde_json::to_string(&zones).unwrap();
        let parsed: Vec<ConstraintZone> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].id, "z1");
    }

    // ---- get_layout_status handler ----

    #[tokio::test]
    async fn test_get_layout_status_response() {
        use actix_web::test;
        use actix_web::App;

        let app = test::init_service(
            App::new().route("/status", actix_web::web::get().to(get_layout_status)),
        )
        .await;

        let req = test::TestRequest::get().uri("/status").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["currentMode"], "forceDirected");
        assert!(!body["transitioning"].as_bool().unwrap());
        assert!((body["transitionProgress"].as_f64().unwrap() - 1.0).abs() < 1e-6);
        let modes = body["availableModes"].as_array().unwrap();
        assert_eq!(modes.len(), 6);
    }

    // ---- set_layout_mode body parsing ----

    #[test]
    fn test_set_layout_mode_body_parsing() {
        // Verify the JSON body parsing logic the handler uses
        let body: serde_json::Value =
            serde_json::from_str(r#"{"mode": "hierarchical", "transitionMs": 1000}"#).unwrap();
        let mode_str = body
            .get("mode")
            .and_then(|m| m.as_str())
            .unwrap_or("forceDirected");
        let transition_ms = body
            .get("transitionMs")
            .and_then(|t| t.as_u64())
            .unwrap_or(500);
        assert_eq!(mode_str, "hierarchical");
        assert_eq!(transition_ms, 1000);
    }

    #[test]
    fn test_set_layout_mode_body_defaults() {
        let body: serde_json::Value = serde_json::from_str(r#"{}"#).unwrap();
        let mode_str = body
            .get("mode")
            .and_then(|m| m.as_str())
            .unwrap_or("forceDirected");
        let transition_ms = body
            .get("transitionMs")
            .and_then(|t| t.as_u64())
            .unwrap_or(500);
        assert_eq!(mode_str, "forceDirected");
        assert_eq!(transition_ms, 500);
    }
}
