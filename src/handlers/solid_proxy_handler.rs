//! Solid Proxy Handler
//!
//! Proxies requests to JavaScript Solid Server (JSS) with NIP-98 authentication.
//! Routes: /solid/* -> JSS
//!
//! Features:
//! - NIP-98 authentication header generation and forwarding
//! - User identity preservation for Solid ACL enforcement
//! - Content negotiation passthrough (JSON-LD, Turtle)
//! - LDP CRUD operations (GET, PUT, POST, DELETE, PATCH, HEAD)
//! - Pod management endpoints
//! - WebSocket upgrade for solid-0.1 notifications
//!
//! Security Architecture:
//! - User's NIP-98 token is verified locally, then forwarded to JSS
//! - Server signing only used when user auth is unavailable (anonymous requests)
//! - This preserves Solid's ACL model where the user's identity is the access subject

use actix_web::{web, HttpRequest, HttpResponse, http::Method};
use log::{debug, error, info, warn};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::models::protected_settings::NostrUser;
use crate::services::nostr_service::NostrService;
use crate::utils::nip98::{generate_nip98_token, build_auth_header, validate_nip98_token, Nip98Config};
use nostr_sdk::{Keys, PublicKey, ToBech32};

/// JSS configuration from environment
#[derive(Debug, Clone)]
pub struct JssConfig {
    pub base_url: String,
    pub ws_url: String,
}

impl JssConfig {
    pub fn from_env() -> Self {
        Self {
            base_url: std::env::var("JSS_URL").unwrap_or_else(|_| "http://jss:3030".to_string()),
            ws_url: std::env::var("JSS_WS_URL")
                .unwrap_or_else(|_| "ws://jss:3030/.notifications".to_string()),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ADR-052: Default-private WAC + 3-container layout + double-gated writes
// ─────────────────────────────────────────────────────────────────────────────

/// Feature flag: default-private pods + 3-container layout.
/// When `true`, newly provisioned Pods use the owner-only root ACL, split
/// content into `./private`, `./public`, `./shared`, `./profile`, and the
/// public container is gated so only content explicitly marked public can
/// be written there. When `false`, the legacy permissive layout (root
/// `foaf:Agent` read + single flat tree) is used for backward compatibility.
pub fn pod_default_private_enabled() -> bool {
    std::env::var("POD_DEFAULT_PRIVATE")
        .map(|v| matches!(v.as_str(), "true" | "1" | "yes" | "on"))
        .unwrap_or(false)
}

/// Public base URL of the Pod server (used for NIP-39-derived WebIDs).
/// Falls back to `https://pods.visionclaw.org` when unset.
pub fn pod_public_base_url() -> String {
    std::env::var("POD_BASE_URL").unwrap_or_else(|_| "https://pods.visionclaw.org".to_string())
}

/// Derive the NIP-39 compliant WebID for a given hex Nostr pubkey.
/// Form: `{POD_BASE_URL}/{pubkey}/profile/card#me`.
pub fn derive_webid(pubkey_hex: &str) -> String {
    format!("{}/{}/profile/card#me", pod_public_base_url().trim_end_matches('/'), pubkey_hex)
}

/// Render the owner-only root/private ACL Turtle template per ADR-052.
pub fn render_owner_only_acl(owner_webid: &str) -> String {
    format!(
        r#"@prefix acl: <http://www.w3.org/ns/auth/acl#>.

<#owner>
    a acl:Authorization;
    acl:agent <{owner}>;
    acl:accessTo <./>;
    acl:default <./>;
    acl:mode acl:Read, acl:Write, acl:Control.
"#,
        owner = owner_webid
    )
}

/// Render the `./public/` ACL Turtle (foaf:Agent read + owner write).
pub fn render_public_container_acl(owner_webid: &str) -> String {
    format!(
        r#"@prefix acl: <http://www.w3.org/ns/auth/acl#>.
@prefix foaf: <http://xmlns.com/foaf/0.1/>.

<#publicRead>
    a acl:Authorization;
    acl:agentClass foaf:Agent;
    acl:accessTo <./>;
    acl:default <./>;
    acl:mode acl:Read.

<#ownerWrite>
    a acl:Authorization;
    acl:agent <{owner}>;
    acl:accessTo <./>;
    acl:default <./>;
    acl:mode acl:Read, acl:Write, acl:Control.
"#,
        owner = owner_webid
    )
}

/// Render the `./profile/` ACL Turtle. Only `card` is public-readable; the
/// rest of the container remains owner-scoped.
pub fn render_profile_container_acl(owner_webid: &str) -> String {
    format!(
        r#"@prefix acl: <http://www.w3.org/ns/auth/acl#>.
@prefix foaf: <http://xmlns.com/foaf/0.1/>.

<#publicReadCard>
    a acl:Authorization;
    acl:agentClass foaf:Agent;
    acl:accessTo <./card>;
    acl:mode acl:Read.

<#ownerWrite>
    a acl:Authorization;
    acl:agent <{owner}>;
    acl:accessTo <./>;
    acl:default <./>;
    acl:mode acl:Read, acl:Write, acl:Control.
"#,
        owner = owner_webid
    )
}

/// Render the minimal NIP-39 WebID document seeded at `./profile/card`.
pub fn render_webid_card(pubkey_hex: &str) -> String {
    format!(
        r#"@prefix foaf: <http://xmlns.com/foaf/0.1/>.
@prefix solid: <http://www.w3.org/ns/solid/terms#>.
@prefix nostr: <https://nostr.com/vocab/>.

<#me>
    a foaf:Person;
    nostr:hasPubkey "{pk}".
"#,
        pk = pubkey_hex
    )
}

/// ADR-052 double-gate decision result for a PUT to `./public/kg/*`.
/// The write is permitted only when BOTH the structural check (path is
/// under `public/kg`) and the source check (content asserts `public:: true`
/// or the caller set `X-VisionClaw-Visibility: public`) pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoubleGateDecision {
    /// Gate does not apply (path is not under `public/kg`).
    NotApplicable,
    /// Both gates passed — write is allowed.
    Allow,
    /// One or both gates failed — write must be rejected with 403.
    Deny,
}

/// Detect whether a body asserts a Logseq-style `public:: true` marker.
/// The assertion is tolerant to whitespace and case: the first bare
/// occurrence of `public:: true` / `public:: "true"` in the body satisfies
/// the source gate.
pub fn body_marks_public(body: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(body) else {
        return false;
    };
    text.lines().any(|line| {
        let trimmed = line.trim_start();
        // Logseq page-property syntax
        if let Some(rest) = trimmed.strip_prefix("public::") {
            let v = rest.trim().trim_matches(|c: char| c == '"' || c == '\'');
            return v.eq_ignore_ascii_case("true");
        }
        // JSON/YAML-ish "public": true variant
        if let Some(rest) = trimmed.strip_prefix("\"public\":") {
            let v = rest.trim().trim_end_matches(',');
            return v.eq_ignore_ascii_case("true");
        }
        false
    })
}

/// Evaluate the ADR-052 double gate for a PUT request.
/// `pod_relative_path` is the path inside the pod (e.g. `public/kg/foo.ttl`).
pub fn evaluate_double_gate(
    pod_relative_path: &str,
    body: &[u8],
    explicit_visibility_header: Option<&str>,
) -> DoubleGateDecision {
    // Strip any leading slashes for consistent matching.
    let normalised = pod_relative_path.trim_start_matches('/');

    // Structural gate: is this a write under ./public/kg/?
    let is_public_kg = normalised.starts_with("public/kg/")
        || normalised == "public/kg";
    if !is_public_kg {
        return DoubleGateDecision::NotApplicable;
    }

    // Source gate: explicit header wins when set to `public`.
    let header_asserts_public = explicit_visibility_header
        .map(|v| v.trim().eq_ignore_ascii_case("public"))
        .unwrap_or(false);

    if header_asserts_public || body_marks_public(body) {
        DoubleGateDecision::Allow
    } else {
        DoubleGateDecision::Deny
    }
}


/// Response from pod creation
#[derive(Debug, Serialize, Deserialize)]
pub struct PodCreationResponse {
    pub pod_url: String,
    pub webid: String,
    pub created: bool,
    pub structure: PodStructure,
}

/// Pod directory structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PodStructure {
    /// User's WebID profile card
    pub profile: String,
    /// Ontology contributions directory
    pub ontology_contributions: String,
    /// Ontology proposals directory
    pub ontology_proposals: String,
    /// Ontology annotations directory
    pub ontology_annotations: String,
    /// User preferences
    pub preferences: String,
    /// Notifications inbox
    pub inbox: String,
    /// ADR-052: private container (owner-only)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub private: Option<String>,
    /// ADR-052: public container (foaf:Agent read + owner write)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public: Option<String>,
    /// ADR-052: shared container (named-group placeholder, owner-only today)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shared: Option<String>,
}

/// Error response structure
#[derive(Debug, Serialize)]
pub struct SolidProxyError {
    pub error: String,
    pub details: Option<String>,
}

/// Result of extracting user identity from request
#[derive(Debug, Clone)]
pub struct UserIdentity {
    /// User's Nostr public key (hex)
    pub pubkey: String,
    /// Original NIP-98 token to forward
    pub nip98_token: String,
    /// Full Authorization header value
    pub auth_header: String,
}

/// Shared state for the proxy
pub struct SolidProxyState {
    pub config: JssConfig,
    pub http_client: Client,
    /// Server-side signing key for fallback (anonymous requests only)
    /// Used when user has no NIP-98 auth - JSS will see server identity
    pub server_keys: Option<Keys>,
    /// Whether to allow anonymous requests (server-signed)
    pub allow_anonymous: bool,
}

impl SolidProxyState {
    pub fn new() -> Self {
        let server_keys = std::env::var("SOLID_PROXY_SECRET_KEY")
            .ok()
            .and_then(|hex| {
                nostr_sdk::SecretKey::from_hex(&hex)
                    .ok()
                    .map(Keys::new)
            });

        let allow_anonymous = std::env::var("SOLID_ALLOW_ANONYMOUS")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);

