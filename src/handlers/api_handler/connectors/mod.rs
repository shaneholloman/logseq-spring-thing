//! Connector CRUD REST handlers (ADR-044).
//!
//! Provides in-memory connector management for discovery sources.
//! A Neo4j-backed adapter can replace the in-memory store later.

use actix_web::{web, Responder};
use log::info;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::RwLock;

use crate::{ok_json, bad_request, not_found, created_json};
use crate::AppState;

// In-memory store for connectors (Neo4j adapter can follow later)
lazy_static::lazy_static! {
    static ref CONNECTORS: RwLock<Vec<ConnectorEntry>> = RwLock::new(Vec::new());
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConnectorEntry {
    id: String,
    connector_type: String,
    name: String,
    status: String,
    config: serde_json::Value,
    last_sync: Option<String>,
    created_at: String,
    created_by: String,
    signal_count: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateConnectorRequest {
    pub connector_type: Option<String>,
    pub name: String,
    pub config: Option<serde_json::Value>,
}

/// GET /api/connectors
pub async fn list_connectors(_state: web::Data<AppState>) -> impl Responder {
    info!("GET /api/connectors");
    let connectors = CONNECTORS.read().unwrap();
    ok_json!(json!({
        "connectors": *connectors,
        "total": connectors.len()
    }))
}

/// POST /api/connectors
pub async fn create_connector(
    _state: web::Data<AppState>,
    body: web::Json<CreateConnectorRequest>,
) -> impl Responder {
    info!("POST /api/connectors");

    if body.name.trim().is_empty() {
        return bad_request!("name is required");
    }
    if body.name.len() > 200 {
        return bad_request!("name exceeds 200 characters");
    }

    let entry = ConnectorEntry {
        id: format!("conn-{}", uuid::Uuid::new_v4()),
        connector_type: body.connector_type.clone().unwrap_or_else(|| "github".to_string()),
        name: body.name.clone(),
        status: "configuring".to_string(),
        config: body.config.clone().unwrap_or(json!({})),
        last_sync: None,
        created_at: chrono::Utc::now().to_rfc3339(),
        created_by: "authenticated_user".to_string(),
        signal_count: 0,
    };

    let id = entry.id.clone();
    CONNECTORS.write().unwrap().push(entry);

    created_json!(json!({
        "id": id,
        "status": "configuring",
        "message": "Connector created. Initial sync will begin shortly."
    }))
}

/// GET /api/connectors/{id}
pub async fn get_connector(
    _state: web::Data<AppState>,
    connector_id: web::Path<String>,
) -> impl Responder {
    info!("GET /api/connectors/{}", connector_id);
    let connectors = CONNECTORS.read().unwrap();
    match connectors.iter().find(|c| c.id == *connector_id) {
        Some(conn) => ok_json!(json!(conn)),
        None => not_found!(format!("Connector {} not found", connector_id)),
    }
}

/// DELETE /api/connectors/{id}
pub async fn delete_connector(
    _state: web::Data<AppState>,
    connector_id: web::Path<String>,
) -> impl Responder {
    info!("DELETE /api/connectors/{}", connector_id);
    let mut connectors = CONNECTORS.write().unwrap();
    let len_before = connectors.len();
    connectors.retain(|c| c.id != *connector_id);
    if connectors.len() < len_before {
        ok_json!(json!({"deleted": true}))
    } else {
        not_found!(format!("Connector {} not found", connector_id))
    }
}

/// Route configuration for discovery connectors.
pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/connectors")
            .wrap(crate::middleware::RequireAuth::authenticated())
            .route("", web::get().to(list_connectors))
            .route("", web::post().to(create_connector))
            .route("/{id}", web::get().to(get_connector))
            .route("/{id}", web::delete().to(delete_connector)),
    );
}
