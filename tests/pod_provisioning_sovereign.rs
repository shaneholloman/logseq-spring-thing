//! Integration tests for ADR-052 — default-private Pod provisioning,
//! 3+1 container layout, double-gated writes, and the startup migration.
//!
//! These tests avoid `AppState`. They compile against the public functions
//! exposed from `webxr::handlers::solid_proxy_handler` and the migration
//! module, plus a minimal in-process mock JSS built on actix-web that
//! captures every PUT the handler issues.

use actix_web::{test, web, App, HttpRequest, HttpResponse};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use webxr::handlers::solid_proxy_handler::{
    body_marks_public, create_pod_if_missing_for_tests, derive_webid,
    evaluate_double_gate, pod_default_private_enabled, render_owner_only_acl,
    render_profile_container_acl, render_public_container_acl, render_webid_card,
    DoubleGateDecision, SolidProxyState,
};
use webxr::handlers::solid_proxy_migration::{
    acl_is_sovereign, migrate_pod_acl, run_migration, MigrateOutcome,
};

// ─────────────────────────────────────────────────────────────────────────────
// Env guard (tests mutate POD_DEFAULT_PRIVATE + JSS_URL)
// ─────────────────────────────────────────────────────────────────────────────

static ENV_LOCK: Mutex<()> = Mutex::new(());

struct EnvGuard {
    prev: Vec<(&'static str, Option<String>)>,
}

impl EnvGuard {
    fn new() -> Self {
        Self { prev: Vec::new() }
    }
    fn set(&mut self, key: &'static str, value: &str) {
        self.prev.push((key, std::env::var(key).ok()));
        std::env::set_var(key, value);
    }
    fn unset(&mut self, key: &'static str) {
        self.prev.push((key, std::env::var(key).ok()));
        std::env::remove_var(key);
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (k, v) in self.prev.drain(..) {
            match v {
                Some(val) => std::env::set_var(k, val),
                None => std::env::remove_var(k),
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Mock JSS backend: captures PUT bodies into a shared map keyed by request path.
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Default, Clone)]
struct JssCapture {
    puts: Arc<Mutex<HashMap<String, String>>>,
}

async fn jss_handle(
    req: HttpRequest,
    body: web::Bytes,
    state: web::Data<JssCapture>,
) -> HttpResponse {
    let path = req.uri().path().to_string();
    if req.method() == "PUT" {
        let text = String::from_utf8_lossy(&body).to_string();
        state.puts.lock().unwrap().insert(path, text);
        return HttpResponse::Created().finish();
    }
    if req.method() == "HEAD" {
        // Pretend the Pod root does NOT exist so provisioning runs.
        return HttpResponse::NotFound().finish();
    }
    HttpResponse::Ok().finish()
}

async fn start_mock_jss() -> (JssCapture, String, actix_web::dev::ServerHandle) {
    let capture = JssCapture::default();
    let capture_cloned = capture.clone();
    let server = actix_web::HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(capture_cloned.clone()))
            .default_service(web::to(jss_handle))
    })
    .bind("127.0.0.1:0")
    .expect("bind mock jss")
    .workers(1);

    let addrs = server.addrs();
    let addr = addrs.first().copied().expect("mock jss addr");
    let running = server.run();
    let handle = running.handle();
    tokio::spawn(running);
    let base = format!("http://{}", addr);
    // Brief pause for listener readiness (no long sleep — best-effort yield).
    tokio::task::yield_now().await;
    (capture, base, handle)
}

fn pubkey_hex() -> &'static str {
    // 32-byte hex pubkey — arbitrary but deterministic.
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
}

fn npub_stub() -> &'static str {
    // We use the hex as the "npub" path segment for test purposes — the
    // handler treats it as an opaque identifier for URL building.
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
}

