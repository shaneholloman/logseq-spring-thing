//! Enterprise RBAC Middleware
//!
//! Provides role-based access control for enterprise endpoints using the
//! `EnterpriseRole` model defined in ADR-040.
//!
//! ## Usage
//!
//! As middleware on a scope/resource:
//! ```rust,ignore
//! web::scope("/admin")
//!     .wrap(RequireRole::admin())
//!     .route("/settings", web::get().to(handler))
//! ```
//!
//! As a per-handler guard via the helper function:
//! ```rust,ignore
//! async fn my_handler(req: HttpRequest) -> impl Responder {
//!     require_role(&req, EnterpriseRole::Broker)?;
//!     // ... handler logic
//! }
//! ```
//!
//! ## Authentication Modes
//!
//! - **Default** (`X-Enterprise-Role` header): Demo/dev mode reads the role
//!   directly from a trusted header. Suitable for development behind a gateway.
//!
//! - **`nip98-auth` feature**: Reads a `Nostr <base64>` Authorization header,
//!   verifies the NIP-98 Schnorr signature, extracts the signer pubkey, and
//!   resolves the enterprise role from the `Nip98RoleResolver`. The
//!   `X-Enterprise-Role` header is ignored when this feature is active.

use actix_web::{
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    Error, HttpMessage, HttpRequest, HttpResponse,
};
use futures_util::future::LocalBoxFuture;
use log::{debug, info, warn};
use serde_json::json;
use std::future::{ready, Ready};
use std::rc::Rc;

use crate::models::enterprise::EnterpriseRole;

// ---------------------------------------------------------------------------
// NIP-98 role resolution (feature-gated)
// ---------------------------------------------------------------------------

/// Maps a verified NIP-98 pubkey to an [`EnterpriseRole`].
///
/// Implementations may consult a config file, database, or external service.
/// The default in-memory implementation is provided for bootstrapping; replace
/// it with a persistent store in production.
#[cfg(feature = "nip98-auth")]
pub trait Nip98RoleResolver: Send + Sync + 'static {
    /// Look up the enterprise role for the given hex pubkey.
    /// Returns `None` if the pubkey is not registered.
    fn resolve_role(&self, pubkey_hex: &str) -> Option<EnterpriseRole>;
}

/// In-memory role map for development and testing.
#[cfg(feature = "nip98-auth")]
#[derive(Debug, Clone, Default)]
pub struct InMemoryRoleMap {
    entries: std::collections::HashMap<String, EnterpriseRole>,
}

#[cfg(feature = "nip98-auth")]
impl InMemoryRoleMap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or update a pubkey-to-role mapping.
    pub fn insert(&mut self, pubkey_hex: impl Into<String>, role: EnterpriseRole) {
        self.entries.insert(pubkey_hex.into(), role);
    }
}

#[cfg(feature = "nip98-auth")]
impl Nip98RoleResolver for InMemoryRoleMap {
    fn resolve_role(&self, pubkey_hex: &str) -> Option<EnterpriseRole> {
        self.entries.get(pubkey_hex).cloned()
    }
}

/// Request extension inserted by the NIP-98 auth path: carries the verified
/// pubkey alongside the resolved role.
#[cfg(feature = "nip98-auth")]
#[derive(Clone, Debug)]
pub struct Nip98IdentityExt {
    pub pubkey_hex: String,
    pub role: EnterpriseRole,
}

