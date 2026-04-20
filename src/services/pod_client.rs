//! Pod Client — NIP-98-signed HTTP client for Solid Pod resources.
//!
//! Part of ADR-051 Pod-first-Neo4j-second ingest saga.
//!
//! Provides a thin wrapper around `reqwest::Client` that:
//! - Signs each request with a fresh NIP-98 event bound to (url, method, payload-hash)
//! - Supports the core LDP verbs needed by the saga: PUT, DELETE, MOVE, and HEAD (for ETag)
//! - Uses the server's Nostr identity unless a per-caller key is passed through
//!
//! The server Nostr identity comes from the sibling agent's `ServerIdentity` once merged;
//! until then this module reads `SERVER_NOSTR_PRIVKEY` from the environment directly and
//! constructs a `Keys` on each request (cheap — just a hex decode).

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use log::{debug, warn};
use nostr_sdk::{Keys, SecretKey};
use reqwest::Client;
use thiserror::Error;

use crate::utils::nip98::{build_auth_header, generate_nip98_token, Nip98Config, Nip98Error};

/// Default HTTP timeout per Pod request.
const DEFAULT_POD_TIMEOUT_SECS: u64 = 15;

/// Environment variable holding the server's Nostr private key (64 hex chars).
///
/// Kept as a plain env var (not a file) because the saga runs in the same
/// process as the rest of the backend and must not require extra bootstrap.
/// If/when the sibling `ServerIdentity` lands, `PodClient::with_server_keys`
/// wires it in and this fallback becomes dead weight.
pub const SERVER_NOSTR_PRIVKEY_ENV: &str = "SERVER_NOSTR_PRIVKEY";

