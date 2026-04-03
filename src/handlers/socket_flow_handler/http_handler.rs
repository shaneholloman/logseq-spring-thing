use actix_web::{web, HttpRequest, HttpResponse};
use actix_web_actors::ws;
use log::{debug, error, info, warn};

use crate::app_state::AppState;
use crate::utils::validation::rate_limit::{create_rate_limit_response, extract_client_id};

use super::types::{PreReadSocketSettings, SocketFlowServer, WEBSOCKET_RATE_LIMITER};

/// HTTP upgrade handler for WebSocket connections at `/wss`.
pub async fn socket_flow_handler(
    req: HttpRequest,
    stream: web::Payload,
    app_state_data: web::Data<AppState>,
    pre_read_ws_settings: web::Data<PreReadSocketSettings>,
) -> Result<HttpResponse, actix_web::Error> {
    let client_ip = extract_client_id(&req);

    if !WEBSOCKET_RATE_LIMITER.is_allowed(&client_ip) {
        warn!("WebSocket rate limit exceeded for client: {}", client_ip);
        return create_rate_limit_response(&client_ip, &WEBSOCKET_RATE_LIMITER);
    }

    // SECURITY: Validate Origin header to prevent cross-site WebSocket hijacking
    if let Some(origin_header) = req.headers().get("Origin") {
        let origin = origin_header.to_str().unwrap_or("");
        let allowed_origins = std::env::var("CORS_ALLOWED_ORIGINS").unwrap_or_else(|_| {
            if std::env::var("ALLOW_INSECURE_DEFAULTS").is_ok() {
                "http://localhost:3000,http://localhost:3001,http://127.0.0.1:3000,http://localhost:5173".to_string()
            } else {
                "http://localhost:3000".to_string()
            }
        });

        // Check explicit allow-list first
        let is_allowed = allowed_origins
            .split(',')
            .map(|s| s.trim())
            .any(|allowed| allowed == origin);

        // Also allow same-host origins: if the Origin hostname matches the Host header,
        // this is a same-origin request proxied through nginx and should be permitted.
        // Nginx may strip the port from Host ($host), so we compare hostnames only.
        let is_same_host = if !is_allowed {
            if let Some(host_header) = req.headers().get("Host").or_else(|| req.headers().get("X-Forwarded-Host")) {
                let host = host_header.to_str().unwrap_or("");
                // Extract host from origin URL (strip scheme)
                let origin_host = origin
                    .strip_prefix("http://")
                    .or_else(|| origin.strip_prefix("https://"))
                    .unwrap_or("");
                // Strip port from both for comparison (nginx $host strips port)
                let host_no_port = host.split(':').next().unwrap_or("");
                let origin_no_port = origin_host.split(':').next().unwrap_or("");
                !host_no_port.is_empty() && !origin_no_port.is_empty() && origin_no_port == host_no_port
            } else {
                false
            }
        } else {
            false
        };

        if !is_allowed && !is_same_host {
            warn!(
                "WebSocket connection rejected - invalid origin: {} (allowed: {})",
                origin, allowed_origins
            );
            return Ok(HttpResponse::Forbidden()
                .body(format!("Origin '{}' not allowed for WebSocket connections", origin)));
        }
    } else if std::env::var("ALLOW_INSECURE_DEFAULTS").is_err() {
        warn!(
            "WebSocket connection rejected - missing Origin header from {}",
            client_ip
        );
        return Ok(
            HttpResponse::BadRequest().body("Origin header required for WebSocket connections")
        );
    }

    // SECURITY: WebSocket token validation at upgrade time.
    {
        let token = req
            .headers()
            .get("Authorization")
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .map(|s| s.to_string())
            .or_else(|| {
                let query = req.query_string();
                url::form_urlencoded::parse(query.as_bytes())
                    .find(|(k, _)| k == "token")
                    .map(|(_, v)| v.to_string())
            });

        if token.as_deref().unwrap_or("").is_empty() {
            if std::env::var("ALLOW_INSECURE_DEFAULTS").is_err() {
                warn!(
                    "SECURITY: Rejecting unauthenticated WebSocket connection on /wss from {}",
                    client_ip
                );
                return Ok(
                    HttpResponse::Unauthorized()
                        .body("Authentication required for WebSocket connections")
                );
            }
            warn!(
                "SECURITY: Unauthenticated WebSocket connection on /wss from {} (ALLOW_INSECURE_DEFAULTS set)",
                client_ip
            );
        }
    }

    let app_state_arc = app_state_data.into_inner();

    let client_manager_addr = app_state_arc.client_manager_addr.clone();

    use crate::actors::messages::GetSettingByPath;
    let settings_addr = app_state_arc.settings_addr.clone();

    let debug_enabled = match settings_addr
        .send(GetSettingByPath {
            path: "system.debug.enabled".to_string(),
        })
        .await
    {
        Ok(Ok(value)) => value.as_bool().unwrap_or(false),
        _ => false,
    };
    let debug_websocket = match settings_addr
        .send(GetSettingByPath {
            path: "system.debug.enable_websocket_debug".to_string(),
        })
        .await
    {
        Ok(Ok(value)) => value.as_bool().unwrap_or(false),
        _ => false,
    };
    let should_debug = debug_enabled && debug_websocket;

    if should_debug {
        debug!("WebSocket connection attempt from {:?}", req.peer_addr());
    }

    if !req.headers().contains_key("Upgrade") {
        return Ok(HttpResponse::BadRequest().body("WebSocket upgrade required"));
    }

    let is_reconnection = req
        .headers()
        .get("X-Client-Session")
        .and_then(|h| h.to_str().ok())
        .is_some();

    // Extract token from query string for authentication
    let token_from_qs = req.query_string().split('&').find_map(|param| {
        let parts: Vec<&str> = param.split('=').collect();
        if parts.len() == 2 && parts[0] == "token" {
            Some(parts[1].to_string())
        } else {
            None
        }
    });

    let mut ws_server = SocketFlowServer::new(
        app_state_arc.clone(),
        pre_read_ws_settings.get_ref().clone(),
        client_manager_addr,
        client_ip.clone(),
    );

    ws_server.is_reconnection = is_reconnection;

    // Store HTTP-equivalent URL for NIP-98 WS auth validation
    {
        let conn_info = req.connection_info();
        ws_server.connection_url = format!(
            "{}://{}{}",
            conn_info.scheme(),
            conn_info.host(),
            req.uri()
                .path_and_query()
                .map(|pq| pq.as_str())
                .unwrap_or("/wss")
        );
    }

    // Try to authenticate from query string token
    if let Some(token) = token_from_qs {
        if let Some(ref nostr_service) = app_state_arc.nostr_service {
            if let Some(user) = nostr_service.get_session(&token).await {
                ws_server.pubkey = Some(user.pubkey.clone());
                ws_server.is_power_user = user.is_power_user;
                info!(
                    "Pre-authenticated WebSocket client via query string: pubkey={}",
                    user.pubkey
                );
            }
        }
    }

    // Restore .protocols() for WebSocket subprotocol negotiation.
    // Even though permessage-deflate is technically an extension not a subprotocol,
    // removing this broke WebSocket connections through cloudflared/nginx proxy chains
    // that expect the server to echo back the Sec-WebSocket-Protocol header.
    match ws::WsResponseBuilder::new(ws_server, &req, stream)
        .protocols(&["permessage-deflate"])
        .start()
    {
        Ok(response) => {
            info!(
                "[WebSocket] Client {} connected successfully",
                client_ip
            );
            Ok(response)
        }
        Err(e) => {
            error!(
                "[WebSocket] Failed to start WebSocket for client {}: {}",
                client_ip, e
            );
            Err(e)
        }
    }
}