/// Extract the enterprise role from a NIP-98 Authorization header.
///
/// 1. Parse "Nostr <base64>" from the Authorization header.
/// 2. Validate the NIP-98 token (signature, timestamp, URL, method).
/// 3. Look up the signer's pubkey in the role resolver.
/// 4. Fall back to `Contributor` if the pubkey is not registered.
#[cfg(feature = "nip98-auth")]
fn extract_role_from_nip98(
    req: &ServiceRequest,
    resolver: &dyn Nip98RoleResolver,
) -> Result<(String, EnterpriseRole), String> {
    use crate::utils::nip98::{parse_auth_header, validate_nip98_token};

    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| "Missing Authorization header".to_string())?;

    let token = parse_auth_header(auth_header)
        .ok_or_else(|| "Authorization header is not a Nostr NIP-98 token".to_string())?;

    // Build the expected URL from the request.
    let conn_info = req.connection_info();
    let scheme = req
        .headers()
        .get("X-Forwarded-Proto")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_else(|| conn_info.scheme());
    let host = req
        .headers()
        .get("X-Forwarded-Host")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_else(|| conn_info.host());
    let url = format!(
        "{}://{}{}",
        scheme,
        host,
        req.uri()
            .path_and_query()
            .map(|pq| pq.as_str())
            .unwrap_or("/")
    );
    let method = req.method().as_str();

    let validation = validate_nip98_token(token, &url, method, None)
        .map_err(|e| format!("NIP-98 validation failed: {}", e))?;

    let role = resolver
        .resolve_role(&validation.pubkey)
        .unwrap_or(EnterpriseRole::Contributor);

    Ok((validation.pubkey, role))
}

/// Helper to extract role from NIP-98 for per-handler usage.
#[cfg(feature = "nip98-auth")]
pub fn require_role_nip98(
    req: &HttpRequest,
    resolver: &dyn Nip98RoleResolver,
    min_role: EnterpriseRole,
) -> Result<Nip98IdentityExt, Result<HttpResponse, actix_web::Error>> {
    use crate::utils::nip98::{parse_auth_header, validate_nip98_token};

    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            Ok(HttpResponse::Unauthorized().json(json!({
                "error": "Missing Authorization header (expected NIP-98)"
            })))
        })?;

    let token = parse_auth_header(auth_header).ok_or_else(|| {
        Ok(HttpResponse::Unauthorized().json(json!({
            "error": "Authorization header is not a Nostr NIP-98 token"
        })))
    })?;

    let conn_info = req.connection_info();
    let scheme = req
        .headers()
        .get("X-Forwarded-Proto")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_else(|| conn_info.scheme());
    let host = req
        .headers()
        .get("X-Forwarded-Host")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_else(|| conn_info.host());
    let url = format!(
        "{}://{}{}",
        scheme,
        host,
        req.uri()
            .path_and_query()
            .map(|pq| pq.as_str())
            .unwrap_or("/")
    );
    let method = req.method().as_str();

    let validation = validate_nip98_token(token, &url, method, None).map_err(|e| {
        Ok(HttpResponse::Unauthorized().json(json!({
            "error": format!("NIP-98 validation failed: {}", e)
        })))
    })?;

    let actual = resolver
        .resolve_role(&validation.pubkey)
        .unwrap_or(EnterpriseRole::Contributor);
    let actual_level = role_level(&actual);
    let required_level = role_level(&min_role);

    if actual_level >= required_level {
        Ok(Nip98IdentityExt {
            pubkey_hex: validation.pubkey,
            role: actual,
        })
    } else {
        Err(Ok(HttpResponse::Forbidden().json(json!({
            "error": format!("Requires {:?} role or higher", min_role),
            "your_role": format!("{:?}", actual),
            "pubkey": validation.pubkey,
        }))))
    }
}

// ---------------------------------------------------------------------------
// Role level mapping
// ---------------------------------------------------------------------------

/// Maps an `EnterpriseRole` to a numeric privilege level.
/// Higher values represent broader permissions:
///   Admin (4) > Broker (3) > Auditor (2) > Contributor (1)
fn role_level(role: &EnterpriseRole) -> u8 {
    match role {
        EnterpriseRole::Admin => 4,
        EnterpriseRole::Broker => 3,
        EnterpriseRole::Auditor => 2,
        EnterpriseRole::Contributor => 1,
    }
}

// ---------------------------------------------------------------------------
// Role extraction
// ---------------------------------------------------------------------------