        if server_keys.is_some() {
            info!("Solid proxy initialized with server-side signing key (for anonymous fallback)");
        } else {
            info!("Solid proxy initialized without server-side signing");
        }

        if allow_anonymous {
            info!("Anonymous Solid requests enabled (will use server identity)");
        } else {
            info!("Anonymous Solid requests disabled (user auth required)");
        }

        Self {
            config: JssConfig::from_env(),
            http_client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_else(|e| {
                    error!("Failed to create HTTP client with custom config: {e}, falling back to default");
                    Client::new()
                }),
            server_keys,
            allow_anonymous,
        }
    }

    /// Extract and verify user identity from NIP-98 Authorization header
    /// Returns the user's pubkey and original token for forwarding
    /// Validates the NIP-98 signature, timestamp, URL, and method before accepting
    pub fn extract_user_identity(&self, req: &HttpRequest) -> Option<UserIdentity> {
        let auth_header = req.headers().get("Authorization")?;
        let auth_str = auth_header.to_str().ok()?;

        // Must be "Nostr <base64-token>" format
        if !auth_str.starts_with("Nostr ") {
            debug!("Authorization header is not NIP-98 format");
            return None;
        }

        let token = &auth_str[6..]; // Skip "Nostr "

        // Reconstruct the request URL and method for NIP-98 validation
        // Behind a TLS-terminating proxy, connection_info returns internal
        // scheme/host; prefer X-Forwarded-* headers from the proxy.
        let conn_info = req.connection_info();
        let scheme = req.headers()
            .get("X-Forwarded-Proto")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_else(|| conn_info.scheme());
        let host = req.headers()
            .get("X-Forwarded-Host")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_else(|| conn_info.host());
        // Prefer X-Forwarded-URI (the original nginx-facing path, e.g. /solid/npub1.../
        // over the nginx-rewritten backend path (e.g. /api/solid/npub1.../)).
        // The client signs the NIP-98 token with the URL as sent to nginx.
        let path = req.headers()
            .get("X-Forwarded-URI")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_else(|| req.uri().path_and_query().map(|pq| pq.as_str()).unwrap_or("/"));
        let expected_url = format!(
            "{}://{}{}",
            scheme,
            host,
            path
        );
        let expected_method = req.method().as_str();

        // Validate the NIP-98 token: signature, timestamp, URL, and method
        match validate_nip98_token(token, &expected_url, expected_method, None) {
            Ok(validation) => {
                debug!(
                    "Verified NIP-98 user identity: pubkey={}...",
                    &validation.pubkey[..16.min(validation.pubkey.len())]
                );
                Some(UserIdentity {
                    pubkey: validation.pubkey,
                    nip98_token: token.to_string(),
                    auth_header: auth_str.to_string(),
                })
            }
            Err(e) => {
                warn!("NIP-98 token validation failed in proxy: {}", e);
                None
            }
        }
    }
}

impl Default for SolidProxyState {
    fn default() -> Self {
        Self::new()
    }
}

