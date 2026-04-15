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
//! **Current implementation**: reads `X-Enterprise-Role` header.
//! This will be replaced with OIDC JWT claim extraction (ADR-040 Phase 2).

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
pub struct RequireRole {
    required_role: EnterpriseRole,
}

impl RequireRole {
    pub fn new(required_role: EnterpriseRole) -> Self {
        Self { required_role }
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
        }))
    }
}

pub struct RoleMiddleware<S> {
    service: Rc<S>,
    required_role: EnterpriseRole,
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

        Box::pin(async move {
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
                req.extensions_mut().insert(EnterpriseRoleExt {
                    role: actual,
                });

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
}
