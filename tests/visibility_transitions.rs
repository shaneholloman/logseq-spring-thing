//! Integration tests for the publish/unpublish saga (ADR-051).
//!
//! Strategy:
//!   * Pod is mocked via a lightweight wiremock server; the `VisibilityTransitionService`
//!     is driven against a real `PodClient` pointing at that mock.
//!   * Neo4j is abstracted behind the `VisibilityNeo4jOps` trait; each test
//!     injects an in-memory fake that records calls so we can assert ordering,
//!     state transitions, and the tombstone side-effect.
//!   * The server-Nostr actor is instantiated with a deterministic test
//!     identity exposed via the `test-utils` feature flag (see
//!     `ServerIdentity::for_test`). Signing runs locally — no relay traffic.
//!
//! These tests do not touch a live Neo4j. The full wire-level publish/unpublish
//! saga against Neo4j is covered by the manual kill-test in
//! `tests/ingest_saga.rs` (same pattern; gated on `NEO4J_TEST_URI`).

#![cfg(feature = "test-utils")]

use std::sync::Arc;

use actix::Actor;
use async_trait::async_trait;
use nostr_sdk::{Keys, SecretKey};
use tokio::sync::Mutex;
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

use webxr::actors::server_nostr_actor::ServerNostrActor;
use webxr::services::pod_client::PodClient;
use webxr::services::server_identity::ServerIdentity;
use webxr::sovereign::visibility::{
    visibility_transitions_enabled, PublishRequest, UnpublishRequest, VisibilityError,
    VisibilityNeo4jOps, VisibilityTransitionService, VISIBILITY_TRANSITIONS_ENV,
};

// ──────────────────────────────────────────────────────────────────────────
// Fake Neo4j
// ──────────────────────────────────────────────────────────────────────────

#[derive(Default, Debug, Clone)]
struct FakeNeo4jState {
    pub flip_public: Vec<(u32, String, String)>,   // (id, label, new_url)
    pub flip_private: Vec<(u32, String)>,          // (id, new_url)
    pub saga_pending: Vec<(u32, String, String)>,  // (id, step, err)
    pub tombstones: Vec<(String, String)>,         // (path, owner)
    /// If set, `flip_to_public` / `flip_to_private` returns this error.
    pub flip_error: Option<String>,
}

#[derive(Clone)]
struct FakeNeo4j {
    state: Arc<Mutex<FakeNeo4jState>>,
}

impl FakeNeo4j {
    fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(FakeNeo4jState::default())),
        }
    }

    fn with_flip_error(err: &str) -> Self {
        let s = FakeNeo4jState {
            flip_error: Some(err.to_string()),
            ..Default::default()
        };
        Self {
            state: Arc::new(Mutex::new(s)),
        }
    }

    async fn snapshot(&self) -> FakeNeo4jState {
        self.state.lock().await.clone()
    }
}

#[async_trait]
impl VisibilityNeo4jOps for FakeNeo4j {
    async fn flip_to_public(
        &self,
        node_id: u32,
        real_label: &str,
        new_pod_url: &str,
    ) -> Result<(), String> {
        let mut s = self.state.lock().await;
        if let Some(err) = s.flip_error.clone() {
            return Err(err);
        }
        s.flip_public
            .push((node_id, real_label.to_string(), new_pod_url.to_string()));
        Ok(())
    }

    async fn flip_to_private(&self, node_id: u32, new_pod_url: &str) -> Result<(), String> {
        let mut s = self.state.lock().await;
        if let Some(err) = s.flip_error.clone() {
            return Err(err);
        }
        s.flip_private.push((node_id, new_pod_url.to_string()));
        Ok(())
    }

    async fn mark_saga_pending(
        &self,
        node_id: u32,
        saga_step: &str,
        err: &str,
    ) -> Result<(), String> {
        let mut s = self.state.lock().await;
        s.saga_pending
            .push((node_id, saga_step.to_string(), err.to_string()));
        Ok(())
    }