/// Main proxy handler for all /solid/* routes
/// Security: This handler prioritizes forwarding the USER's NIP-98 identity to JSS.
/// This ensures Solid ACLs are enforced against the actual user, not the proxy server.
/// Authentication flow:
/// 1. If user has NIP-98 Authorization header -> Forward it directly to JSS
/// 2. If no user auth AND anonymous allowed -> Generate server-signed NIP-98
/// 3. If no user auth AND anonymous NOT allowed -> Return 401
pub async fn handle_solid_proxy(
    req: HttpRequest,
    body: web::Bytes,
    path: web::Path<String>,
    state: web::Data<SolidProxyState>,
    _nostr_service: web::Data<NostrService>,
) -> HttpResponse {
    let target_path = path.into_inner();
    let method = req.method().clone();

    debug!(
        "Solid proxy: {} /solid/{} -> JSS",
        method, target_path
    );

    // ADR-052 double gate: PUTs under `<npub>/public/kg/*` must assert
    // public visibility either via `public:: true` in the body or the
    // `X-VisionClaw-Visibility: public` header. Denials short-circuit
    // before any upstream traffic and produce a stable 403 payload.
    if method.as_str() == "PUT" && pod_default_private_enabled() {
        // Strip the leading `<npub>/` so the relative path starts at the
        // pod root, mirroring the saga's own view of visibility.
        let pod_relative = match target_path.split_once('/') {
            Some((_npub, rest)) => rest,
            None => target_path.as_str(),
        };
        let visibility_header = req
            .headers()
            .get("X-VisionClaw-Visibility")
            .and_then(|v| v.to_str().ok());
        match evaluate_double_gate(pod_relative, &body, visibility_header) {
            DoubleGateDecision::Deny => {
                warn!(
                    "ADR-052 double-gate denial for PUT /solid/{} (header={:?})",
                    target_path, visibility_header
                );
                return HttpResponse::Forbidden()
                    .content_type("text/plain; charset=utf-8")
                    .body("double-gate failure: content not marked public");
            }
            DoubleGateDecision::Allow | DoubleGateDecision::NotApplicable => {}
        }
    }

    // Build target URL
    let target_url = format!("{}/{}", state.config.base_url, target_path);

    // Extract user identity from NIP-98 header (if present)
    let user_identity = state.extract_user_identity(&req);

    // Build the proxied request
    let mut proxy_req = match method.as_str() {
        "GET" => state.http_client.get(&target_url),
        "HEAD" => state.http_client.head(&target_url),
        "PUT" => state.http_client.put(&target_url),
        "POST" => state.http_client.post(&target_url),
        "DELETE" => state.http_client.delete(&target_url),
        "PATCH" => state.http_client.patch(&target_url),
        _ => {
            return HttpResponse::MethodNotAllowed().json(SolidProxyError {
                error: "Method not allowed".to_string(),
                details: Some(format!("Unsupported method: {}", method)),
            });
        }
    };

    // Forward relevant headers (excluding Authorization - handled separately)
    for (name, value) in req.headers() {
        let name_str = name.as_str().to_lowercase();
        // Forward these headers to JSS
        if matches!(
            name_str.as_str(),
            "accept" | "content-type" | "if-match" | "if-none-match" | "slug" | "link"
        ) {
            if let Ok(val) = value.to_str() {
                proxy_req = proxy_req.header(name.as_str(), val);
            }
        }
    }

    // Authentication handling: Prioritize USER identity over server identity
    if let Some(identity) = &user_identity {
        // PREFERRED: Forward user's NIP-98 directly - JSS sees the USER's identity
        // This ensures Solid ACLs are enforced against the actual user
        proxy_req = proxy_req.header("Authorization", &identity.auth_header);
        debug!(
            "Forwarding user NIP-98 to JSS (pubkey: {}...)",
            &identity.pubkey[..16.min(identity.pubkey.len())]
        );

        // Also add X-Forwarded-User for audit/logging (JSS can use this for context)
        proxy_req = proxy_req.header("X-Forwarded-User", format!("did:nostr:{}", identity.pubkey));
    } else if state.allow_anonymous {
        // FALLBACK: Anonymous request - use server identity if available
        if let Some(keys) = &state.server_keys {
            let config = Nip98Config {
                url: target_url.clone(),
                method: method.to_string(),
                body: if body.is_empty() {
                    None
                } else {
                    String::from_utf8(body.to_vec()).ok()
                },
            };

            match generate_nip98_token(keys, &config) {
                Ok(token) => {
                    proxy_req = proxy_req.header("Authorization", build_auth_header(&token));
                    debug!("Using server identity for anonymous request");
                }
                Err(e) => {
                    warn!("Failed to generate server NIP-98 token: {}", e);
                    // Continue without auth - JSS may allow public resources
                }
            }
        }
        // If no server keys, request goes through without Authorization
        // JSS will apply public access rules
    } else {
        // Anonymous not allowed and no user auth
        return HttpResponse::Unauthorized().json(SolidProxyError {
            error: "Authentication required".to_string(),
            details: Some("NIP-98 Authorization header required for Solid access".to_string()),
        });
    }

    // Add body for methods that support it
    if !body.is_empty() && matches!(method.as_str(), "PUT" | "POST" | "PATCH") {
        proxy_req = proxy_req.body(body.to_vec());
    }

    // Execute the request
    match proxy_req.send().await {
        Ok(response) => {
            let status = response.status();
            let mut builder = HttpResponse::build(
                actix_web::http::StatusCode::from_u16(status.as_u16())
                    .unwrap_or(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR),
            );

            // Rewrite internal JSS URLs to public proxy paths in header values.
            // JSS returns URLs like http://jss:3030/... or ws://jss:3030/... which
            // are unreachable from the browser.
            let jss_base = &state.config.base_url; // e.g. "http://jss:3030"
            // Also build the ws:// variant for updates-via headers
            let jss_ws_base = jss_base.replace("http://", "ws://");

            // Track content-type for body rewriting decision
            let mut response_content_type = String::new();

            // Forward response headers (with URL rewriting)
            for (name, value) in response.headers() {
                let name_str = name.as_str().to_lowercase();
                // Forward these headers from JSS
                if matches!(
                    name_str.as_str(),
                    "content-type"
                        | "etag"
                        | "last-modified"
                        | "link"
                        | "location"
                        | "updates-via"
                        | "wac-allow"
                        | "accept-patch"
                        | "accept-post"
                        | "allow"
                        | "ms-author-via"
                ) {
                    if let Ok(val) = value.to_str() {
                        if name_str == "content-type" {
                            response_content_type = val.to_string();
                        }
                        let rewritten = val
                            .replace(jss_base, "/solid")
                            .replace(&jss_ws_base, "/solid");
                        builder.insert_header((name.as_str(), rewritten));
                    }
                }
            }

            // Get response body and rewrite internal URLs
            match response.bytes().await {
                Ok(bytes) => {
                    // Only rewrite text-based responses (JSON-LD, Turtle, HTML)
                    let is_text = response_content_type.contains("json")
                        || response_content_type.contains("turtle")
                        || response_content_type.contains("html")
                        || response_content_type.contains("xml")
                        || response_content_type.contains("text");

                    if is_text {
                        if let Ok(body_str) = std::str::from_utf8(&bytes) {
                            let rewritten = body_str
                                .replace(jss_base, "/solid")
                                .replace(&jss_ws_base, "/solid");
                            builder.body(rewritten)
                        } else {
                            builder.body(bytes.to_vec())
                        }
                    } else {
                        builder.body(bytes.to_vec())
                    }
                }
                Err(e) => {
                    error!("Failed to read JSS response body: {}", e);
                    HttpResponse::BadGateway().json(SolidProxyError {
                        error: "Failed to read response".to_string(),
                        details: Some(e.to_string()),
                    })
                }
            }
        }
        Err(e) => {
            error!("Solid proxy request failed: {}", e);
            HttpResponse::BadGateway().json(SolidProxyError {
                error: "Proxy request failed".to_string(),
                details: Some(e.to_string()),
            })
        }
    }
}