// ─────────────────────────────────────────────────────────────────────────────
// Template rendering — zero-network tests
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn owner_only_acl_has_no_foaf_agent() {
    let acl = render_owner_only_acl("https://pods.visionclaw.org/abc/profile/card#me");
    assert!(acl.contains("acl:agent <https://pods.visionclaw.org/abc/profile/card#me>"));
    assert!(acl.contains("acl:Read, acl:Write, acl:Control"));
    assert!(!acl.contains("foaf:Agent"));
    assert!(!acl.contains("agentClass"));
    assert!(acl.contains("acl:default <./>"));
}

#[test]
fn public_acl_has_public_read_and_owner_write() {
    let acl = render_public_container_acl("https://pods.visionclaw.org/abc/profile/card#me");
    assert!(acl.contains("acl:agentClass foaf:Agent"));
    assert!(acl.contains("<#publicRead>"));
    assert!(acl.contains("<#ownerWrite>"));
    assert!(acl.contains("acl:Read, acl:Write, acl:Control"));
}

#[test]
fn profile_acl_restricts_public_to_card() {
    let acl = render_profile_container_acl("https://pods.visionclaw.org/abc/profile/card#me");
    assert!(acl.contains("acl:accessTo <./card>"));
    assert!(acl.contains("<#publicReadCard>"));
}

#[test]
fn webid_card_carries_nip39_claim() {
    let card = render_webid_card(pubkey_hex());
    assert!(card.contains(&format!("nostr:hasPubkey \"{}\"", pubkey_hex())));
    assert!(card.contains("a foaf:Person"));
}

