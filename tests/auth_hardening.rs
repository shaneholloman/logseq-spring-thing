//! Integration tests for ADR-055 auth hardening (QE audit 2026-04-19, B3 + H3).
//!
//! Scope:
//! - **B3** — NIP-98 body-hash binding. The verifier now receives the raw
//!   bytes of the request body via `verify_access_with_body` and
//!   `extract_user_identity(.., Some(body))`, so a captured token cannot be
//!   replayed against a different payload. Pre-fix, both call sites passed
//!   `None` and any body would pass through silently.
//! - **H3** — `APP_ENV` fail-closed default. The dev-mode bypass and legacy
//!   X-Nostr-Pubkey path now treat an *unset* `APP_ENV` as production,
//!   closing the previous "missing = non-production = dev-bypass active"
//!   door. `APP_ENV=development` remains the only way to unlock dev paths.
//!
//! Env-var manipulation is serialised through a process-wide mutex because
//! Rust's test harness runs tests in parallel and both `APP_ENV` and
//! `NIP98_OPTIONAL_AUTH` are read inside `verify_access` on every request.
//! The `serial_test` crate is not in our dev-dependencies, so the mutex +
//! RAII `EnvGuard` idiom (copied from `tests/auth_sovereign_mesh.rs`) is
//! the isolation mechanism.

use actix_web::{
    body::to_bytes,
    http::StatusCode,
    test, web, App, HttpRequest, HttpResponse, Responder,
};
use nostr_sdk::prelude::Keys;
use std::sync::Mutex;

use webxr::middleware::{get_authenticated_user, RequireAuth};
use webxr::services::nostr_service::NostrService;
use webxr::utils::nip98::{build_auth_header, generate_nip98_token, Nip98Config};

// ---------------------------------------------------------------------------
// Shared env-var serialisation (no serial_test crate available)
// ---------------------------------------------------------------------------

static ENV_LOCK: Mutex<()> = Mutex::new(());

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

// ---------------------------------------------------------------------------
// B3 — NIP-98 body-hash binding (via NostrService::verify_nip98_auth direct)
// ---------------------------------------------------------------------------
//
// These exercise the verifier contract directly: a token signed over body X
// must reject body Y, accept body X, and accept an empty-body POST when the
// token was signed for an empty body. The call sites in `verify_access` and
// `extract_user_identity` now forward the buffered body to this verifier,
// so passing the contract here is what closes the replay gap.

fn url() -> &'static str {
    "http://localhost:3030/api/graph/update"
}

#[actix_rt::test]
async fn b3a_tampered_body_is_rejected() {
    // Client signed {"x":1}; attacker replays the same token with
    // {"x":999}. The payload-hash check must fire.
    let keys = Keys::generate();
    let signed_body = r#"{"x":1}"#;
    let tampered_body = r#"{"x":999}"#;

    let token = generate_nip98_token(
        &keys,
        &Nip98Config {
            url: url().to_string(),
            method: "POST".to_string(),
            body: Some(signed_body.to_string()),
        },
    )
    .expect("token generation");
    let auth_header = build_auth_header(&token);

    let svc = NostrService::new();
    let result = svc
        .verify_nip98_auth(&auth_header, url(), "POST", Some(tampered_body))
        .await;

    assert!(
        result.is_err(),
        "tampered body must fail NIP-98 payload-hash binding"
    );
}

#[actix_rt::test]
async fn b3b_matching_body_is_accepted() {
    // Control: same body in, same body out — must authenticate cleanly.
    let keys = Keys::generate();
    let body = r#"{"x":1}"#;

    let token = generate_nip98_token(
        &keys,
        &Nip98Config {
            url: url().to_string(),
            method: "POST".to_string(),
            body: Some(body.to_string()),
        },
    )
    .expect("token generation");
    let auth_header = build_auth_header(&token);

    let svc = NostrService::new();
    let result = svc
        .verify_nip98_auth(&auth_header, url(), "POST", Some(body))
        .await;

    assert!(
        result.is_ok(),
        "matching body must authenticate: {:?}",
        result.err()
    );
}

