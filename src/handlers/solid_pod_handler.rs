//! Native Solid Pod handler (`solid-pod-rs` backend).
//!
//! ADR-053/056 — dispatches `/solid/*` requests against the external
//! `solid-pod-rs` crate (pinned in Cargo.toml). As of 2026-04-20 this
//! is the sole Pod implementation; the legacy JSS proxy and shadow
//! comparator were retired in the `chore/solid-pod-rs-externalise-jss-cut`
//! commit.

use std::path::PathBuf;
use std::sync::Arc;

use actix_web::http::header::{HeaderName, HeaderValue};
use actix_web::{web, HttpRequest, HttpResponse, Responder};
use log::{debug, error, info, warn};

use solid_pod_rs::auth::nip98;
use solid_pod_rs::ldp;
use solid_pod_rs::storage::fs::FsBackend;
use solid_pod_rs::storage::Storage;
use solid_pod_rs::wac::{
    evaluate_access, method_to_mode, wac_allow_header, AclDocument, AclResolver,
    StorageAclResolver,
};
use solid_pod_rs::PodError;

// ─────────────────────────────────────────────────────────────────────────────
// Service wiring
// ─────────────────────────────────────────────────────────────────────────────

/// Lightweight adapter exposing the crate's LDP helpers as a
/// value-typed "service". The crate itself ships the LDP semantics
/// as free functions + a `LdpContainerOps` trait rather than a
/// `LdpService` struct; this shim lets the Actix-web layer own an
/// `Arc<LdpService>` so it can be cloned into request handlers and
/// later swapped out by tests without touching the crate.
pub struct LdpService<S: Storage> {
    storage: Arc<S>,
}

impl<S: Storage> LdpService<S> {
    pub fn new(storage: Arc<S>) -> Self {
        Self { storage }
    }

    /// Access the underlying `Storage` as a shared pointer.
    pub fn storage(&self) -> Arc<S> {
        self.storage.clone()
    }

    /// Render the appropriate `Link` header values for a path. Thin
    /// forward to [`ldp::link_headers`] so handlers do not need to
    /// pull the free function in separately.
    pub fn link_headers_for(&self, path: &str) -> Vec<String> {
        ldp::link_headers(path)
    }

    /// Compute the WAC access mode required for an HTTP method.
    pub fn required_mode(method: &str) -> solid_pod_rs::wac::AccessMode {
        method_to_mode(method)
    }
}

/// Native Solid service: storage + WAC + LDP glue, constructed once
/// and shared across all requests as `web::Data<Arc<NativeSolidService>>`.
pub struct NativeSolidService {
    pub storage: Arc<FsBackend>,
    pub wac: Arc<StorageAclResolver<FsBackend>>,
    pub ldp: Arc<LdpService<FsBackend>>,
    pub public_base_url: String,
}

impl NativeSolidService {
    /// Build a service from process environment:
    ///
    /// | Env var          | Default                            | Meaning                              |
    /// |------------------|------------------------------------|--------------------------------------|
    /// | `POD_DATA_ROOT`  | `/app/data/solid-pod-rs`           | Filesystem root for the FS backend.  |
    /// | `POD_BASE_URL`   | `https://pods.visionclaw.org`      | Public base URL for `Link` headers.  |
    ///
    /// Returns an `anyhow::Result` so the main binary can propagate
    /// with `?`. Must be called from within a Tokio runtime context
    /// (main awaits it before starting the `HttpServer`).
    pub async fn from_env() -> anyhow::Result<Self> {
        let root = std::env::var("POD_DATA_ROOT")
            .unwrap_or_else(|_| "/app/data/solid-pod-rs".to_string());
        let root_path: PathBuf = root.into();

        // Eager directory creation is handled inside `FsBackend::new`.
        let storage = FsBackend::new(root_path.clone())
            .await
            .map_err(|e| anyhow::anyhow!("FsBackend init failed: {e}"))?;

        let storage = Arc::new(storage);
        let wac = Arc::new(StorageAclResolver::new(storage.clone()));
        let ldp = Arc::new(LdpService::new(storage.clone()));
        let public_base_url = std::env::var("POD_BASE_URL")
            .unwrap_or_else(|_| "https://pods.visionclaw.org".to_string());

        info!(
            "[solid-pod-rs] NativeSolidService ready (root={}, base_url={})",
            root_path.display(),
            public_base_url
        );

        Ok(Self {
            storage,
            wac,
            ldp,
            public_base_url,
        })
    }

