//! Integration tests for ADR-028-ext: NIP-98 optional auth + caller-aware
//! visibility filter (Sprint A, sovereign-private-node mesh).
//!
//! Scope:
//! - `RequireAuth::optional()` middleware behaviour (anonymous passthrough,
//!   signed verification, malformed rejection) against a minimal actix
//!   service that does not depend on the full `AppState`.
//! - `visibility_allows` filter logic (anonymous-sees-public-only,
//!   signed-sees-own-private, other-users-never-see-private, legacy-no-
//!   visibility-treated-as-public).
//! - Legacy `X-Nostr-Pubkey` + `X-Nostr-Token` path gated by
//!   `APP_ENV=production` (`verify_access` level).
//! - Feature-flag rollback: `NIP98_OPTIONAL_AUTH=false` demotes Optional
//!   to Authenticated so anonymous callers get 401.
//!
//! These tests intentionally avoid `AppState` entirely (see
//! `tests/batch_update_integration_test.rs` for why the full-app path is
//! parked). They exercise the auth plumbing in isolation by mounting the
//! middleware around a dummy handler that echoes the authenticated pubkey.

use actix_web::{
    body::to_bytes,
    http::StatusCode,
    test, web, App, HttpRequest, HttpResponse, Responder,
};
use std::collections::HashMap;
use std::sync::Mutex;

use webxr::handlers::api_handler::graph::visibility_allows;
use webxr::middleware::{get_authenticated_user, RequireAuth};
use webxr::services::nostr_service::NostrService;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Env-var mutator shared across tests that mutate APP_ENV / NIP98_OPTIONAL_AUTH.
/// Rust's test harness runs tests in parallel by default; env-var reads inside
/// `verify_access` happen on every request so tests must serialise their env
/// writes to avoid cross-test interference.
static ENV_LOCK: Mutex<()> = Mutex::new(());

/// RAII env-var guard that restores the previous value on drop.
struct EnvGuard {
    key: &'static str,
    prev: Option<String>,
}

impl EnvGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let prev = std::env::var(key).ok();
        std::env::set_var(key, value);
        Self { key, prev }
    }

    fn unset(key: &'static str) -> Self {
        let prev = std::env::var(key).ok();
        std::env::remove_var(key);
        Self { key, prev }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.prev {
            Some(v) => std::env::set_var(self.key, v),
            None => std::env::remove_var(self.key),
        }
    }
}

/// Dummy handler that echoes the pubkey the auth middleware stored (or an
/// empty string for anonymous Optional passthrough).
async fn echo_pubkey(req: HttpRequest) -> impl Responder {
    let pubkey = get_authenticated_user(&req)
        .map(|u| u.pubkey)
        .unwrap_or_default();
    HttpResponse::Ok().body(pubkey)
}

/// Macro: build a test service with a single `/t/echo` route, wrapped with
/// the caller-specified middleware. Inlining as a macro avoids the complex
/// `impl Future<Output = impl Service>` return type that would otherwise be
/// needed, and sidesteps actix's private `ServiceResponse` generics.
macro_rules! build_test_service {
    ($wrap:expr) => {
        test::init_service(
            App::new()
                .app_data(web::Data::new(NostrService::new()))
                .service(
                    web::scope("/t")
                        .wrap($wrap)
                        .route("/echo", web::get().to(echo_pubkey)),
                ),
        )
        .await
    };
}