/// Check if a pod exists for the given npub
async fn pod_exists(state: &SolidProxyState, npub: &str) -> bool {
    let pod_url = format!("{}/{}/", state.config.base_url, npub);
    match state.http_client.head(&pod_url).send().await {
        // 401/403 means the pod exists but WAC denies access (missing/restrictive ACL)
        Ok(resp) => resp.status().is_success() || resp.status().as_u16() == 401 || resp.status().as_u16() == 403,
        Err(_) => false,
    }
}

/// Helper: PUT a container (LDP BasicContainer) at `url`.
async fn put_container(
    state: &SolidProxyState,
    url: &str,
    auth_header: Option<&str>,
    label: &str,
) {
    let mut req = state.http_client.put(url);
    if let Some(auth) = auth_header {
        req = req.header("Authorization", auth);
    }
    req = req
        .header("Content-Type", "text/turtle")
        .header("Link", "<http://www.w3.org/ns/ldp#BasicContainer>; rel=\"type\"")
        .body("");
    match req.send().await {
        Ok(resp) if resp.status().is_success() || resp.status().as_u16() == 409 => {
            debug!("Created/confirmed container: {}", label);
        }
        Ok(resp) => debug!("Container {} creation returned {}", label, resp.status()),
        Err(e) => warn!("Failed to create container {}: {}", label, e),
    }
}

/// Helper: PUT a Turtle resource at `url` with the given body.
async fn put_turtle(
    state: &SolidProxyState,
    url: &str,
    body: String,
    auth_header: Option<&str>,
    label: &str,
) {
    let mut req = state.http_client.put(url);
    if let Some(auth) = auth_header {
        req = req.header("Authorization", auth);
    }
    req = req
        .header("Content-Type", "text/turtle")
        .body(body);
    match req.send().await {
        Ok(resp) if resp.status().is_success() || resp.status().as_u16() == 409 => {
            info!("Wrote {}", label);
        }
        Ok(resp) => warn!("Write of {} returned {}", label, resp.status()),
        Err(e) => warn!("Failed to write {}: {}", label, e),
    }
}

/// ADR-052 provisioning: default-private root + `./private`, `./public`,
/// `./shared`, `./profile` containers with correctly scoped ACLs.
async fn create_sovereign_pod_structure(
    state: &SolidProxyState,
    npub: &str,
    pubkey: &str,
    auth_header: Option<&str>,
) -> Result<PodStructure, String> {
    let pod_base = format!("{}/{}", state.config.base_url, npub);
    let owner_webid = derive_webid(pubkey);

    // 1. Root ACL — owner-only, zero foaf:Agent grants (ADR-052 §root-acl).
    put_turtle(
        state,
        &format!("{}/.acl", pod_base),
        render_owner_only_acl(&owner_webid),
        auth_header,
        &format!("root ACL for {}", npub),
    )
    .await;

    // 2. `./private/` — mirrors root's owner-only template; sub-containers
    //    inherit via `acl:default` so no further ACL writes are required.
    let private_dirs = ["private", "private/kg", "private/config", "private/bridges"];
    for dir in private_dirs {
        put_container(state, &format!("{}/{}/", pod_base, dir), auth_header, dir).await;
    }
    put_turtle(
        state,
        &format!("{}/private/.acl", pod_base),
        render_owner_only_acl(&owner_webid),
        auth_header,
        "./private/.acl",
    )
    .await;

    // 3. `./public/` — foaf:Agent read + owner write (double-gated at write
    //    time by `evaluate_double_gate`). Sub-container `public/kg/` inherits.
    for dir in ["public", "public/kg"] {
        put_container(state, &format!("{}/{}/", pod_base, dir), auth_header, dir).await;
    }
    put_turtle(
        state,
        &format!("{}/public/.acl", pod_base),
        render_public_container_acl(&owner_webid),
        auth_header,
        "./public/.acl",
    )
    .await;

    // 4. `./shared/` — placeholder for future named-group read. Today it
    //    inherits the owner-only default from root; we still write an
    //    explicit owner-only ACL for clarity and easy future extension.
    put_container(state, &format!("{}/shared/", pod_base), auth_header, "shared").await;
    put_turtle(
        state,
        &format!("{}/shared/.acl", pod_base),
        render_owner_only_acl(&owner_webid),
        auth_header,
        "./shared/.acl",
    )
    .await;

    // 5. `./profile/` + seeded `card` with NIP-39 claim.
    put_container(state, &format!("{}/profile/", pod_base), auth_header, "profile").await;
    put_turtle(
        state,
        &format!("{}/profile/.acl", pod_base),
        render_profile_container_acl(&owner_webid),
        auth_header,
        "./profile/.acl",
    )
    .await;
    put_turtle(
        state,
        &format!("{}/profile/card", pod_base),
        render_webid_card(pubkey),
        auth_header,
        "./profile/card (WebID)",
    )
    .await;

    info!(
        "Provisioned sovereign pod layout for {} (webid: {})",
        npub, owner_webid
    );

    Ok(PodStructure {
        profile: format!("{}/profile/card#me", pod_base),
        // Legacy fields kept for API compatibility — map to private/kg where
        // ontology work now lives by default.
        ontology_contributions: format!("{}/private/kg/contributions/", pod_base),
        ontology_proposals: format!("{}/private/kg/proposals/", pod_base),
        ontology_annotations: format!("{}/private/kg/annotations/", pod_base),
        preferences: format!("{}/private/config/", pod_base),
        inbox: format!("{}/private/bridges/inbox/", pod_base),
        private: Some(format!("{}/private/", pod_base)),
        public: Some(format!("{}/public/", pod_base)),
        shared: Some(format!("{}/shared/", pod_base)),
    })
}