    async fn write_tombstone(
        &self,
        old_public_path: &str,
        owner_pubkey: &str,
    ) -> Result<(), String> {
        let mut s = self.state.lock().await;
        s.tombstones
            .push((old_public_path.to_string(), owner_pubkey.to_string()));
        Ok(())
    }

    async fn is_tombstoned(&self, old_public_path: &str) -> Result<bool, String> {
        let s = self.state.lock().await;
        Ok(s.tombstones.iter().any(|(p, _)| p == old_public_path))
    }

    async fn tombstone_sunset(
        &self,
        old_public_path: &str,
    ) -> Result<Option<String>, String> {
        let s = self.state.lock().await;
        if s.tombstones.iter().any(|(p, _)| p == old_public_path) {
            Ok(Some("2026-04-20T00:00:00Z".to_string()))
        } else {
            Ok(None)
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────
// Harness helpers
// ──────────────────────────────────────────────────────────────────────────

fn test_server_keys() -> Arc<Keys> {
    let sk =
        SecretKey::from_hex("1111111111111111111111111111111111111111111111111111111111111111")
            .unwrap();
    Arc::new(Keys::new(sk))
}

fn test_server_identity() -> Arc<ServerIdentity> {
    let sk =
        SecretKey::from_hex("2222222222222222222222222222222222222222222222222222222222222222")
            .unwrap();
    Arc::new(ServerIdentity::for_test(sk))
}

fn test_pod_client() -> Arc<PodClient> {
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    Arc::new(PodClient::new(http, Some(test_server_keys())))
}

fn enable_flag() {
    std::env::set_var(VISIBILITY_TRANSITIONS_ENV, "true");
}

fn disable_flag() {
    std::env::remove_var(VISIBILITY_TRANSITIONS_ENV);
}

// ──────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────

#[actix_rt::test]
async fn publish_happy_path_flips_neo4j_and_signs_audit() {
    enable_flag();

    let mock = MockServer::start().await;
    // MOVE success
    Mock::given(method("MOVE"))
        .and(path_regex(r".*/private/kg/.*"))
        .respond_with(ResponseTemplate::new(201))
        .mount(&mock)
        .await;

    let neo = Arc::new(FakeNeo4j::new());
    let server_nostr = ServerNostrActor::new(test_server_identity()).start();

    let svc = VisibilityTransitionService::with_ops(
        test_pod_client(),
        neo.clone() as Arc<dyn VisibilityNeo4jOps>,
        server_nostr,
        None,
    );

    let req = PublishRequest {
        node_id: 42,
        owner_pubkey: "abcdef".to_string(),
        current_path: format!("{}/npub1/private/kg/page", mock.uri()),
        target_path: format!("{}/npub1/public/kg/page", mock.uri()),
        real_label: "Page A".to_string(),
    };

    svc.publish(req).await.expect("publish should succeed");

    let snap = neo.snapshot().await;
    assert_eq!(snap.flip_public.len(), 1, "one flip_public call expected");
    assert_eq!(snap.flip_public[0].0, 42);
    assert_eq!(snap.flip_public[0].1, "Page A");
    assert!(snap.flip_public[0].2.contains("/public/kg/page"));
    assert!(snap.saga_pending.is_empty(), "no pending marker on success");
    assert!(snap.tombstones.is_empty(), "publish must not write tombstone");

    disable_flag();
}

#[actix_rt::test]
async fn publish_pod_move_failure_aborts_without_touching_neo4j() {
    enable_flag();

    let mock = MockServer::start().await;
    Mock::given(method("MOVE"))
        .respond_with(ResponseTemplate::new(503).set_body_string("pod down"))
        .mount(&mock)
        .await;

    let neo = Arc::new(FakeNeo4j::new());
    let server_nostr = ServerNostrActor::new(test_server_identity()).start();

    let svc = VisibilityTransitionService::with_ops(
        test_pod_client(),
        neo.clone() as Arc<dyn VisibilityNeo4jOps>,
        server_nostr,
        None,
    );

    let req = PublishRequest {
        node_id: 7,
        owner_pubkey: "beef".to_string(),
        current_path: format!("{}/npub1/private/kg/x", mock.uri()),
        target_path: format!("{}/npub1/public/kg/x", mock.uri()),
        real_label: "X".to_string(),
    };

    let err = svc.publish(req).await.expect_err("must fail");
    assert!(
        matches!(err, VisibilityError::PodMove { .. }),
        "got: {err:?}"
    );

    let snap = neo.snapshot().await;
    assert!(
        snap.flip_public.is_empty(),
        "Neo4j must not have been touched: {:?}",
        snap.flip_public
    );
    assert!(
        snap.saga_pending.is_empty(),
        "no saga_pending on pod-failure: {:?}",
        snap.saga_pending
    );

    disable_flag();
}

#[actix_rt::test]
async fn publish_pod_ok_neo4j_fail_marks_saga_pending() {
    enable_flag();

    let mock = MockServer::start().await;
    Mock::given(method("MOVE"))
        .respond_with(ResponseTemplate::new(201))
        .mount(&mock)
        .await;

    let neo = Arc::new(FakeNeo4j::with_flip_error("neo4j down"));
    let server_nostr = ServerNostrActor::new(test_server_identity()).start();

    let svc = VisibilityTransitionService::with_ops(
        test_pod_client(),
        neo.clone() as Arc<dyn VisibilityNeo4jOps>,
        server_nostr,
        None,
    );

    let req = PublishRequest {
        node_id: 99,
        owner_pubkey: "cafe".to_string(),
        current_path: format!("{}/npub1/private/kg/p", mock.uri()),
        target_path: format!("{}/npub1/public/kg/p", mock.uri()),
        real_label: "P".to_string(),
    };

    let err = svc.publish(req).await.expect_err("must fail");
    assert!(matches!(err, VisibilityError::Neo4j(_)), "got: {err:?}");

    let snap = neo.snapshot().await;
    assert_eq!(
        snap.saga_pending.len(),
        1,
        "saga_pending must be marked: {:?}",
        snap.saga_pending
    );
    assert_eq!(snap.saga_pending[0].0, 99);
    assert_eq!(snap.saga_pending[0].1, "published_pod");

    disable_flag();
}

#[actix_rt::test]
async fn unpublish_happy_path_writes_tombstone() {
    enable_flag();

    let mock = MockServer::start().await;
    Mock::given(method("MOVE"))
        .respond_with(ResponseTemplate::new(201))
        .mount(&mock)
        .await;

    let neo = Arc::new(FakeNeo4j::new());
    let server_nostr = ServerNostrActor::new(test_server_identity()).start();

    let svc = VisibilityTransitionService::with_ops(
        test_pod_client(),
        neo.clone() as Arc<dyn VisibilityNeo4jOps>,
        server_nostr,
        None,
    );

    let current = format!("{}/npub1/public/kg/page", mock.uri());
    let target = format!("{}/npub1/private/kg/page", mock.uri());

    let req = UnpublishRequest {
        node_id: 11,
        owner_pubkey: "feed".to_string(),
        current_path: current.clone(),
        target_path: target.clone(),
    };

    svc.unpublish(req).await.expect("unpublish ok");

    let snap = neo.snapshot().await;
    assert_eq!(snap.flip_private.len(), 1);
    assert_eq!(snap.flip_private[0].0, 11);
    assert_eq!(snap.flip_private[0].1, target);

    assert_eq!(snap.tombstones.len(), 1);
    assert_eq!(snap.tombstones[0].0, current, "tombstone keyed by old public URL");
    assert_eq!(snap.tombstones[0].1, "feed", "owner pubkey recorded");

    // And then a subsequent GET-tombstone lookup should return 410 semantics
    // (true + Some(sunset)).
    let is_tomb = neo
        .is_tombstoned(&current)
        .await
        .expect("tombstone lookup ok");
    assert!(is_tomb, "must be tombstoned after unpublish");
    let sunset = neo.tombstone_sunset(&current).await.expect("sunset");
    assert!(sunset.is_some(), "sunset timestamp must be present");

    disable_flag();
}

#[actix_rt::test]
async fn unpublish_pod_failure_leaves_state_untouched() {
    enable_flag();

    let mock = MockServer::start().await;
    Mock::given(method("MOVE"))
        .respond_with(ResponseTemplate::new(502))
        .mount(&mock)
        .await;

    let neo = Arc::new(FakeNeo4j::new());
    let server_nostr = ServerNostrActor::new(test_server_identity()).start();

    let svc = VisibilityTransitionService::with_ops(
        test_pod_client(),
        neo.clone() as Arc<dyn VisibilityNeo4jOps>,
        server_nostr,
        None,
    );

    let req = UnpublishRequest {
        node_id: 44,
        owner_pubkey: "bad".to_string(),
        current_path: format!("{}/npub1/public/kg/x", mock.uri()),
        target_path: format!("{}/npub1/private/kg/x", mock.uri()),
    };

    let err = svc.unpublish(req).await.expect_err("must fail");
    assert!(
        matches!(err, VisibilityError::PodMove { .. }),
        "got: {err:?}"
    );

    let snap = neo.snapshot().await;
    assert!(snap.flip_private.is_empty(), "no Neo4j change on pod failure");
    assert!(snap.tombstones.is_empty(), "no tombstone on pod failure");

    disable_flag();
}

#[actix_rt::test]
async fn tombstone_lookup_returns_sunset_for_unpublished_path() {
    // Exercises just the trait-level lookup used by the solid-proxy GET path.
    let neo = FakeNeo4j::new();
    let path = "http://pod/npub1/public/kg/retracted".to_string();

    neo.write_tombstone(&path, "owner").await.unwrap();

    assert!(neo.is_tombstoned(&path).await.unwrap());
    assert!(!neo.is_tombstoned("http://pod/npub1/public/kg/other").await.unwrap());

    let sunset = neo.tombstone_sunset(&path).await.unwrap();
    assert!(sunset.is_some());
    assert!(neo
        .tombstone_sunset("http://pod/npub1/public/kg/other")
        .await
        .unwrap()
        .is_none());
}

#[actix_rt::test]
async fn feature_flag_off_returns_not_enabled_without_side_effects() {
    disable_flag();
    assert!(!visibility_transitions_enabled());

    // Even if the Pod would succeed, the service must no-op.
    let mock = MockServer::start().await;
    Mock::given(method("MOVE"))
        .respond_with(ResponseTemplate::new(201))
        .mount(&mock)
        .await;

    let neo = Arc::new(FakeNeo4j::new());
    let server_nostr = ServerNostrActor::new(test_server_identity()).start();

    let svc = VisibilityTransitionService::with_ops(
        test_pod_client(),
        neo.clone() as Arc<dyn VisibilityNeo4jOps>,
        server_nostr,
        None,
    );

    let pub_err = svc
        .publish(PublishRequest {
            node_id: 1,
            owner_pubkey: "x".into(),
            current_path: format!("{}/a/private/kg/n", mock.uri()),
            target_path: format!("{}/a/public/kg/n", mock.uri()),
            real_label: "N".into(),
        })
        .await
        .expect_err("flag off → must error");
    assert!(matches!(pub_err, VisibilityError::NotEnabled));

    let unpub_err = svc
        .unpublish(UnpublishRequest {
            node_id: 1,
            owner_pubkey: "x".into(),
            current_path: format!("{}/a/public/kg/n", mock.uri()),
            target_path: format!("{}/a/private/kg/n", mock.uri()),
        })
        .await
        .expect_err("flag off → must error");
    assert!(matches!(unpub_err, VisibilityError::NotEnabled));

    let snap = neo.snapshot().await;
    assert!(snap.flip_public.is_empty());
    assert!(snap.flip_private.is_empty());
    assert!(snap.tombstones.is_empty());
    assert!(snap.saga_pending.is_empty());
}
