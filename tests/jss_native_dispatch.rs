//! ADR-053 Phase 3: SOLID_IMPL dispatcher selection tests.
//!
//! These tests verify the env-var-driven selection logic in
//! `webxr::handlers::solid_pod_handler::SolidImpl`. The dispatcher
//! itself is wired in `main.rs` (binary-level), so we assert the
//! behaviour at the library-level boundary where it's deterministic:
//! the `SolidImpl::from_env()` classifier.
//!
//! Mapping from env → dispatcher:
//!
//! | `SOLID_IMPL`    | Resolved variant | Handler chosen (main.rs)          |
//! |-----------------|------------------|-----------------------------------|
//! | unset           | `Jss`            | legacy `configure_solid_routes`   |
//! | `jss`           | `Jss`            | legacy `configure_solid_routes`   |
//! | `JSS` (mixed)   | `Jss`            | legacy `configure_solid_routes`   |
//! | `native`        | `Native`         | `configure_solid_native_routes`   |
//! | `shadow`        | `Shadow`         | legacy + shadow comparator        |
//! | `bogus`         | `Jss` (+ warn)   | legacy `configure_solid_routes`   |
//!
//! The shadow-mode comparator and audit writer are exercised via
//! separate unit tests so test order and env-var mutation don't
//! interfere.

use std::sync::Mutex;

use webxr::handlers::solid_pod_handler::{
    append_shadow_diff, compare_shadow, CapturedResponse, ShadowDiff, SolidImpl,
};

// The std::env API is process-global, so we serialise the env-mutating
// tests with a mutex. Without this, parallel cargo-test execution will
// race on `SOLID_IMPL` and produce flakes.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn with_env<T>(key: &str, value: Option<&str>, f: impl FnOnce() -> T) -> T {
    let _guard = ENV_LOCK.lock().unwrap();
    let prior = std::env::var(key).ok();
    match value {
        Some(v) => std::env::set_var(key, v),
        None => std::env::remove_var(key),
    }
    let out = f();
    match prior {
        Some(v) => std::env::set_var(key, v),
        None => std::env::remove_var(key),
    }
    out
}

// ---------------------------------------------------------------------------
// 1. Env SOLID_IMPL=jss → JSS handler selected.
// ---------------------------------------------------------------------------

#[test]
fn solid_impl_jss_selects_jss_variant() {
    let resolved = with_env("SOLID_IMPL", Some("jss"), SolidImpl::from_env);
    assert_eq!(resolved, SolidImpl::Jss);
    assert_eq!(resolved.as_str(), "jss");
}

// ---------------------------------------------------------------------------
// 2. Env SOLID_IMPL=native → native handler selected.
// ---------------------------------------------------------------------------

#[test]
fn solid_impl_native_selects_native_variant() {
    let resolved = with_env("SOLID_IMPL", Some("native"), SolidImpl::from_env);
    assert_eq!(resolved, SolidImpl::Native);
    assert_eq!(resolved.as_str(), "native");
}

// ---------------------------------------------------------------------------
// 3. Env SOLID_IMPL=shadow → JSS handler (client-visible) + shadow variant.
// ---------------------------------------------------------------------------

#[test]
fn solid_impl_shadow_selects_shadow_variant() {
    let resolved = with_env("SOLID_IMPL", Some("shadow"), SolidImpl::from_env);
    assert_eq!(resolved, SolidImpl::Shadow);
    assert_eq!(resolved.as_str(), "shadow");
}

// ---------------------------------------------------------------------------
// 4. Env unset → defaults to jss.
// ---------------------------------------------------------------------------

#[test]
fn solid_impl_unset_defaults_to_jss() {
    let resolved = with_env("SOLID_IMPL", None, SolidImpl::from_env);
    assert_eq!(resolved, SolidImpl::Jss);
}

// ---------------------------------------------------------------------------
// 5. Invalid env value → defaults to jss (with WARN log). We cannot
//    observe the WARN directly without a log capture harness, but the
//    fallback-to-jss behaviour is the contract we care about.
// ---------------------------------------------------------------------------

#[test]
fn solid_impl_invalid_defaults_to_jss() {
    let resolved = with_env("SOLID_IMPL", Some("bogus-value-🤦"), SolidImpl::from_env);
    assert_eq!(resolved, SolidImpl::Jss);
}

// ---------------------------------------------------------------------------
// Bonus: case-insensitive matching for operator-friendly input.
// ---------------------------------------------------------------------------