/// Create the standard pod directory structure.
/// Dispatches to the ADR-052 sovereign layout when `POD_DEFAULT_PRIVATE=true`.
async fn create_pod_structure(
    state: &SolidProxyState,
    npub: &str,
    pubkey: &str,
    auth_header: Option<&str>,
) -> Result<PodStructure, String> {
    if pod_default_private_enabled() {
        return create_sovereign_pod_structure(state, npub, pubkey, auth_header).await;
    }

    let pod_base = format!("{}/{}", state.config.base_url, npub);

    // Directories to create (relative to pod root)
    let directories = [
        "profile",
        "ontology",
        "ontology/contributions",
        "ontology/proposals",
        "ontology/annotations",
        "preferences",
        "inbox",
    ];

    // Create each directory with a .meta file to ensure it exists as a container
    for dir in &directories {
        let dir_url = format!("{}/{}/", pod_base, dir);
        let mut req = state.http_client.put(&dir_url);

        if let Some(auth) = auth_header {
            req = req.header("Authorization", auth);
        }

        // Use Link header to indicate this is a Container (directory)
        req = req
            .header("Content-Type", "text/turtle")
            .header("Link", "<http://www.w3.org/ns/ldp#Container>; rel=\"type\"")
            .body("");

        match req.send().await {
            Ok(resp) if resp.status().is_success() || resp.status().as_u16() == 409 => {
                debug!("Created/confirmed directory: {}", dir);
            }
            Ok(resp) => {
                let status = resp.status();
                debug!("Directory {} creation returned {}: may already exist", dir, status);
            }
            Err(e) => {
                warn!("Failed to create directory {}: {}", dir, e);
            }
        }
    }

    // Create WebID profile card with Nostr identity
    let profile_url = format!("{}/profile/card", pod_base);
    let _webid = format!("{}#me", profile_url);
    let profile_content = format!(
        r#"@prefix foaf: <http://xmlns.com/foaf/0.1/> .
@prefix solid: <http://www.w3.org/ns/solid/terms#> .
@prefix vcard: <http://www.w3.org/2006/vcard/ns#> .
@prefix nostr: <https://github.com/nostr-protocol/nostr#> .

<#me>
    a foaf:Person ;
    solid:oidcIssuer <https://visionflow.info> ;
    nostr:pubkey "{pubkey}" ;
    nostr:npub "{npub}" ;
    vcard:hasUID <did:nostr:{pubkey}> .
"#,
        pubkey = pubkey,
        npub = npub
    );

    let mut profile_req = state.http_client.put(&profile_url);
    if let Some(auth) = auth_header {
        profile_req = profile_req.header("Authorization", auth);
    }
    profile_req = profile_req
        .header("Content-Type", "text/turtle")
        .body(profile_content);

    match profile_req.send().await {
        Ok(resp) if resp.status().is_success() || resp.status().as_u16() == 409 => {
            info!("Created WebID profile for {}", npub);
        }
        Ok(resp) => {
            let status = resp.status();
            debug!("Profile creation returned {}: may already exist", status);
        }
        Err(e) => {
            warn!("Failed to create profile: {}", e);
        }
    }

    // Create WAC ACL for the pod root granting owner full control + public read
    let acl_url = format!("{}/.acl", pod_base);
    let acl_content = format!(
        r#"@prefix acl: <http://www.w3.org/ns/auth/acl#> .
@prefix foaf: <http://xmlns.com/foaf/0.1/> .

<#owner>
    a acl:Authorization ;
    acl:agent <did:nostr:{pubkey}> ;
    acl:accessTo <./> ;
    acl:default <./> ;
    acl:mode acl:Read, acl:Write, acl:Control .

<#public>
    a acl:Authorization ;
    acl:agentClass foaf:Agent ;
    acl:accessTo <./> ;
    acl:mode acl:Read .
"#,
        pubkey = pubkey
    );

    let mut acl_req = state.http_client.put(&acl_url);
    if let Some(auth) = auth_header {
        acl_req = acl_req.header("Authorization", auth);
    }
    acl_req = acl_req
        .header("Content-Type", "text/turtle")
        .body(acl_content);

    match acl_req.send().await {
        Ok(resp) if resp.status().is_success() || resp.status().as_u16() == 409 => {
            info!("Created ACL for pod {}", npub);
        }
        Ok(resp) => {
            let status = resp.status();
            warn!("ACL creation for {} returned {}: access may be restricted", npub, status);
        }
        Err(e) => {
            warn!("Failed to create ACL for {}: {}", npub, e);
        }
    }

    Ok(PodStructure {
        profile: format!("{}/profile/card#me", pod_base),
        ontology_contributions: format!("{}/ontology/contributions/", pod_base),
        ontology_proposals: format!("{}/ontology/proposals/", pod_base),
        ontology_annotations: format!("{}/ontology/annotations/", pod_base),
        preferences: format!("{}/preferences/", pod_base),
        inbox: format!("{}/inbox/", pod_base),
        private: None,
        public: None,
        shared: None,
    })
}

/// Test-only helper: directly provision a pod's root container + structure
/// without the `pod_exists` HEAD short-circuit. Used by ADR-052 integration
/// tests to observe the full sequence of PUTs a fresh Pod generates.
#[doc(hidden)]
pub async fn create_pod_if_missing_for_tests(
    state: &SolidProxyState,
    npub: &str,
    pubkey: &str,
) -> Result<PodStructure, String> {
    // Create the pod root container
    let pod_container_url = format!("{}/{}/", state.config.base_url, npub);
    let mut req = state.http_client.put(&pod_container_url);
    req = req
        .header("Content-Type", "text/turtle")
        .header("Link", "<http://www.w3.org/ns/ldp#BasicContainer>; rel=\"type\"")
        .body("");
    match req.send().await {
        Ok(resp) if resp.status().is_success() || resp.status().as_u16() == 409 => {}
        Ok(resp) => return Err(format!("root container PUT failed: {}", resp.status())),
        Err(e) => return Err(format!("root container PUT error: {}", e)),
    }
    create_pod_structure(state, npub, pubkey, None).await
}

/// Initialize pod with auto-provisioning
/// Called when user first accesses their pod area
pub async fn ensure_pod_exists(
    state: &SolidProxyState,
    npub: &str,
    pubkey: &str,
    auth_header: Option<&str>,
) -> Result<(bool, PodStructure), String> {
    // Check if pod already exists
    if pod_exists(state, npub).await {
        let pod_base = format!("{}/{}", state.config.base_url, npub);

        // Ensure ACL exists even for pre-existing pods (backfill for pods created
        // before ACL creation was added to create_pod_structure)
        let acl_url = format!("{}/.acl", pod_base);
        let acl_check = state.http_client.head(&acl_url).send().await;
        let acl_missing = match acl_check {
            Ok(resp) => !resp.status().is_success(),
            Err(_) => true,
        };
        if acl_missing {
            info!("Backfilling missing ACL for existing pod: {}", npub);
            // Under the ADR-052 flag, backfill uses the owner-only sovereign
            // template so pre-existing pods default to private; otherwise keep
            // the legacy permissive ACL for backward compatibility.
            let acl_content = if pod_default_private_enabled() {
                render_owner_only_acl(&derive_webid(pubkey))
            } else {
                format!(
                    r#"@prefix acl: <http://www.w3.org/ns/auth/acl#> .
@prefix foaf: <http://xmlns.com/foaf/0.1/> .

<#owner>
    a acl:Authorization ;
    acl:agent <did:nostr:{pubkey}> ;
    acl:accessTo <./> ;
    acl:default <./> ;
    acl:mode acl:Read, acl:Write, acl:Control .

<#public>
    a acl:Authorization ;
    acl:agentClass foaf:Agent ;
    acl:accessTo <./> ;
    acl:mode acl:Read .
"#,
                    pubkey = pubkey
                )
            };

            let mut acl_req = state.http_client.put(&acl_url);
            if let Some(auth) = auth_header {
                acl_req = acl_req.header("Authorization", auth);
            }
            acl_req = acl_req
                .header("Content-Type", "text/turtle")
                .body(acl_content);

            match acl_req.send().await {
                Ok(resp) if resp.status().is_success() => {
                    info!("Backfilled ACL for pod {}", npub);
                }
                Ok(resp) => {
                    warn!("ACL backfill for {} returned {}", npub, resp.status());
                }
                Err(e) => {
                    warn!("Failed to backfill ACL for {}: {}", npub, e);
                }
            }
        }

        let sovereign = pod_default_private_enabled();
        return Ok((false, PodStructure {
            profile: format!("{}/profile/card#me", pod_base),
            ontology_contributions: format!("{}/ontology/contributions/", pod_base),
            ontology_proposals: format!("{}/ontology/proposals/", pod_base),
            ontology_annotations: format!("{}/ontology/annotations/", pod_base),
            preferences: format!("{}/preferences/", pod_base),
            inbox: format!("{}/inbox/", pod_base),
            private: sovereign.then(|| format!("{}/private/", pod_base)),
            public: sovereign.then(|| format!("{}/public/", pod_base)),
            shared: sovereign.then(|| format!("{}/shared/", pod_base)),
        }));
    }

    info!("Auto-provisioning pod for user: {}", npub);

    // Create pod as an LDP Basic Container at the root level via PUT
    let pod_container_url = format!("{}/{}/", state.config.base_url, npub);

    let mut req = state.http_client.put(&pod_container_url);
    if let Some(auth) = auth_header {
        req = req.header("Authorization", auth);
    }
    req = req
        .header("Content-Type", "text/turtle")
        .header("Link", "<http://www.w3.org/ns/ldp#BasicContainer>; rel=\"type\"")
        .body("");

    let response = req.send().await;

    match response {
        Ok(resp) if resp.status().is_success() || resp.status().as_u16() == 409 => {
            // Pod created (or already exists), now create structure
            let structure = create_pod_structure(state, npub, pubkey, auth_header).await?;
            Ok((true, structure))
        }
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            Err(format!("Pod creation failed ({}): {}", status, body))
        }
        Err(e) => Err(format!("Failed to connect to Solid server: {}", e)),
    }
}