#[test]
fn derive_webid_uses_pod_base_url_when_set() {
    let _lock = ENV_LOCK.lock().unwrap();
    let mut guard = EnvGuard::new();
    guard.set("POD_BASE_URL", "https://example.test");
    let wid = derive_webid(pubkey_hex());
    assert_eq!(
        wid,
        format!("https://example.test/{}/profile/card#me", pubkey_hex())
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Double-gate evaluation
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn double_gate_not_applicable_outside_public_kg() {
    assert_eq!(
        evaluate_double_gate("private/kg/foo.ttl", b"hello", None),
        DoubleGateDecision::NotApplicable
    );
    assert_eq!(
        evaluate_double_gate("profile/card", b"", None),
        DoubleGateDecision::NotApplicable
    );
}

#[test]
fn double_gate_deny_when_no_assertion() {
    assert_eq!(
        evaluate_double_gate("public/kg/foo.ttl", b"private note", None),
        DoubleGateDecision::Deny
    );
}

#[test]
fn double_gate_allow_when_header_asserts_public() {
    assert_eq!(
        evaluate_double_gate("public/kg/foo.ttl", b"", Some("public")),
        DoubleGateDecision::Allow
    );
}

#[test]
fn double_gate_allow_when_body_marks_public() {
    let body = b"title:: Demo\npublic:: true\n\nBody text.";
    assert_eq!(
        evaluate_double_gate("public/kg/foo.ttl", body, None),
        DoubleGateDecision::Allow
    );
}

#[test]
fn body_marks_public_accepts_common_variants() {
    assert!(body_marks_public(b"public:: true"));
    assert!(body_marks_public(b"public:: \"true\""));
    assert!(body_marks_public(b"\"public\": true"));
    assert!(!body_marks_public(b"public:: false"));
    assert!(!body_marks_public(b""));
}

// ─────────────────────────────────────────────────────────────────────────────
// Provisioning round-trip against mock JSS
// ─────────────────────────────────────────────────────────────────────────────

#[actix_web::test]
async fn provision_flag_off_preserves_legacy_layout() {
    let _lock = ENV_LOCK.lock().unwrap();
    let (capture, base, handle) = start_mock_jss().await;
    let mut guard = EnvGuard::new();
    guard.set("JSS_URL", &base);
    guard.unset("POD_DEFAULT_PRIVATE");
    assert!(!pod_default_private_enabled());

    let state = SolidProxyState::new();
    let result = create_pod_if_missing_for_tests(&state, npub_stub(), pubkey_hex()).await;
    assert!(result.is_ok(), "legacy provisioning should succeed");

    let puts = capture.puts.lock().unwrap();
    let root_acl = puts
        .get(&format!("/{}/.acl", npub_stub()))
        .expect("legacy root ACL written");
    assert!(
        root_acl.contains("foaf:Agent"),
        "legacy path must keep foaf:Agent grant, got: {}",
        root_acl
    );
    assert!(
        !puts.contains_key(&format!("/{}/public/.acl", npub_stub())),
        "legacy path must NOT create public container ACL"
    );

    handle.stop(true).await;
}

#[actix_web::test]
async fn provision_flag_on_creates_sovereign_layout() {
    let _lock = ENV_LOCK.lock().unwrap();
    let (capture, base, handle) = start_mock_jss().await;
    let mut guard = EnvGuard::new();
    guard.set("JSS_URL", &base);
    guard.set("POD_DEFAULT_PRIVATE", "true");
    guard.set("POD_BASE_URL", "https://pods.visionclaw.org");
    assert!(pod_default_private_enabled());

    let state = SolidProxyState::new();
    let result = create_pod_if_missing_for_tests(&state, npub_stub(), pubkey_hex()).await;
    assert!(result.is_ok(), "sovereign provisioning should succeed");

    let puts = capture.puts.lock().unwrap();
    let n = npub_stub();

    // 1. Root ACL owner-only
    let root_acl = puts
        .get(&format!("/{}/.acl", n))
        .expect("root ACL written");
    assert!(
        !root_acl.contains("foaf:Agent") && !root_acl.contains("agentClass"),
        "root ACL must be owner-only, got: {}",
        root_acl
    );
    assert!(root_acl.contains(&format!(
        "acl:agent <https://pods.visionclaw.org/{}/profile/card#me>",
        pubkey_hex()
    )));

    // 2. Private container + sub-containers + owner-only ACL
    for dir in ["private", "private/kg", "private/config", "private/bridges"] {
        assert!(
            puts.contains_key(&format!("/{}/{}/", n, dir)),
            "expected container {} to be created",
            dir
        );
    }
    let private_acl = puts
        .get(&format!("/{}/private/.acl", n))
        .expect("private ACL written");
    assert!(!private_acl.contains("foaf:Agent"));

    // 3. Public container with foaf:Agent read + owner write
    let public_acl = puts
        .get(&format!("/{}/public/.acl", n))
        .expect("public ACL written");
    assert!(public_acl.contains("foaf:Agent"));
    assert!(public_acl.contains("<#publicRead>"));
    assert!(public_acl.contains("<#ownerWrite>"));
    assert!(puts.contains_key(&format!("/{}/public/kg/", n)));

    // 4. Profile container + seeded card with NIP-39 claim
    let profile_acl = puts
        .get(&format!("/{}/profile/.acl", n))
        .expect("profile ACL written");
    assert!(profile_acl.contains("acl:accessTo <./card>"));
    let card = puts
        .get(&format!("/{}/profile/card", n))
        .expect("WebID card seeded");
    assert!(card.contains(&format!("nostr:hasPubkey \"{}\"", pubkey_hex())));

    // 5. Shared placeholder is owner-only
    let shared_acl = puts
        .get(&format!("/{}/shared/.acl", n))
        .expect("shared ACL written");
    assert!(!shared_acl.contains("foaf:Agent"));

    handle.stop(true).await;
}

// ─────────────────────────────────────────────────────────────────────────────
// Migration idempotency (filesystem-only — no network)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn acl_is_sovereign_detects_foaf_agent_grant() {
    let legacy = "@prefix acl: <http://www.w3.org/ns/auth/acl#>.\n@prefix foaf: <http://xmlns.com/foaf/0.1/>.\n<#public> a acl:Authorization; acl:agentClass foaf:Agent; acl:mode acl:Read.";
    assert!(!acl_is_sovereign(legacy));

    let sov = render_owner_only_acl("https://pods.visionclaw.org/x/profile/card#me");
    assert!(acl_is_sovereign(&sov));
}

#[test]
fn migration_is_idempotent() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();

    // Pod A: legacy ACL (needs migration)
    let pod_a = root.join(pubkey_hex());
    std::fs::create_dir_all(&pod_a).unwrap();
    std::fs::write(
        pod_a.join(".acl"),
        "@prefix acl: <http://www.w3.org/ns/auth/acl#>.\n@prefix foaf: <http://xmlns.com/foaf/0.1/>.\n<#public> a acl:Authorization; acl:agentClass foaf:Agent; acl:accessTo <./>; acl:mode acl:Read.\n",
    )
    .unwrap();

    // Pod B: already sovereign (no public-read grant)
    let pod_b = root.join("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
    std::fs::create_dir_all(&pod_b).unwrap();
    std::fs::write(
        pod_b.join(".acl"),
        render_owner_only_acl("https://pods.visionclaw.org/bbb/profile/card#me"),
    )
    .unwrap();

    // First pass: Pod A migrates, Pod B is skipped
    let first = run_migration(root);
    assert_eq!(first.migrated, 1);
    assert_eq!(first.skipped_already_private, 1);
    assert_eq!(first.scanned, 2);

    // Second pass: both are sovereign now -> 0 migrated
    let second = run_migration(root);
    assert_eq!(second.migrated, 0, "second run must be a no-op");
    assert_eq!(second.skipped_already_private, 2);

    // Per-Pod helper matches the aggregate outcome
    let a_outcome = migrate_pod_acl(&pod_a).unwrap();
    assert_eq!(a_outcome, MigrateOutcome::AlreadySovereign);
}

// ─────────────────────────────────────────────────────────────────────────────
// PUT double-gate enforced end-to-end (mock JSS, real handler wiring)
// ─────────────────────────────────────────────────────────────────────────────

#[actix_web::test]
async fn put_to_public_kg_denied_without_public_marker() {
    let _lock = ENV_LOCK.lock().unwrap();
    let (_capture, base, handle) = start_mock_jss().await;
    let mut guard = EnvGuard::new();
    guard.set("JSS_URL", &base);
    guard.set("POD_DEFAULT_PRIVATE", "true");
    guard.set("SOLID_ALLOW_ANONYMOUS", "true");

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(SolidProxyState::new()))
            .configure(webxr::handlers::solid_proxy_handler::configure_routes),
    )
    .await;

    let req = test::TestRequest::put()
        .uri(&format!("/solid/{}/public/kg/foo.ttl", npub_stub()))
        .set_payload("private content, no public marker")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status().as_u16(), 403);
    let body = test::read_body(resp).await;
    let text = String::from_utf8_lossy(&body);
    assert!(text.contains("double-gate failure"));

    handle.stop(true).await;
}

#[actix_web::test]
async fn put_to_public_kg_allowed_with_visibility_header() {
    let _lock = ENV_LOCK.lock().unwrap();
    let (_capture, base, handle) = start_mock_jss().await;
    let mut guard = EnvGuard::new();
    guard.set("JSS_URL", &base);
    guard.set("POD_DEFAULT_PRIVATE", "true");
    guard.set("SOLID_ALLOW_ANONYMOUS", "true");

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(SolidProxyState::new()))
            .configure(webxr::handlers::solid_proxy_handler::configure_routes),
    )
    .await;

    let req = test::TestRequest::put()
        .uri(&format!("/solid/{}/public/kg/foo.ttl", npub_stub()))
        .insert_header(("X-VisionClaw-Visibility", "public"))
        .set_payload("body without explicit marker")
        .to_request();
    let resp = test::call_service(&app, req).await;
    // 200/201 from the mock JSS pass-through
    assert!(
        resp.status().is_success(),
        "expected success, got {}",
        resp.status()
    );

    handle.stop(true).await;
}