/// Errors from Pod operations.
#[derive(Debug, Error)]
pub enum PodClientError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("NIP-98 signing error: {0}")]
    Nip98(#[from] Nip98Error),

    #[error("Server identity not configured: set {0} env var")]
    NoServerIdentity(&'static str),

    #[error("Invalid server Nostr privkey: {0}")]
    InvalidPrivkey(String),

    #[error("Pod returned status {status} for {method} {url}: {body}")]
    Status {
        method: String,
        url: String,
        status: u16,
        body: String,
    },

    #[error("Invalid URL: {0}")]
    InvalidUrl(String),
}

pub type PodResult<T> = Result<T, PodClientError>;

/// Response metadata from a successful Pod write.
#[derive(Debug, Clone)]
pub struct PodResponse {
    pub status: u16,
    pub etag: Option<String>,
    pub location: Option<String>,
}

/// NIP-98-signed Pod HTTP client.
///
/// Cheap to clone — the inner `reqwest::Client` is pooled and `Keys` are
/// an `Arc`-ish handle under the hood.
#[derive(Clone)]
pub struct PodClient {
    http: Client,
    /// Server-side signing keys. `None` forces per-call `auth_header` overrides.
    server_keys: Option<Arc<Keys>>,
}

impl PodClient {
    /// Build a client that signs requests with the server's Nostr identity,
    /// pulled from `SERVER_NOSTR_PRIVKEY` env. Returns `Err` only if the env
    /// var is set but malformed — an absent env var is deferred until first
    /// unsigned call (so the client can still be constructed for shim/test
    /// scenarios that override the auth header per call).
    pub fn from_env() -> PodResult<Self> {
        let http = Client::builder()
            .timeout(Duration::from_secs(DEFAULT_POD_TIMEOUT_SECS))
            // Do not follow redirects — Pods should return 200/201/204 directly.
            .redirect(reqwest::redirect::Policy::none())
            .build()?;

        let server_keys = match std::env::var(SERVER_NOSTR_PRIVKEY_ENV) {
            Ok(hex) if !hex.is_empty() => {
                let sk = SecretKey::from_hex(&hex)
                    .map_err(|e| PodClientError::InvalidPrivkey(e.to_string()))?;
                Some(Arc::new(Keys::new(sk)))
            }
            _ => {
                debug!("PodClient::from_env: {} not set; calls must supply auth_header", SERVER_NOSTR_PRIVKEY_ENV);
                None
            }
        };

        Ok(Self { http, server_keys })
    }

    /// Build a client with an explicit reqwest::Client (for tests that want
    /// a custom timeout/base URL shim) and optional server keys.
    pub fn new(http: Client, server_keys: Option<Arc<Keys>>) -> Self {
        Self { http, server_keys }
    }

    /// Replace server keys — used once the sibling `ServerIdentity` agent is merged.
    pub fn with_server_keys(mut self, keys: Arc<Keys>) -> Self {
        self.server_keys = Some(keys);
        self
    }

    /// Compute a NIP-98 auth header bound to the given request, using the
    /// server keys. Returns `Err` if no keys are configured.
    fn sign_with_server_keys(&self, method: &str, url: &str, body: Option<&str>) -> PodResult<String> {
        let keys = self
            .server_keys
            .as_ref()
            .ok_or(PodClientError::NoServerIdentity(SERVER_NOSTR_PRIVKEY_ENV))?;
        let config = Nip98Config {
            url: url.to_string(),
            method: method.to_string(),
            body: body.map(str::to_string),
        };
        let token = generate_nip98_token(keys, &config)?;
        Ok(build_auth_header(&token))
    }

    /// Resolve an auth header: use caller-provided one if given, else sign with server keys.
    fn resolve_auth(
        &self,
        method: &str,
        url: &str,
        body: Option<&str>,
        override_auth: Option<&str>,
    ) -> PodResult<String> {
        match override_auth {
            Some(h) => Ok(h.to_string()),
            None => self.sign_with_server_keys(method, url, body),
        }
    }

    /// PUT a resource, replacing any existing one. Idempotent: the saga
    /// relies on retries producing the same ETag if content is unchanged.
    ///
    /// If `auth_header` is `Some`, that value is used verbatim (this is the
    /// user-signing path, for acting on the user's own container). If `None`,
    /// the server keys are used to sign.
    pub async fn put_resource(
        &self,
        pod_url: &str,
        content: Bytes,
        content_type: &str,
        auth_header: Option<&str>,
    ) -> PodResult<PodResponse> {
        // Body hash must match the bytes we actually send. `generate_nip98_token`
        // hashes the `body` string as UTF-8, so for text content types we pass
        // the body through; for binary we pass the base64 hash-input form.
        // Simpler and correct: always hash the raw bytes via the same SHA256
        // pipeline used by compute_payload_hash (which lives in nip98.rs as
        // private). Since that helper is private, reuse `body: Some(<utf8>)`
        // for text payloads and `body: None` for binary (the NIP-98 spec makes
        // payload tag optional — JSS accepts absent `payload` for binary
        // uploads when signature is otherwise valid).
        let body_for_sign = std::str::from_utf8(&content).ok().map(str::to_string);

        let auth = self.resolve_auth("PUT", pod_url, body_for_sign.as_deref(), auth_header)?;

        debug!("[pod_client] PUT {} ({} bytes, {})", pod_url, content.len(), content_type);

        let resp = self
            .http
            .put(pod_url)
            .header("Authorization", auth)
            .header("Content-Type", content_type)
            .body(content.clone())
            .send()
            .await?;

        let status = resp.status();
        let etag = resp
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let location = resp
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);

        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(PodClientError::Status {
                method: "PUT".into(),
                url: pod_url.into(),
                status: status.as_u16(),
                body,
            });
        }

        Ok(PodResponse { status: status.as_u16(), etag, location })
    }

    /// DELETE a resource.
    pub async fn delete_resource(&self, pod_url: &str, auth_header: Option<&str>) -> PodResult<()> {
        let auth = self.resolve_auth("DELETE", pod_url, None, auth_header)?;

        debug!("[pod_client] DELETE {}", pod_url);

        let resp = self
            .http
            .delete(pod_url)
            .header("Authorization", auth)
            .send()
            .await?;

        if !resp.status().is_success() && resp.status().as_u16() != 404 {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(PodClientError::Status {
                method: "DELETE".into(),
                url: pod_url.into(),
                status,
                body,
            });
        }

        Ok(())
    }

    /// MOVE a resource from `from` to `to`. LDP-style MOVE via `Destination`
    /// header (matches JSS behaviour). Used by publish/unpublish flows.
    pub async fn move_resource(
        &self,
        from: &str,
        to: &str,
        auth_header: Option<&str>,
    ) -> PodResult<()> {
        let auth = self.resolve_auth("MOVE", from, None, auth_header)?;

        debug!("[pod_client] MOVE {} -> {}", from, to);

        // Custom method — reqwest supports MOVE via `request(Method, url)`.
        let method = reqwest::Method::from_bytes(b"MOVE")
            .map_err(|_| PodClientError::InvalidUrl("Failed to construct MOVE method".into()))?;

        let resp = self
            .http
            .request(method, from)
            .header("Authorization", auth)
            .header("Destination", to)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(PodClientError::Status {
                method: "MOVE".into(),
                url: from.into(),
                status,
                body,
            });
        }

        Ok(())
    }

    /// GET a resource. Returns `Ok(None)` on 404 (the caller decides whether
    /// to create-on-miss). Returns the raw body on 2xx with the declared
    /// `Content-Type` as a best-effort string.
    ///
    /// Used by ADR-029 Type Index discovery to fetch remote WebID profile
    /// documents and `publicTypeIndex.jsonld` from peer Pods. For read-only
    /// discovery against public containers, pass `auth_header: None` — the
    /// server keys signature is sufficient since the resource is world-readable
    /// under WAC (ADR-052). For user-scoped reads, the caller supplies an
    /// explicit NIP-98 header signed with the user's keys.
    pub async fn get_resource(
        &self,
        pod_url: &str,
        accept: Option<&str>,
        auth_header: Option<&str>,
    ) -> PodResult<Option<(String, Option<String>)>> {
        let auth = self.resolve_auth("GET", pod_url, None, auth_header)?;

        debug!("[pod_client] GET {}", pod_url);

        let mut req = self.http.get(pod_url).header("Authorization", auth);
        if let Some(a) = accept {
            req = req.header("Accept", a);
        }
        let resp = req.send().await?;

        let status = resp.status();
        if status.as_u16() == 404 {
            return Ok(None);
        }
        if !status.is_success() {
            let code = status.as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(PodClientError::Status {
                method: "GET".into(),
                url: pod_url.into(),
                status: code,
                body,
            });
        }

        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let body = resp.text().await.unwrap_or_default();
        Ok(Some((body, content_type)))
    }

    /// HEAD a resource and return its ETag (if present). Returns `Ok(None)`
    /// when the resource does not exist (404) — callers use this to decide
    /// whether to skip a redundant PUT on saga replay.
    pub async fn get_etag(
        &self,
        pod_url: &str,
        auth_header: Option<&str>,
    ) -> PodResult<Option<String>> {
        let auth = self.resolve_auth("HEAD", pod_url, None, auth_header)?;

        let resp = self
            .http
            .head(pod_url)
            .header("Authorization", auth)
            .send()
            .await?;

        if resp.status().as_u16() == 404 {
            return Ok(None);
        }
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            warn!("[pod_client] HEAD {} returned {} — treating as unknown", pod_url, status);
            return Err(PodClientError::Status {
                method: "HEAD".into(),
                url: pod_url.into(),
                status,
                body,
            });
        }

        let etag = resp
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        Ok(etag)
    }
}