#[actix_rt::test]
async fn b3c_empty_body_post_is_accepted() {
    // POST with empty body + token signed for empty body must succeed.
    // The `extract_user_identity(.., Some(&[]))` path feeds `Some("")` into
    // the verifier; the client-side token carries sha256(b"") in its
    // payload tag.
    let keys = Keys::generate();
    let empty = "";

    let token = generate_nip98_token(
        &keys,
        &Nip98Config {
            url: url().to_string(),
            method: "POST".to_string(),
            body: Some(empty.to_string()),
        },
    )
    .expect("token generation");
    let auth_header = build_auth_header(&token);

    let svc = NostrService::new();
    let result = svc
        .verify_nip98_auth(&auth_header, url(), "POST", Some(empty))
        .await;

    assert!(
        result.is_ok(),
        "empty-body POST with matching token must authenticate: {:?}",
        result.err()
    );
}

// ---------------------------------------------------------------------------
// H3 — APP_ENV fail-closed default (via RequireAuth middleware)
// ---------------------------------------------------------------------------
//
// The reference invariant: the legacy `X-Nostr-Pubkey` + `X-Nostr-Token`
// branch in `verify_access` is only reachable when APP_ENV is explicitly
// "development". Before H3, an unset APP_ENV left that branch active,
// exposing an unsigned bearer-style auth flow to production traffic.
//
// We assert the rejection *body text* — the production gate emits a
// specific string ("Legacy session auth not available in production") that
// dev-mode must not emit and production-mode (and unset-mode) must.

async fn echo_pubkey(req: HttpRequest) -> impl Responder {
    let pubkey = get_authenticated_user(&req)
        .map(|u| u.pubkey)
        .unwrap_or_default();
    HttpResponse::Ok().body(pubkey)
}

async fn body_string<B>(resp: actix_web::dev::ServiceResponse<B>) -> String
where
    B: actix_web::body::MessageBody,
    <B as actix_web::body::MessageBody>::Error: std::fmt::Debug,
{
    let bytes = to_bytes(resp.into_body()).await.unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[actix_rt::test]
async fn h3a_unset_app_env_is_production_mode() {
    // Unset APP_ENV must behave like APP_ENV=production — the legacy path
    // must be rejected with the production-gate body.
    let _lock = ENV_LOCK.lock().unwrap();
    let _env = EnvGuard::unset("APP_ENV");
    let _flag = EnvGuard::unset("NIP98_OPTIONAL_AUTH");

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(NostrService::new()))
            .service(
                web::scope("/t")
                    .wrap(RequireAuth::authenticated())
                    .route("/echo", web::get().to(echo_pubkey)),
            ),
    )
    .await;
    let req = test::TestRequest::get()
        .uri("/t/echo")
        .insert_header(("X-Nostr-Pubkey", "npub_legacy"))
        .insert_header(("X-Nostr-Token", "stale-token"))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "unset APP_ENV must reject legacy path (401)"
    );
    let body = body_string(resp).await;
    assert!(
        body.contains("Legacy session auth not available in production"),
        "unset APP_ENV must emit the production-gate rejection body, got: {}",
        body
    );
}

#[actix_rt::test]
async fn h3b_development_accepts_legacy_path() {
    // APP_ENV=development is the only way to unlock the legacy branch.
    // Session validation still fails (no session registered) so the
    // response is 401, but critically NOT the production-gate body —
    // that proves the code reached session validation rather than
    // short-circuiting at the gate.
    let _lock = ENV_LOCK.lock().unwrap();
    let _env = EnvGuard::set("APP_ENV", "development");
    let _flag = EnvGuard::unset("NIP98_OPTIONAL_AUTH");

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(NostrService::new()))
            .service(
                web::scope("/t")
                    .wrap(RequireAuth::authenticated())
                    .route("/echo", web::get().to(echo_pubkey)),
            ),
    )
    .await;
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
        "development mode must not emit the production-gate rejection body, got: {}",
        body
    );
}

#[actix_rt::test]
async fn h3c_production_rejects_legacy_path() {
    // Explicit APP_ENV=production continues to reject — behaviour
    // preservation check so the flip to fail-closed doesn't regress the
    // already-correct production path.
    let _lock = ENV_LOCK.lock().unwrap();
    let _env = EnvGuard::set("APP_ENV", "production");
    let _flag = EnvGuard::unset("NIP98_OPTIONAL_AUTH");

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(NostrService::new()))
            .service(
                web::scope("/t")
                    .wrap(RequireAuth::authenticated())
                    .route("/echo", web::get().to(echo_pubkey)),
            ),
    )
    .await;
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
        "production must emit the specific rejection body, got: {}",
        body
    );
}
