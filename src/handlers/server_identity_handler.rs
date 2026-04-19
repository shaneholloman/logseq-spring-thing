//! `GET /api/server/identity` — advertises the server's Nostr public identity.
//!
//! The response lists:
//!   * `pubkey_hex`       — 64-char lowercase hex
//!   * `pubkey_npub`      — bech32 `npub1…`
//!   * `supported_kinds`  — event kinds the server will sign (30023, 30100, 30200, 30300)
//!   * `relay_urls`       — relays to which the server publishes
//!
//! No authentication is required — this is public identity info. The private
//! key is **never** touched or exposed.

use std::sync::Arc;

use actix_web::{get, web, HttpResponse, Responder};
use serde::Serialize;

use crate::services::server_identity::{ServerIdentity, SUPPORTED_KINDS};

/// JSON response shape for `GET /api/server/identity`.
#[derive(Debug, Serialize)]
pub struct ServerIdentityResponse {
    pub pubkey_hex: String,
    pub pubkey_npub: String,
    pub supported_kinds: Vec<u16>,
    pub relay_urls: Vec<String>,
}

#[get("/identity")]
pub async fn get_server_identity(
    identity: web::Data<Arc<ServerIdentity>>,
) -> impl Responder {
    HttpResponse::Ok().json(ServerIdentityResponse {
        pubkey_hex: identity.pubkey_hex(),
        pubkey_npub: identity.pubkey_npub(),
        supported_kinds: SUPPORTED_KINDS.to_vec(),
        relay_urls: identity.relay_urls().to_vec(),
    })
}

/// Register `GET /api/server/identity` under a parent scope.
///
/// Wired in `main.rs` as:
/// ```ignore
/// .service(web::scope("/api/server").configure(server_identity_handler::configure_routes))
/// ```
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(get_server_identity);
}