/// Extract the response body as a `String`, consuming the service response.
async fn body_string<B>(resp: actix_web::dev::ServiceResponse<B>) -> String
where
    B: actix_web::body::MessageBody,
    <B as actix_web::body::MessageBody>::Error: std::fmt::Debug,
{
    let bytes = to_bytes(resp.into_body()).await.unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

// ---------------------------------------------------------------------------
// visibility_allows: pure-filter unit tests
// ---------------------------------------------------------------------------

#[test]
fn visibility_legacy_row_without_field_is_public() {
    // Legacy row (no `visibility` key) must behave like `public` for backwards
    // compatibility with rows predating ADR-050.
    let meta = HashMap::new();
    assert!(visibility_allows(&meta, None));
    assert!(visibility_allows(&meta, Some("npub_alice")));
}

#[test]
fn visibility_public_allowed_for_everyone() {
    let mut meta = HashMap::new();
    meta.insert("visibility".to_string(), "public".to_string());
    meta.insert("owner_pubkey".to_string(), "npub_bob".to_string());
    assert!(visibility_allows(&meta, None));
    assert!(visibility_allows(&meta, Some("npub_alice")));
    assert!(visibility_allows(&meta, Some("npub_bob")));
}

#[test]
fn visibility_private_hidden_from_anonymous() {
    let mut meta = HashMap::new();
    meta.insert("visibility".to_string(), "private".to_string());
    meta.insert("owner_pubkey".to_string(), "npub_bob".to_string());
    assert!(!visibility_allows(&meta, None));
}

#[test]
fn visibility_private_visible_only_to_owner() {
    let mut meta = HashMap::new();
    meta.insert("visibility".to_string(), "private".to_string());
    meta.insert("owner_pubkey".to_string(), "npub_bob".to_string());
    assert!(visibility_allows(&meta, Some("npub_bob")));
    assert!(!visibility_allows(&meta, Some("npub_alice")));
}

#[test]
fn visibility_private_without_owner_field_is_deny() {
    // Private row missing owner_pubkey is a mis-tagged row; fail closed so
    // corrupt data never leaks.
    let mut meta = HashMap::new();
    meta.insert("visibility".to_string(), "private".to_string());
    assert!(!visibility_allows(&meta, None));
    assert!(!visibility_allows(&meta, Some("npub_alice")));
}

#[test]
fn visibility_unknown_value_fails_closed() {
    // Future visibility values (e.g. "team") must hide the node unless owner
    // matches, so unknown-but-restrictive semantics never become default-open.
    let mut meta = HashMap::new();
    meta.insert("visibility".to_string(), "team-only".to_string());
    meta.insert("owner_pubkey".to_string(), "npub_bob".to_string());
    assert!(!visibility_allows(&meta, Some("npub_alice")));
    assert!(visibility_allows(&meta, Some("npub_bob"))); // owner always sees own
    assert!(!visibility_allows(&meta, None));
}

#[test]
fn visibility_empty_caller_pubkey_treated_as_anonymous() {
    // Defensive: middleware stores "" for anonymous; the handler also does
    // `.filter(|p| !p.is_empty())` before calling, but belt-and-braces the
    // filter itself never lets "" match an owner_pubkey="".
    let mut meta = HashMap::new();
    meta.insert("visibility".to_string(), "private".to_string());
    meta.insert("owner_pubkey".to_string(), "".to_string());
    assert!(!visibility_allows(&meta, Some("")));
}

// ---------------------------------------------------------------------------
// Middleware: RequireAuth::optional() anonymous passthrough
// ---------------------------------------------------------------------------

#[actix_rt::test]
async fn optional_anonymous_no_headers_passes_through_with_empty_pubkey() {
    let _lock = ENV_LOCK.lock().unwrap();
    let _flag = EnvGuard::set("NIP98_OPTIONAL_AUTH", "true");
    let _env = EnvGuard::set("APP_ENV", "development");

    let app = build_test_service!(RequireAuth::optional());
    let req = test::TestRequest::get().uri("/t/echo").to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp).await;
    assert_eq!(
        body, "",
        "anonymous passthrough must echo empty pubkey marker"
    );
}