/// Create a new pod for a user based on their Nostr identity
pub async fn create_pod(
    req: HttpRequest,
    _body: web::Json<CreatePodRequest>,
    state: web::Data<SolidProxyState>,
    nostr_service: web::Data<NostrService>,
) -> HttpResponse {
    // Get user from session/token
    let user = match get_user_from_request(&req, &nostr_service).await {
        Some(u) => u,
        None => {
            return HttpResponse::Unauthorized().json(SolidProxyError {
                error: "Authentication required".to_string(),
                details: Some("Valid Nostr session required to create pod".to_string()),
            });
        }
    };

    let npub = &user.npub;
    let pubkey = &user.pubkey;
    info!("Creating pod for user: {}", npub);

    // Get auth header for forwarding
    let auth_header = req.headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok());

    // Use ensure_pod_exists for creation with full structure
    match ensure_pod_exists(&state, npub, pubkey, auth_header).await {
        Ok((created, structure)) => {
            let pod_url = format!("{}/{}/", state.config.base_url, npub);
            let status = if created {
                actix_web::http::StatusCode::CREATED
            } else {
                actix_web::http::StatusCode::OK
            };
            HttpResponse::build(status).json(PodCreationResponse {
                pod_url,
                webid: structure.profile.clone(),
                created,
                structure,
            })
        }
        Err(e) => {
            error!("Pod creation failed: {}", e);
            HttpResponse::BadGateway().json(SolidProxyError {
                error: "Pod creation failed".to_string(),
                details: Some(e),
            })
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreatePodRequest {
    /// Optional custom pod name (defaults to npub)
    pub name: Option<String>,
}

/// Check if a pod exists for the current user
pub async fn check_pod_exists(
    req: HttpRequest,
    state: web::Data<SolidProxyState>,
    nostr_service: web::Data<NostrService>,
) -> HttpResponse {
    let user = match get_user_from_request(&req, &nostr_service).await {
        Some(u) => u,
        None => {
            return HttpResponse::Unauthorized().json(SolidProxyError {
                error: "Authentication required".to_string(),
                details: None,
            });
        }
    };

    let pod_base = format!("{}/{}", state.config.base_url, user.npub);
    let pod_url = format!("{}/", pod_base);

    match state.http_client.head(&pod_url).send().await {
        Ok(resp) if resp.status().is_success() => {
            HttpResponse::Ok().json(serde_json::json!({
                "exists": true,
                "pod_url": pod_url,
                "webid": format!("{}/profile/card#me", pod_base),
                "structure": PodStructure {
                    profile: format!("{}/profile/card#me", pod_base),
                    ontology_contributions: format!("{}/ontology/contributions/", pod_base),
                    ontology_proposals: format!("{}/ontology/proposals/", pod_base),
                    ontology_annotations: format!("{}/ontology/annotations/", pod_base),
                    preferences: format!("{}/preferences/", pod_base),
                    inbox: format!("{}/inbox/", pod_base),
                    private: None,
                    public: None,
                    shared: None,
                }
            }))
        }
        Ok(resp) if resp.status().as_u16() == 404 => {
            HttpResponse::Ok().json(serde_json::json!({
                "exists": false,
                "suggested_url": pod_url
            }))
        }
        Ok(resp) => {
            HttpResponse::build(
                actix_web::http::StatusCode::from_u16(resp.status().as_u16())
                    .unwrap_or(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR),
            )
            .json(SolidProxyError {
                error: "Failed to check pod".to_string(),
                details: None,
            })
        }
        Err(e) => {
            HttpResponse::BadGateway().json(SolidProxyError {
                error: "Failed to connect to Solid server".to_string(),
                details: Some(e.to_string()),
            })
        }
    }
}

/// Initialize pod for current user (auto-provision if needed)
/// This should be called on user login to ensure their pod exists
pub async fn init_pod(
    req: HttpRequest,
    state: web::Data<SolidProxyState>,
    nostr_service: web::Data<NostrService>,
) -> HttpResponse {
    // Get user from session/token
    let user = match get_user_from_request(&req, &nostr_service).await {
        Some(u) => u,
        None => {
            return HttpResponse::Unauthorized().json(SolidProxyError {
                error: "Authentication required".to_string(),
                details: Some("Valid Nostr session required to initialize pod".to_string()),
            });
        }
    };

    let npub = &user.npub;
    let pubkey = &user.pubkey;

    // Get auth header for forwarding
    let auth_header = req.headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok());

    debug!("Initializing pod for user: {}", npub);

    // Use ensure_pod_exists for auto-provisioning
    match ensure_pod_exists(&state, npub, pubkey, auth_header).await {
        Ok((created, structure)) => {
            let pod_url = format!("{}/{}/", state.config.base_url, npub);
            HttpResponse::Ok().json(serde_json::json!({
                "pod_url": pod_url,
                "webid": structure.profile,
                "created": created,
                "structure": structure
            }))
        }
        Err(e) => {
            error!("Pod initialization failed: {}", e);
            HttpResponse::BadGateway().json(SolidProxyError {
                error: "Pod initialization failed".to_string(),
                details: Some(e),
            })
        }
    }
}

/// Initialize pod from NIP-98 auth (for Solid-first requests)
/// Can be called with NIP-98 Authorization header instead of Bearer token
pub async fn init_pod_nip98(
    req: HttpRequest,
    state: web::Data<SolidProxyState>,
) -> HttpResponse {
    // Get user identity from NIP-98 header
    let identity = match state.extract_user_identity(&req) {
        Some(id) => id,
        None => {
            return HttpResponse::Unauthorized().json(SolidProxyError {
                error: "NIP-98 authentication required".to_string(),
                details: Some("Valid NIP-98 Authorization header required".to_string()),
            });
        }
    };

    // Convert hex pubkey to npub (bech32)
    let npub = match PublicKey::from_hex(&identity.pubkey) {
        Ok(pk) => match pk.to_bech32() {
            Ok(n) => n,
            Err(e) => {
                error!("Failed to convert pubkey to npub: {}", e);
                return HttpResponse::InternalServerError().json(SolidProxyError {
                    error: "Failed to process public key".to_string(),
                    details: Some(e.to_string()),
                });
            }
        },
        Err(e) => {
            error!("Invalid pubkey in NIP-98 token: {}", e);
            return HttpResponse::BadRequest().json(SolidProxyError {
                error: "Invalid public key".to_string(),
                details: Some(e.to_string()),
            });
        }
    };

    debug!("Initializing pod for NIP-98 user: {}", npub);

    // Use ensure_pod_exists for auto-provisioning
    match ensure_pod_exists(&state, &npub, &identity.pubkey, Some(&identity.auth_header)).await {
        Ok((created, structure)) => {
            let pod_url = format!("{}/{}/", state.config.base_url, npub);
            HttpResponse::Ok().json(serde_json::json!({
                "pod_url": pod_url,
                "webid": structure.profile,
                "created": created,
                "structure": structure,
                "npub": npub
            }))
        }
        Err(e) => {
            error!("Pod initialization failed: {}", e);
            HttpResponse::BadGateway().json(SolidProxyError {
                error: "Pod initialization failed".to_string(),
                details: Some(e),
            })
        }
    }
}

/// Get user from request using NIP-98 auth (primary) or session token (fallback)
async fn get_user_from_request(
    req: &HttpRequest,
    nostr_service: &web::Data<NostrService>,
) -> Option<NostrUser> {
    // Try to get token from Authorization header
    let auth_header = req.headers().get("Authorization")?;
    let auth_str = auth_header.to_str().ok()?;

    // Try NIP-98 first (primary authentication path)
    if auth_str.starts_with("Nostr ") {
        // Behind a TLS-terminating proxy, connection_info returns internal
        // scheme/host; prefer X-Forwarded-* headers from the proxy.
        let conn_info = req.connection_info();
        let scheme = req.headers()
            .get("X-Forwarded-Proto")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_else(|| conn_info.scheme());
        let host = req.headers()
            .get("X-Forwarded-Host")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_else(|| conn_info.host());
        // Prefer X-Forwarded-URI (the original nginx-facing path, e.g. /solid/pods/init)
        // over the nginx-rewritten backend path (e.g. /api/solid/pods/init).
        // The client signs the NIP-98 token with the URL as sent to nginx, not the internal path.
        let path = req.headers()
            .get("X-Forwarded-URI")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_else(|| req.uri().path_and_query().map(|pq| pq.as_str()).unwrap_or("/"));
        let request_url = format!("{}://{}{}", scheme, host, path);
        let request_method = req.method().as_str();

        match nostr_service
            .verify_nip98_auth(auth_str, &request_url, request_method, None)
            .await
        {
            Ok(user) => return Some(user),
            Err(e) => {
                warn!("NIP-98 auth failed in pod management: {}", e);
                return None;
            }
        }
    }

    // Fall back to Bearer session token (legacy path)
    if auth_str.starts_with("Bearer ") {
        let token = &auth_str[7..];
        nostr_service.get_session(token).await
    } else {
        None
    }
}

/// Get user identity from NIP-98 Authorization header
#[allow(dead_code)]
async fn get_user_identity_from_request(
    req: &HttpRequest,
    state: &web::Data<SolidProxyState>,
) -> Option<UserIdentity> {
    state.extract_user_identity(req)
}

// ============================================================================
// WebSocket Handler for Solid-0.1 Notifications
// ============================================================================

use actix::{Actor, StreamHandler, ActorContext};
use actix_web_actors::ws;

/// WebSocket actor for proxying solid-0.1 notifications
pub struct SolidNotificationWs {
    /// User identity for the connection
    user_identity: Option<UserIdentity>,
    /// JSS WebSocket URL
    #[allow(dead_code)]
    jss_ws_url: String,
    /// Subscribed resources
    subscriptions: Vec<String>,
}

impl SolidNotificationWs {
    pub fn new(user_identity: Option<UserIdentity>, jss_config: &JssConfig) -> Self {
        Self {
            user_identity,
            jss_ws_url: jss_config.ws_url.clone(),
            subscriptions: Vec::new(),
        }
    }
}

impl Actor for SolidNotificationWs {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        info!("Solid notification WebSocket started for user: {:?}",
            self.user_identity.as_ref().map(|u| &u.pubkey[..16.min(u.pubkey.len())]));
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        info!("Solid notification WebSocket stopped");
    }
}