#[test]
fn solid_impl_case_insensitive() {
    let upper = with_env("SOLID_IMPL", Some("NATIVE"), SolidImpl::from_env);
    assert_eq!(upper, SolidImpl::Native);
    let mixed = with_env("SOLID_IMPL", Some("Shadow"), SolidImpl::from_env);
    assert_eq!(mixed, SolidImpl::Shadow);
}

// ---------------------------------------------------------------------------
// Shadow comparator: identical responses compare equal.
// ---------------------------------------------------------------------------

#[test]
fn shadow_comparator_flags_byte_equal_bodies_as_matching() {
    let body = bytes::Bytes::from_static(b"<a> <b> <c> .\n");
    let jss = CapturedResponse {
        status: 200,
        content_type: Some("text/turtle".into()),
        link_headers: vec![
            "<http://www.w3.org/ns/ldp#Resource>; rel=\"type\"".into()
        ],
        body: body.clone(),
    };
    let native = CapturedResponse {
        status: 200,
        content_type: Some("text/turtle".into()),
        link_headers: vec![
            "<http://www.w3.org/ns/ldp#Resource>; rel=\"type\"".into()
        ],
        body,
    };
    let diff: ShadowDiff = compare_shadow("/solid/x", "GET", &jss, &native);
    assert!(diff.status_match);
    assert!(diff.content_type_match);
    assert!(diff.link_match);
    assert!(diff.body_match);
    assert_eq!(diff.body_diff_bytes, 0);
}

// ---------------------------------------------------------------------------
// Shadow comparator: whitespace-only diffs do NOT count as body mismatches.
// ---------------------------------------------------------------------------

#[test]
fn shadow_comparator_normalises_turtle_whitespace() {
    let jss = CapturedResponse {
        status: 200,
        content_type: Some("text/turtle".into()),
        link_headers: vec![],
        body: bytes::Bytes::from_static(b"<a> <b> <c> .\n<d> <e> <f> ."),
    };
    let native = CapturedResponse {
        status: 200,
        content_type: Some("text/turtle".into()),
        link_headers: vec![],
        body: bytes::Bytes::from_static(b"<a>  <b>\t<c>   .\n\n<d>   <e>  <f>  ."),
    };
    let diff = compare_shadow("/solid/x", "GET", &jss, &native);
    assert!(diff.body_match, "whitespace-only diff must not mismatch");
}

// ---------------------------------------------------------------------------
// Shadow comparator: genuine body diff is flagged.
// ---------------------------------------------------------------------------

#[test]
fn shadow_comparator_flags_real_body_diff() {
    let jss = CapturedResponse {
        status: 200,
        content_type: Some("text/turtle".into()),
        link_headers: vec![],
        body: bytes::Bytes::from_static(b"<a> <b> <c> ."),
    };
    let native = CapturedResponse {
        status: 200,
        content_type: Some("text/turtle".into()),
        link_headers: vec![],
        body: bytes::Bytes::from_static(b"<a> <b> <DIFFERENT> ."),
    };
    let diff = compare_shadow("/solid/x", "GET", &jss, &native);
    assert!(!diff.body_match);
    assert_ne!(diff.body_diff_bytes, 0);
}

// ---------------------------------------------------------------------------
// Shadow audit writer: writes a well-formed JSONL line into a temp dir.
// We exercise it via tokio test by redirecting CWD.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn shadow_audit_writer_appends_jsonl_line() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let original_cwd = std::env::current_dir().expect("cwd");
    let audits_dir = tmp.path().join("docs/audits");
    std::fs::create_dir_all(&audits_dir).expect("mkdir audits");

    // The writer constructs the path relative to CWD, so pivot to the
    // tempdir for the duration of the test.
    let _guard = ENV_LOCK.lock().unwrap();
    std::env::set_current_dir(tmp.path()).expect("chdir tmp");

    let diff = ShadowDiff {
        ts: "2026-04-20T00:00:00Z".into(),
        path: "/solid/unit-test".into(),
        method: "GET".into(),
        status_match: true,
        jss_status: 200,
        native_status: 200,
        content_type_match: true,
        link_match: true,
        body_match: true,
        body_diff_bytes: 0,
    };
    append_shadow_diff(&diff).await;

    // Restore CWD before asserting so a failed assert doesn't leave us
    // dangling in a tempdir that's about to be deleted.
    std::env::set_current_dir(&original_cwd).expect("restore cwd");

    let day = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let written = audits_dir.join(format!("{day}-jss-native-shadow.jsonl"));
    let contents = std::fs::read_to_string(&written)
        .unwrap_or_else(|e| panic!("read {}: {e}", written.display()));
    assert!(contents.trim_end().ends_with('}'));
    assert!(contents.contains("/solid/unit-test"));
}