#[actix_rt::test]
async fn optional_malformed_authorization_returns_401() {
    let _lock = ENV_LOCK.lock().unwrap();
    let _flag = EnvGuard::set("NIP98_OPTIONAL_AUTH", "true");
    let _env = EnvGuard::set("APP_ENV", "development");

    let app = build_test_service!(RequireAuth::optional());
    let req = test::TestRequest::get()
        .uri("/t/echo")
        .insert_header(("Authorization", "Nostr this-is-not-a-valid-token"))
        .to_request();
    let resp = test::call_service(&app, req).await;

    // Malformed NIP-98 must be rejected — Optional only bypasses *missing*
    // headers, not invalid ones. This prevents smuggling garbage past the
    // gate and being treated as anonymous.
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[actix_rt::test]
async fn optional_dev_session_is_honoured() {
    // Dev-mode bypass (Bearer dev-session-token + X-Nostr-Pubkey) should
    // still work behind Optional because it is a deliberate dev-ergonomics
    // escape hatch. In production APP_ENV gate would disable it.
    let _lock = ENV_LOCK.lock().unwrap();
    let _flag = EnvGuard::set("NIP98_OPTIONAL_AUTH", "true");
    let _env = EnvGuard::set("APP_ENV", "development");

    let app = build_test_service!(RequireAuth::optional());
    let req = test::TestRequest::get()
        .uri("/t/echo")
        .insert_header(("Authorization", "Bearer dev-session-token"))
        .insert_header(("X-Nostr-Pubkey", "npub_dev_user"))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_string(resp).await, "npub_dev_user");
}

// ---------------------------------------------------------------------------
// Middleware: RequireAuth::authenticated() still rejects anonymous
// ---------------------------------------------------------------------------

#[actix_rt::test]
async fn authenticated_no_auth_returns_unauthorized_or_forbidden() {
    // Writes (including /api/graph/update, /ontology-agent/*) must reject
    // anonymous callers.
    let _lock = ENV_LOCK.lock().unwrap();
    let _env = EnvGuard::set("APP_ENV", "development");

    let app = build_test_service!(RequireAuth::authenticated());
    let req = test::TestRequest::get().uri("/t/echo").to_request();
    let resp = test::call_service(&app, req).await;

    // `verify_access` returns 403 Forbidden for "missing pubkey header" on
    // the legacy branch; the key invariant is that anonymous is rejected
    // (not 200). Accept either 401 or 403 — both block the request.
    assert!(
        resp.status() == StatusCode::UNAUTHORIZED
            || resp.status() == StatusCode::FORBIDDEN,
        "anonymous must be rejected for authenticated scope, got {}",
        resp.status()
    );
}

// ---------------------------------------------------------------------------
// Legacy X-Nostr-Pubkey path gated by APP_ENV
// ---------------------------------------------------------------------------

#[actix_rt::test]
async fn legacy_header_accepted_in_development() {
    // NostrService session validation will still fail (no session token
    // registered), but the code path must REACH session validation rather
    // than short-circuiting on the production gate. We detect this by
    // asserting the response is 401 (invalid session) and NOT the exact
    // production-gate 401 body.
    let _lock = ENV_LOCK.lock().unwrap();
    let _env = EnvGuard::set("APP_ENV", "development");

    let app = build_test_service!(RequireAuth::authenticated());
    let req = test::TestRequest::get()
        .uri("/t/echo")
        .insert_header(("X-Nostr-Pubkey", "npub_legacy"))
        .insert_header(("X-Nostr-Token", "stale-token"))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let body = body_string(resp).await;
    assert!(
        !body.contains("Legacy session auth not available in production"),
        "dev-mode must not emit the production-gate rejection body"
    );
}

#[actix_rt::test]
async fn legacy_header_rejected_in_production() {
    let _lock = ENV_LOCK.lock().unwrap();
    let _env = EnvGuard::set("APP_ENV", "production");

    let app = build_test_service!(RequireAuth::authenticated());
    let req = test::TestRequest::get()
        .uri("/t/echo")
        .insert_header(("X-Nostr-Pubkey", "npub_legacy"))
        .insert_header(("X-Nostr-Token", "stale-token"))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let body = body_string(resp).await;
    assert!(
        body.contains("Legacy session auth not available in production"),
        "production gate must emit the specific rejection body, got: {}",
        body
    );
}