/// Extracts the enterprise role from request headers.
///
/// Reads `X-Enterprise-Role` (case-insensitive value matching via serde).
/// Falls back to `Contributor` when the header is absent or unparseable.
pub fn extract_enterprise_role(req: &ServiceRequest) -> EnterpriseRole {
    extract_enterprise_role_from_headers(req.headers())
}

/// Shared extraction logic that works on any `HeaderMap`.
fn extract_enterprise_role_from_headers(
    headers: &actix_web::http::header::HeaderMap,
) -> EnterpriseRole {
    headers
        .get("X-Enterprise-Role")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| {
            serde_json::from_value::<EnterpriseRole>(serde_json::Value::String(s.to_string())).ok()
        })
        .unwrap_or(EnterpriseRole::Contributor)
}

// ---------------------------------------------------------------------------
// Per-handler helper
// ---------------------------------------------------------------------------

/// Checks the enterprise role from request headers and returns a 403
/// `Result<HttpResponse, Error>` (matching the response macros' return type)
/// when the caller's role is insufficient.
///
/// Usage in handlers:
/// ```rust,ignore
/// let role = match require_role(&req, EnterpriseRole::Broker) {
///     Ok(role) => role,
///     Err(resp) => return resp,
/// };
/// ```
pub fn require_role(
    req: &HttpRequest,
    min_role: EnterpriseRole,
) -> Result<EnterpriseRole, Result<HttpResponse, actix_web::Error>> {
    let actual = extract_enterprise_role_from_headers(req.headers());
    let actual_level = role_level(&actual);
    let required_level = role_level(&min_role);

    if actual_level >= required_level {
        debug!(
            "Enterprise role check passed: {:?} (level {}) >= {:?} (level {})",
            actual, actual_level, min_role, required_level
        );
        Ok(actual)
    } else {
        warn!(
            "Enterprise role check FAILED: {:?} (level {}) < {:?} (level {})",
            actual, actual_level, min_role, required_level
        );
        Err(Ok(HttpResponse::Forbidden().json(json!({
            "error": format!(
                "Requires {:?} role or higher",
                min_role
            )
        }))))
    }
}

// ---------------------------------------------------------------------------
// Request extension — stores the extracted role for downstream handlers
// ---------------------------------------------------------------------------

/// Inserted into request extensions by `RequireRole` middleware so handlers
/// can read the verified role without re-parsing headers.
#[derive(Clone, Debug)]
pub struct EnterpriseRoleExt {
    pub role: EnterpriseRole,
}

// ---------------------------------------------------------------------------
// RequireRole middleware (Transform + Service pattern, matching RequireAuth)
// ---------------------------------------------------------------------------

/// Actix-web middleware that enforces a minimum enterprise role.
///
/// Roles are hierarchical: Admin > Broker > Auditor > Contributor.
/// A request is allowed through if the caller's role level is >= the required
/// level. Otherwise, a `403 Forbidden` JSON response is returned.
///
/// When the `nip98-auth` feature is enabled, the middleware reads a NIP-98
/// `Authorization: Nostr <base64>` header, verifies the signature, and
/// resolves the role from the attached `Nip98RoleResolver`. When the feature
/// is disabled, it reads the `X-Enterprise-Role` header (demo mode).
pub struct RequireRole {
    required_role: EnterpriseRole,
    #[cfg(feature = "nip98-auth")]
    role_resolver: Option<std::sync::Arc<dyn Nip98RoleResolver>>,
}

impl RequireRole {
    pub fn new(required_role: EnterpriseRole) -> Self {
        Self {
            required_role,
            #[cfg(feature = "nip98-auth")]
            role_resolver: None,
        }
    }

    /// Attach a NIP-98 role resolver. Only available when `nip98-auth` is
    /// enabled. Without this, the middleware falls back to
    /// `X-Enterprise-Role` header extraction even when the feature is on.
    #[cfg(feature = "nip98-auth")]
    pub fn with_resolver(mut self, resolver: std::sync::Arc<dyn Nip98RoleResolver>) -> Self {
        self.role_resolver = Some(resolver);
        self
    }