/// Message format for solid-0.1 protocol
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SolidNotificationMessage {
    /// Subscribe to resource changes
    #[serde(rename = "sub")]
    Subscribe { resource: String },
    /// Unsubscribe from resource
    #[serde(rename = "unsub")]
    Unsubscribe { resource: String },
    /// Acknowledgment from server
    #[serde(rename = "ack")]
    Ack { resource: String },
    /// Publication notification (resource changed)
    #[serde(rename = "pub")]
    Publish { resource: String },
    /// Ping for keepalive
    #[serde(rename = "ping")]
    Ping,
    /// Pong response
    #[serde(rename = "pong")]
    Pong,
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for SolidNotificationWs {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Text(text)) => {
                debug!("Received solid notification message: {}", text);

                // Parse the solid-0.1 protocol message
                match serde_json::from_str::<SolidNotificationMessage>(&text) {
                    Ok(SolidNotificationMessage::Subscribe { resource }) => {
                        info!("Client subscribing to: {}", resource);
                        self.subscriptions.push(resource.clone());
                        // Send ack back to client
                        let ack = SolidNotificationMessage::Ack { resource };
                        if let Ok(json) = serde_json::to_string(&ack) {
                            ctx.text(json);
                        }
                    }
                    Ok(SolidNotificationMessage::Unsubscribe { resource }) => {
                        info!("Client unsubscribing from: {}", resource);
                        self.subscriptions.retain(|r| r != &resource);
                    }
                    Ok(SolidNotificationMessage::Ping) => {
                        let pong = SolidNotificationMessage::Pong;
                        if let Ok(json) = serde_json::to_string(&pong) {
                            ctx.text(json);
                        }
                    }
                    Ok(msg) => {
                        debug!("Received other solid message: {:?}", msg);
                    }
                    Err(e) => {
                        warn!("Failed to parse solid notification message: {}", e);
                    }
                }
            }
            Ok(ws::Message::Ping(msg)) => {
                ctx.pong(&msg);
            }
            Ok(ws::Message::Pong(_)) => {}
            Ok(ws::Message::Binary(_)) => {
                warn!("Binary messages not supported for solid notifications");
            }
            Ok(ws::Message::Close(reason)) => {
                info!("WebSocket close requested: {:?}", reason);
                ctx.stop();
            }
            Ok(ws::Message::Continuation(_)) => {
                warn!("Continuation frames not supported");
            }
            Ok(ws::Message::Nop) => {}
            Err(e) => {
                error!("WebSocket protocol error: {}", e);
                ctx.stop();
            }
        }
    }
}