// ---------------------------------------------------------------------------
// Feature flag: NIP98_OPTIONAL_AUTH rollback lever
// ---------------------------------------------------------------------------

#[actix_rt::test]
async fn flag_disabled_demotes_optional_to_authenticated() {
    // With the flag off, a scope wrapped with `RequireAuth::optional()`
    // must behave exactly like `RequireAuth::authenticated()` — anonymous
    // requests get rejected instead of passing through with an empty
    // pubkey.
    let _lock = ENV_LOCK.lock().unwrap();
    let _flag = EnvGuard::unset("NIP98_OPTIONAL_AUTH");
    let _env = EnvGuard::set("APP_ENV", "development");

    let app = build_test_service!(RequireAuth::optional());
    let req = test::TestRequest::get().uri("/t/echo").to_request();
    let resp = test::call_service(&app, req).await;

    assert!(
        resp.status() == StatusCode::UNAUTHORIZED
            || resp.status() == StatusCode::FORBIDDEN,
        "flag-off Optional must reject anonymous (got {})",
        resp.status()
    );
}

#[actix_rt::test]
async fn flag_enabled_allows_anonymous_through_optional() {
    let _lock = ENV_LOCK.lock().unwrap();
    let _flag = EnvGuard::set("NIP98_OPTIONAL_AUTH", "true");
    let _env = EnvGuard::set("APP_ENV", "development");

    let app = build_test_service!(RequireAuth::optional());
    let req = test::TestRequest::get().uri("/t/echo").to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_string(resp).await, "");
}

// ---------------------------------------------------------------------------
// Combined: visibility filter drives the anonymous-vs-signed view difference
// ---------------------------------------------------------------------------

#[test]
fn anonymous_sees_public_only_signed_sees_own_private() {
    // Simulate three rows across two owners. Verify the filter produces the
    // three end-states required by the sprint's acceptance criteria.
    let mut public_row = HashMap::new();
    public_row.insert("visibility".to_string(), "public".to_string());

    let mut alice_private = HashMap::new();
    alice_private.insert("visibility".to_string(), "private".to_string());
    alice_private.insert("owner_pubkey".to_string(), "npub_alice".to_string());

    let mut bob_private = HashMap::new();
    bob_private.insert("visibility".to_string(), "private".to_string());
    bob_private.insert("owner_pubkey".to_string(), "npub_bob".to_string());

    let rows = [&public_row, &alice_private, &bob_private];

    // Case 1: anonymous — public only.
    let anon_visible: Vec<_> = rows
        .iter()
        .filter(|m| visibility_allows(m, None))
        .collect();
    assert_eq!(anon_visible.len(), 1);
    assert!(std::ptr::eq(*anon_visible[0] as *const _, &public_row as *const _));

    // Case 2: alice — public + own private, never Bob's.
    let alice_visible: Vec<_> = rows
        .iter()
        .filter(|m| visibility_allows(m, Some("npub_alice")))
        .collect();
    assert_eq!(alice_visible.len(), 2);
    assert!(
        alice_visible
            .iter()
            .all(|m| !std::ptr::eq(**m as *const _, &bob_private as *const _)),
        "alice must never see bob's private row"
    );

    // Case 3: bob — public + own private, never Alice's.
    let bob_visible: Vec<_> = rows
        .iter()
        .filter(|m| visibility_allows(m, Some("npub_bob")))
        .collect();
    assert_eq!(bob_visible.len(), 2);
    assert!(
        bob_visible
            .iter()
            .all(|m| !std::ptr::eq(**m as *const _, &alice_private as *const _)),
        "bob must never see alice's private row"
    );
}
