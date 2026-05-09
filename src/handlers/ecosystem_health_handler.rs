use actix_web::{web, HttpResponse, Result};
use chrono::Utc;
use futures::future::join_all;
use log::warn;
use serde::Serialize;
use std::env;
use std::time::Instant;
use tokio::time::Duration;

/// Per-substrate health check configuration, read from environment variables.
struct SubstrateHealthConfig {
    name: String,
    url: String,
    timeout_ms: u64,
}

#[derive(Serialize)]
struct SubstrateHealth {
    name: String,
    status: String,
    latency_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    last_check: String,
}

#[derive(Serialize)]
struct EcosystemHealthResponse {
    substrates: Vec<SubstrateHealth>,
    overall: String,
    checked_at: String,
}

fn load_substrate_configs() -> Vec<SubstrateHealthConfig> {
    let default_timeout: u64 = 5000;

    vec![
        SubstrateHealthConfig {
            name: "visionclaw".to_string(),
            url: env::var("ECOSYSTEM_HEALTH_VISIONCLAW_URL")
                .unwrap_or_else(|_| "http://localhost:4000/api/health".to_string()),
            timeout_ms: default_timeout,
        },
        SubstrateHealthConfig {
            name: "forum".to_string(),
            url: env::var("ECOSYSTEM_HEALTH_FORUM_URL")
                .unwrap_or_else(|_| "https://dreamlab-nostr-relay.solitary-paper-764d.workers.dev/".to_string()),
            timeout_ms: default_timeout,
        },
        SubstrateHealthConfig {
            name: "agentbox".to_string(),
            url: env::var("ECOSYSTEM_HEALTH_AGENTBOX_URL")
                .unwrap_or_else(|_| "http://host.docker.internal:9090/health".to_string()),
            timeout_ms: default_timeout,
        },
        SubstrateHealthConfig {
            name: "solidpod".to_string(),
            url: env::var("ECOSYSTEM_HEALTH_SOLIDPOD_URL")
                .unwrap_or_else(|_| "http://host.docker.internal:8484/.well-known/solid".to_string()),
            timeout_ms: default_timeout,
        },
    ]
}

async fn check_substrate(config: &SubstrateHealthConfig) -> SubstrateHealth {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(config.timeout_ms))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    let start = Instant::now();
    let check_time = Utc::now().to_rfc3339();

    match client.get(&config.url).send().await {
        Ok(resp) => {
            let latency = start.elapsed().as_millis() as u64;
            if resp.status().is_success() {
                SubstrateHealth {
                    name: config.name.clone(),
                    status: "healthy".to_string(),
                    latency_ms: Some(latency),
                    error: None,
                    last_check: check_time,
                }
            } else {
                SubstrateHealth {
                    name: config.name.clone(),
                    status: "unhealthy".to_string(),
                    latency_ms: Some(latency),
                    error: Some(format!("HTTP {}", resp.status().as_u16())),
                    last_check: check_time,
                }
            }
        }
        Err(e) => {
            let latency = start.elapsed().as_millis() as u64;
            let error_msg = if e.is_timeout() {
                "timeout".to_string()
            } else if e.is_connect() {
                "connection refused".to_string()
            } else {
                e.to_string()
            };
            warn!(
                "Ecosystem health check failed for {}: {}",
                config.name, error_msg
            );
            SubstrateHealth {
                name: config.name.clone(),
                status: "unreachable".to_string(),
                latency_ms: Some(latency),
                error: Some(error_msg),
                last_check: check_time,
            }
        }
    }
}

pub async fn ecosystem_health() -> Result<HttpResponse> {
    let configs = load_substrate_configs();

    let futures: Vec<_> = configs.iter().map(|c| check_substrate(c)).collect();
    let substrates = join_all(futures).await;

    let healthy_count = substrates
        .iter()
        .filter(|s| s.status == "healthy")
        .count();
    let total = substrates.len();

    let overall = if healthy_count == total {
        "healthy"
    } else if healthy_count == 0 {
        "unhealthy"
    } else {
        "degraded"
    }
    .to_string();

    let response = EcosystemHealthResponse {
        substrates,
        overall,
        checked_at: Utc::now().to_rfc3339(),
    };

    Ok(HttpResponse::Ok().json(response))
}

pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/ecosystem").route("/health", web::get().to(ecosystem_health)),
    );
}