/// WebSocket handler for solid-0.1 notifications
/// Endpoint: /solid/.notifications (WebSocket upgrade)
/// Protocol: solid-0.1
/// - Client sends: { "type": "sub", "resource": "<url>" }
/// - Server sends: { "type": "ack", "resource": "<url>" }
/// - Server sends: { "type": "pub", "resource": "<url>" } on changes
pub async fn handle_solid_notifications_ws(
    req: HttpRequest,
    stream: web::Payload,
    state: web::Data<SolidProxyState>,
) -> Result<HttpResponse, actix_web::Error> {
    // Extract user identity if present
    let user_identity = state.extract_user_identity(&req);

    if let Some(ref identity) = user_identity {
        debug!(
            "Solid notifications WebSocket connecting for user: {}...",
            &identity.pubkey[..16.min(identity.pubkey.len())]
        );
    } else {
        debug!("Solid notifications WebSocket connecting (anonymous)");
    }

    // Create WebSocket actor
    let ws_actor = SolidNotificationWs::new(user_identity, &state.config);

    // Start WebSocket handshake
    ws::start(ws_actor, &req, stream)
}

/// Configuration for connecting to JSS notifications via WebSocket proxy
/// This creates a bidirectional proxy between the client and JSS's WebSocket endpoint
pub async fn handle_solid_notifications_proxy(
    req: HttpRequest,
    stream: web::Payload,
    state: web::Data<SolidProxyState>,
) -> Result<HttpResponse, actix_web::Error> {
    // For full proxy mode, we would need to establish a connection to JSS
    // and relay messages bidirectionally. For now, use the simpler actor model.
    handle_solid_notifications_ws(req, stream, state).await
}

/// Health check for JSS connectivity
/// Returns the health status of the connection to the JavaScript Solid Server.
/// - 200 OK with "healthy" status if JSS is reachable
/// - 503 Service Unavailable with "degraded" if JSS responds with non-success
/// - 503 Service Unavailable with "unhealthy" if JSS is unreachable
pub async fn jss_health_check(state: web::Data<SolidProxyState>) -> HttpResponse {
    let health_url = format!("{}/", state.config.base_url);

    match state
        .http_client
        .head(&health_url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => {
            HttpResponse::Ok().json(serde_json::json!({
                "status": "healthy",
                "jss_url": state.config.base_url
            }))
        }
        Ok(response) => {
            HttpResponse::ServiceUnavailable().json(serde_json::json!({
                "status": "degraded",
                "code": response.status().as_u16()
            }))
        }
        Err(e) => {
            HttpResponse::ServiceUnavailable().json(serde_json::json!({
                "status": "unhealthy",
                "error": e.to_string()
            }))
        }
    }
}

// ─── DID Resolution Proxy ────────────────────────────────────────────────────
//
// DID documents are public and unauthenticated.  They must be served at the
// canonical paths specified by the DID spec:
//   /.well-known/did.json   (did:web method)
//   /did/{tail}             (JSS DID resolution endpoint)
//
// Both are proxied directly to JSS without NIP-98 or ACL enforcement.
// Internal JSS host references are stripped so DID docs are self-consistent
// when consumed externally.

/// Proxy GET /.well-known/did.json → JSS
async fn handle_did_wellknown(state: web::Data<SolidProxyState>) -> HttpResponse {
    let target_url = format!("{}/.well-known/did.json", state.config.base_url);
    proxy_did_request(&state, &target_url).await
}

/// Proxy GET /did/{tail:.*} → JSS (DID resolution, e.g. /did/nostr:<npub>)
async fn handle_did_proxy(
    path: web::Path<String>,
    state: web::Data<SolidProxyState>,
) -> HttpResponse {
    let tail = path.into_inner();
    let target_url = format!("{}/did/{}", state.config.base_url, tail);
    proxy_did_request(&state, &target_url).await
}

/// Shared DID proxy implementation.  Rewrites internal JSS host refs in the
/// body so consumers receive clean, externally-addressable DID documents.
async fn proxy_did_request(state: &SolidProxyState, target_url: &str) -> HttpResponse {
    match state.http_client.get(target_url).send().await {
        Ok(resp) => {
            let status = actix_web::http::StatusCode::from_u16(resp.status().as_u16())
                .unwrap_or(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR);
            let content_type = resp
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("application/json")
                .to_string();
            match resp.bytes().await {
                Ok(bytes) => {
                    // Strip internal JSS host so DID doc URLs are relative or external.
                    if let Ok(body_str) = std::str::from_utf8(&bytes) {
                        let rewritten = body_str.replace(&state.config.base_url, "");
                        HttpResponse::build(status)
                            .content_type(content_type)
                            .body(rewritten)
                    } else {
                        HttpResponse::build(status).content_type(content_type).body(bytes)
                    }
                }
                Err(e) => HttpResponse::BadGateway().json(SolidProxyError {
                    error: "DID response read failed".to_string(),
                    details: Some(e.to_string()),
                }),
            }
        }
        Err(e) => HttpResponse::BadGateway().json(SolidProxyError {
            error: "DID resolution failed".to_string(),
            details: Some(e.to_string()),
        }),
    }
}

/// Configure Solid proxy routes
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    info!("=== REGISTERING SOLID PROXY ROUTES ===");
    cfg.app_data(web::Data::new(SolidProxyState::new()))
        .service(
            web::scope("/solid")
                // Health check endpoint
                .route("/health", web::get().to(jss_health_check))
                // WebSocket endpoint for notifications (solid-0.1 protocol)
                .route("/.notifications", web::get().to(handle_solid_notifications_ws))
                // Pod management endpoints
                .route("/pods", web::post().to(create_pod))
                .route("/pods/check", web::get().to(check_pod_exists))
                .route("/pods/init", web::post().to(init_pod))
                .route("/pods/init-nip98", web::post().to(init_pod_nip98))
                // LDP proxy for all other paths
                .route("/{tail:.*}", web::get().to(handle_solid_proxy))
                .route("/{tail:.*}", web::put().to(handle_solid_proxy))
                .route("/{tail:.*}", web::post().to(handle_solid_proxy))
                .route("/{tail:.*}", web::delete().to(handle_solid_proxy))
                .route("/{tail:.*}", web::head().to(handle_solid_proxy))
                .route(
                    "/{tail:.*}",
                    web::method(Method::PATCH).to(handle_solid_proxy),
                ),
        )
        // DID document resolution — public endpoints at canonical spec paths.
        // Must sit outside /solid scope so the URL scheme matches the DID spec.
        .service(
            web::resource("/.well-known/did.json")
                .route(web::get().to(handle_did_wellknown)),
        )
        .service(
            web::scope("/did")
                .route("/{tail:.*}", web::get().to(handle_did_proxy)),
        );
}