/// Compute the Pod URL for a KG node given the Pod base URL, the owner's
/// Nostr npub (bech32) or pubkey hex, the node's slug, and its visibility.
///
/// Layout follows the sovereign-schema convention:
///   Public:  `{pod_base}/{owner}/public/kg/{slug}`
///   Private: `{pod_base}/{owner}/private/kg/{slug}`
///
/// The helper keeps slugs URL-safe by replacing spaces with underscores and
/// stripping characters that would break Solid/LDP paths. More aggressive
/// slugging (e.g. collision-safe hashing) belongs to the parser agent.
pub fn pod_url_for(pod_base: &str, owner: &str, slug: &str, visibility: Visibility) -> String {
    let container = match visibility {
        Visibility::Public => "public",
        Visibility::Private => "private",
    };
    let base = pod_base.trim_end_matches('/');
    let owner = owner.trim_matches('/');
    let safe_slug = sanitise_slug(slug);
    format!("{base}/{owner}/{container}/kg/{safe_slug}")
}

/// Visibility of a KG node — drives Pod container routing.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Private,
}

impl Visibility {
    pub fn from_str(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "public" | "true" | "1" => Visibility::Public,
            _ => Visibility::Private,
        }
    }
}

/// Minimal URL-path slug sanitiser. We do not lowercase (case may be semantic
/// in the upstream slug); we only replace structurally-unsafe characters.
pub fn sanitise_slug(slug: &str) -> String {
    let mut out = String::with_capacity(slug.len());
    for ch in slug.chars() {
        match ch {
            ' ' => out.push('_'),
            '/' | '\\' | '#' | '?' | '&' | '%' => out.push('-'),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    if out.is_empty() {
        out.push_str("_unnamed");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pod_url_for_public() {
        let url = pod_url_for(
            "https://pod.example.org",
            "npub1abc",
            "Photosynthesis",
            Visibility::Public,
        );
        assert_eq!(url, "https://pod.example.org/npub1abc/public/kg/Photosynthesis");
    }

    #[test]
    fn test_pod_url_for_private_strips_trailing_slash() {
        let url = pod_url_for(
            "https://pod.example.org/",
            "/npub1xyz/",
            "Recipe Book",
            Visibility::Private,
        );
        assert_eq!(url, "https://pod.example.org/npub1xyz/private/kg/Recipe_Book");
    }

    #[test]
    fn test_sanitise_slug_replaces_unsafe() {
        assert_eq!(sanitise_slug("Bob's Plants/2024"), "Bob's_Plants-2024");
        assert_eq!(sanitise_slug(""), "_unnamed");
    }

    #[test]
    fn test_visibility_from_str() {
        assert_eq!(Visibility::from_str("public"), Visibility::Public);
        assert_eq!(Visibility::from_str("Private"), Visibility::Private);
        assert_eq!(Visibility::from_str("true"), Visibility::Public);
        assert_eq!(Visibility::from_str(""), Visibility::Private);
    }
}