    pub fn broker() -> Self {
        Self::new(EnterpriseRole::Broker)
    }

    pub fn admin() -> Self {
        Self::new(EnterpriseRole::Admin)
    }

    pub fn auditor() -> Self {
        Self::new(EnterpriseRole::Auditor)
    }

    pub fn contributor() -> Self {
        Self::new(EnterpriseRole::Contributor)
    }
}

impl<S, B> Transform<S, ServiceRequest> for RequireRole
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: actix_web::body::MessageBody + 'static,
{
    type Response = ServiceResponse<actix_web::body::BoxBody>;
    type Error = Error;
    type InitError = ();
    type Transform = RoleMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(RoleMiddleware {
            service: Rc::new(service),
            required_role: self.required_role.clone(),
            #[cfg(feature = "nip98-auth")]
            role_resolver: self.role_resolver.clone(),
        }))
    }
}

pub struct RoleMiddleware<S> {
    service: Rc<S>,
    required_role: EnterpriseRole,
    #[cfg(feature = "nip98-auth")]
    role_resolver: Option<std::sync::Arc<dyn Nip98RoleResolver>>,
}

impl<S, B> Service<ServiceRequest> for RoleMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: actix_web::body::MessageBody + 'static,
{
    type Response = ServiceResponse<actix_web::body::BoxBody>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let svc = self.service.clone();
        let required = self.required_role.clone();
        #[cfg(feature = "nip98-auth")]
        let resolver = self.role_resolver.clone();

        Box::pin(async move {
            // --- NIP-98 path (feature-gated) ---
            #[cfg(feature = "nip98-auth")]
            {
                if let Some(ref resolver) = resolver {
                    match extract_role_from_nip98(&req, resolver.as_ref()) {
                        Ok((pubkey, actual)) => {
                            let actual_level = role_level(&actual);
                            let required_level = role_level(&required);

                            if actual_level >= required_level {
                                info!(
                                    "Enterprise RBAC (NIP-98): {:?} access granted (required {:?}) for {} [pubkey={}]",
                                    actual, required, req.path(), &pubkey[..16.min(pubkey.len())]
                                );
                                req.extensions_mut().insert(EnterpriseRoleExt {
                                    role: actual.clone(),
                                });
                                req.extensions_mut().insert(Nip98IdentityExt {
                                    pubkey_hex: pubkey,
                                    role: actual,
                                });
                                let resp = svc.call(req).await?;
                                return Ok(resp.map_into_boxed_body());
                            } else {
                                warn!(
                                    "Enterprise RBAC (NIP-98): {:?} DENIED (required {:?}) for {} [pubkey={}]",
                                    actual, required, req.path(), &pubkey[..16.min(pubkey.len())]
                                );
                                let resp = HttpResponse::Forbidden().json(json!({
                                    "error": format!("Requires {:?} role or higher", required),
                                    "your_role": format!("{:?}", actual),
                                    "pubkey": pubkey,
                                }));
                                return Ok(req.into_response(resp).map_into_boxed_body());
                            }
                        }
                        Err(e) => {
                            warn!(
                                "Enterprise RBAC (NIP-98): auth failed for {}: {}",
                                req.path(),
                                e
                            );
                            let resp = HttpResponse::Unauthorized().json(json!({
                                "error": e,
                            }));
                            return Ok(req.into_response(resp).map_into_boxed_body());
                        }
                    }
                }
                // If no resolver is attached, fall through to header-based extraction.
            }

            // --- Legacy header-based path (default) ---
            let actual = extract_enterprise_role(&req);
            let actual_level = role_level(&actual);
            let required_level = role_level(&required);

            if actual_level >= required_level {
                info!(
                    "Enterprise RBAC: {:?} access granted (required {:?}) for {}",
                    actual,
                    required,
                    req.path()
                );

                // Store verified role in extensions for handler use
                req.extensions_mut()
                    .insert(EnterpriseRoleExt { role: actual });

                let resp = svc.call(req).await?;
                Ok(resp.map_into_boxed_body())
            } else {
                warn!(
                    "Enterprise RBAC: {:?} DENIED (required {:?}) for {}",
                    actual,
                    required,
                    req.path()
                );

                let resp = HttpResponse::Forbidden().json(json!({
                    "error": format!("Requires {:?} role or higher", required),
                    "your_role": format!("{:?}", actual),
                }));
                Ok(req.into_response(resp).map_into_boxed_body())
            }
        })
    }
}