    /// Dispatch a raw `(HttpRequest, body)` pair through the native
    /// pipeline and produce the Solid-compliant response.
    ///
    /// Order of operations:
    ///
    /// 1. Extract path + method.
    /// 2. Verify NIP-98 `Authorization` (if supplied) and capture the
    ///    signer pubkey as the WAC agent URI.
    /// 3. Load the effective ACL via `StorageAclResolver` and check
    ///    `method → mode` with `evaluate_access`.
    /// 4. Hand off to the LDP dispatcher (`dispatch_ldp`) which
    ///    returns a fully-formed `HttpResponse` including Solid
    ///    headers (`Link`, `WAC-Allow`, `Accept-Post`, ETag, etc.).
    pub async fn handle_request(
        &self,
        req: &HttpRequest,
        body: web::Bytes,
    ) -> HttpResponse {
        let method = req.method().as_str().to_string();
        let path = extract_solid_path(req);

        // Authenticate (best-effort — anonymous requests are allowed
        // through WAC if the ACL grants `foaf:Agent` access).
        let agent_uri = match verify_nip98(req, &body, &method).await {
            Ok(pk) => Some(derive_webid(&self.public_base_url, &pk)),
            Err(err) => {
                debug!("[solid-pod-rs] NIP-98 verify skipped/failed: {err}");
                None
            }
        };

        // Resolve effective ACL.
        let acl = match self.wac.find_effective_acl(&path).await {
            Ok(doc) => doc,
            Err(e) => {
                error!("[solid-pod-rs] ACL resolution failed for {path}: {e}");
                return pod_error_to_http(&e);
            }
        };

        let required = method_to_mode(&method);
        // F4 (ADR-056): evaluate_access gained a `request_origin` parameter.
        // VisionClaw mounts solid-pod-rs natively over its own auth; we pass
        // None here (origin check intentionally off for this internal path).
        if !evaluate_access(acl.as_ref(), agent_uri.as_deref(), &path, required, None) {
            return forbidden_response(acl.as_ref(), agent_uri.as_deref(), &path);
        }

        match dispatch_ldp(&self.storage, &self.ldp, &method, &path, body).await {
            Ok(mut resp) => {
                attach_wac_allow(
                    &mut resp,
                    acl.as_ref(),
                    agent_uri.as_deref(),
                    &path,
                );
                resp
            }
            Err(e) => pod_error_to_http(&e),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// HTTP method dispatch
// ─────────────────────────────────────────────────────────────────────────────

async fn dispatch_ldp<S: Storage>(
    storage: &Arc<S>,
    ldp_svc: &Arc<LdpService<S>>,
    method: &str,
    path: &str,
    body: web::Bytes,
) -> Result<HttpResponse, PodError> {
    match method.to_uppercase().as_str() {
        "GET" => handle_get(storage, ldp_svc, path).await,
        "HEAD" => handle_head(storage, ldp_svc, path).await,
        "PUT" => handle_put(storage, path, body).await,
        "POST" => handle_post(storage, path, body).await,
        "DELETE" => handle_delete(storage, path).await,
        "PATCH" => handle_patch(storage, path, body).await,
        "OPTIONS" => Ok(options_response(ldp_svc, path)),
        other => {
            warn!("[solid-pod-rs] unsupported method: {other}");
            Ok(HttpResponse::MethodNotAllowed()
                .insert_header(("Allow", "GET, HEAD, PUT, POST, DELETE, PATCH, OPTIONS"))
                .finish())
        }
    }
}

async fn handle_get<S: Storage>(
    storage: &Arc<S>,
    ldp_svc: &Arc<LdpService<S>>,
    path: &str,
) -> Result<HttpResponse, PodError> {
    if ldp::is_container(path) {
        let members = storage.list(path).await?;
        let body = ldp::render_container_turtle(
            path,
            &members,
            ldp::PreferHeader::default(),
        );
        let mut resp = HttpResponse::Ok();
        resp.content_type("text/turtle");
        for l in ldp_svc.link_headers_for(path) {
            resp.insert_header(("Link", l));
        }
        return Ok(resp.body(body));
    }
    let (body, meta) = storage.get(path).await?;
    let mut resp = HttpResponse::Ok();
    resp.content_type(meta.content_type.clone());
    resp.insert_header(("ETag", meta.etag.clone()));
    for l in ldp_svc.link_headers_for(path) {
        resp.insert_header(("Link", l));
    }
    for l in &meta.links {
        resp.insert_header(("Link", l.as_str()));
    }
    Ok(resp.body(body))
}

async fn handle_head<S: Storage>(
    storage: &Arc<S>,
    ldp_svc: &Arc<LdpService<S>>,
    path: &str,
) -> Result<HttpResponse, PodError> {
    let meta = storage.head(path).await?;
    let mut resp = HttpResponse::Ok();
    resp.content_type(meta.content_type.clone());
    resp.insert_header(("ETag", meta.etag.clone()));
    resp.insert_header(("Content-Length", meta.size.to_string()));
    for l in ldp_svc.link_headers_for(path) {
        resp.insert_header(("Link", l));
    }
    for l in &meta.links {
        resp.insert_header(("Link", l.as_str()));
    }
    Ok(resp.finish())
}

async fn handle_put<S: Storage>(
    storage: &Arc<S>,
    path: &str,
    body: web::Bytes,
) -> Result<HttpResponse, PodError> {
    let content_type = "application/octet-stream";
    let meta = storage
        .put(path, bytes::Bytes::from(body.to_vec()), content_type)
        .await?;
    Ok(HttpResponse::Created()
        .insert_header(("ETag", meta.etag))
        .insert_header(("Location", path.to_string()))
        .finish())
}

async fn handle_post<S: Storage>(
    storage: &Arc<S>,
    container: &str,
    body: web::Bytes,
) -> Result<HttpResponse, PodError> {
    let slug = ldp::resolve_slug(container, None);
    let meta = storage
        .put(&slug, bytes::Bytes::from(body.to_vec()), "application/octet-stream")
        .await?;
    Ok(HttpResponse::Created()
        .insert_header(("ETag", meta.etag))
        .insert_header(("Location", slug))
        .finish())
}

async fn handle_delete<S: Storage>(
    storage: &Arc<S>,
    path: &str,
) -> Result<HttpResponse, PodError> {
    storage.delete(path).await?;
    Ok(HttpResponse::NoContent().finish())
}

async fn handle_patch<S: Storage>(
    storage: &Arc<S>,
    path: &str,
    body: web::Bytes,
) -> Result<HttpResponse, PodError> {
    // Minimal PATCH: fetch, parse as N-Triples, apply N3 patch, write back.
    // The crate supports both N3 and SPARQL-Update; we pick by content-type
    // in a future revision (header plumbing lives here).
    let (current, meta) = match storage.get(path).await {
        Ok(v) => v,
        Err(PodError::NotFound(_)) => (
            bytes::Bytes::new(),
            solid_pod_rs::ResourceMeta::new("", 0, "text/turtle"),
        ),
        Err(e) => return Err(e),
    };
    let target = ldp::Graph::parse_ntriples(std::str::from_utf8(&current).unwrap_or(""))?;
    let patch = std::str::from_utf8(&body)
        .map_err(|e| PodError::Nip98(format!("patch utf8: {e}")))?;
    let outcome = ldp::apply_n3_patch(target, patch)?;
    let new_body = outcome.graph.to_ntriples();
    let new_meta = storage
        .put(
            path,
            bytes::Bytes::from(new_body.into_bytes()),
            &meta.content_type,
        )
        .await?;
    Ok(HttpResponse::Ok()
        .insert_header(("ETag", new_meta.etag))
        .finish())
}

fn options_response<S: Storage>(
    ldp_svc: &Arc<LdpService<S>>,
    path: &str,
) -> HttpResponse {
    let mut resp = HttpResponse::Ok();
    resp.insert_header((
        "Allow",
        "GET, HEAD, PUT, POST, DELETE, PATCH, OPTIONS",
    ));
    if ldp::is_container(path) {
        resp.insert_header(("Accept-Post", ldp::ACCEPT_POST));
    }
    for l in ldp_svc.link_headers_for(path) {
        resp.insert_header(("Link", l));
    }
    resp.finish()
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Extract the pod-relative path from the full request. Scopes
/// mounted at `/solid` or `/api/solid` both trigger: the pod path is
/// everything after the first `/solid`.
fn extract_solid_path(req: &HttpRequest) -> String {
    let full = req.path();
    match full.find("/solid") {
        Some(idx) => {
            let tail = &full[idx + "/solid".len()..];
            if tail.is_empty() {
                "/".to_string()
            } else {
                tail.to_string()
            }
        }
        None => "/".to_string(),
    }
}

/// Verify a NIP-98 `Authorization` header using the crate's structural
/// validator. On success, returns the signer pubkey in hex.
async fn verify_nip98(
    req: &HttpRequest,
    body: &[u8],
    method: &str,
) -> Result<String, PodError> {
    let header = req
        .headers()
        .get(actix_web::http::header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| PodError::Nip98("missing Authorization".into()))?;
    let url = req.uri().to_string();
    let body_hash = if body.is_empty() { None } else { Some(body) };
    nip98::verify(header, &url, method, body_hash).await
}

/// Derive a WebID for a signer pubkey under the pod's public base URL.
/// Mirrors the NIP-39 shape used by the JSS proxy
/// (`{base}/{pubkey}/profile/card#me`) so ACLs can be shared across
/// implementations during shadow mode.
fn derive_webid(base_url: &str, pubkey_hex: &str) -> String {
    format!(
        "{}/{}/profile/card#me",
        base_url.trim_end_matches('/'),
        pubkey_hex
    )
}

/// Compute the Solid `WAC-Allow` header and attach it to a response.
fn attach_wac_allow(
    resp: &mut HttpResponse,
    acl: Option<&AclDocument>,
    agent_uri: Option<&str>,
    path: &str,
) {
    let value = wac_allow_header(acl, agent_uri, path);
    if let (Ok(name), Ok(val)) = (
        HeaderName::from_bytes(b"WAC-Allow"),
        HeaderValue::from_str(&value),
    ) {
        resp.headers_mut().insert(name, val);
    }
}

/// Canonical 403 response with `WAC-Allow` so clients can discover
/// the effective modes without a follow-up request.
fn forbidden_response(
    acl: Option<&AclDocument>,
    agent_uri: Option<&str>,
    path: &str,
) -> HttpResponse {
    HttpResponse::Forbidden()
        .insert_header(("WAC-Allow", wac_allow_header(acl, agent_uri, path)))
        .finish()
}

// ─────────────────────────────────────────────────────────────────────────────
// PodError → HttpResponse
// ─────────────────────────────────────────────────────────────────────────────

/// Convert a `PodError` to the Solid-idiomatic HTTP response. Kept as
/// a free function so call sites don't need a trait import in scope.
fn pod_error_to_http(err: &PodError) -> HttpResponse {
    match err {
        PodError::NotFound(_) => HttpResponse::NotFound().finish(),
        PodError::AlreadyExists(_) => HttpResponse::Conflict().finish(),
        PodError::Forbidden => HttpResponse::Forbidden().finish(),
        PodError::Unauthenticated => HttpResponse::Unauthorized().finish(),
        PodError::InvalidPath(p) => {
            HttpResponse::BadRequest().body(format!("invalid path: {p}"))
        }
        PodError::PreconditionFailed(msg) => {
            HttpResponse::PreconditionFailed().body(msg.to_string())
        }
        PodError::Unsupported(msg) => {
            HttpResponse::NotImplemented().body(msg.to_string())
        }
        _ => HttpResponse::InternalServerError().body(err.to_string()),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Actix handler + route configuration
// ─────────────────────────────────────────────────────────────────────────────

/// Single-handler entry point. Mounted in [`configure_routes`] for
/// every HTTP method on `/solid/{tail:.*}`.
pub async fn handle_native_solid(
    req: HttpRequest,
    body: web::Bytes,
    svc: web::Data<Arc<NativeSolidService>>,
) -> impl Responder {
    svc.handle_request(&req, body).await
}

/// Mount the native Solid service at `/solid/*`. The scope nests
/// cleanly under `/api` (e.g. `/api/solid/alice/public/...`).
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    info!("=== REGISTERING SOLID-POD-RS NATIVE ROUTES ===");
    cfg.service(
        web::scope("/solid").route(
            "/{path:.*}",
            web::route().to(handle_native_solid),
        ),
    );
}