// ---------------------------------------------------------------------------
// Convenience extractor for handlers
// ---------------------------------------------------------------------------

/// Retrieves the `EnterpriseRoleExt` from request extensions (populated by
/// the `RequireRole` middleware).
pub fn get_enterprise_role(req: &HttpRequest) -> Option<EnterpriseRoleExt> {
    req.extensions().get::<EnterpriseRoleExt>().cloned()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_levels() {
        assert!(role_level(&EnterpriseRole::Admin) > role_level(&EnterpriseRole::Broker));
        assert!(role_level(&EnterpriseRole::Broker) > role_level(&EnterpriseRole::Auditor));
        assert!(role_level(&EnterpriseRole::Auditor) > role_level(&EnterpriseRole::Contributor));
    }

    #[test]
    fn test_role_level_values() {
        assert_eq!(role_level(&EnterpriseRole::Admin), 4);
        assert_eq!(role_level(&EnterpriseRole::Broker), 3);
        assert_eq!(role_level(&EnterpriseRole::Auditor), 2);
        assert_eq!(role_level(&EnterpriseRole::Contributor), 1);
    }

    #[test]
    fn test_admin_can_access_all() {
        let admin_level = role_level(&EnterpriseRole::Admin);
        assert!(admin_level >= role_level(&EnterpriseRole::Admin));
        assert!(admin_level >= role_level(&EnterpriseRole::Broker));
        assert!(admin_level >= role_level(&EnterpriseRole::Auditor));
        assert!(admin_level >= role_level(&EnterpriseRole::Contributor));
    }

    #[test]
    fn test_contributor_only_contributor() {
        let contrib_level = role_level(&EnterpriseRole::Contributor);
        assert!(contrib_level >= role_level(&EnterpriseRole::Contributor));
        assert!(contrib_level < role_level(&EnterpriseRole::Auditor));
        assert!(contrib_level < role_level(&EnterpriseRole::Broker));
        assert!(contrib_level < role_level(&EnterpriseRole::Admin));
    }

    #[cfg(feature = "nip98-auth")]
    mod nip98_tests {
        use super::super::*;

        #[test]
        fn in_memory_role_map_resolves_registered_pubkey() {
            let mut map = InMemoryRoleMap::new();
            map.insert(
                "aaaa000000000000000000000000000000000000000000000000000000000001",
                EnterpriseRole::Broker,
            );
            assert_eq!(
                map.resolve_role(
                    "aaaa000000000000000000000000000000000000000000000000000000000001"
                ),
                Some(EnterpriseRole::Broker)
            );
        }

        #[test]
        fn in_memory_role_map_returns_none_for_unknown() {
            let map = InMemoryRoleMap::new();
            assert_eq!(
                map.resolve_role(
                    "bbbb000000000000000000000000000000000000000000000000000000000002"
                ),
                None
            );
        }

        #[test]
        fn in_memory_role_map_upsert() {
            let mut map = InMemoryRoleMap::new();
            let pk = "cccc000000000000000000000000000000000000000000000000000000000003";
            map.insert(pk, EnterpriseRole::Contributor);
            assert_eq!(map.resolve_role(pk), Some(EnterpriseRole::Contributor));
            map.insert(pk, EnterpriseRole::Admin);
            assert_eq!(map.resolve_role(pk), Some(EnterpriseRole::Admin));
        }
    }
}
