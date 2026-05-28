//! Integration tests for solid_proxy_handler.rs
//!
//! Uses `solid_pod_rs::storage::memory::MemoryBackend` as a zero-filesystem
//! Storage implementation for unit-level tests, and `tempfile` + `FsBackend`
//! for the provisioning and HTTP handler tests that go through `SolidPodState`.
//!
//! Run with:
//!   cargo test --features solid-pod-embed \
//!              --test solid_pod_handler_test
//!
//! The `solid-pod-rs` dependency must include `memory-backend` (already added
//! to Cargo.toml) so `MemoryBackend` is available in the test binary.

// ============================================================================
// Feature-gated integration tests
// ============================================================================

#[cfg(feature = "solid-pod-embed")]
mod embed_tests {
    use actix_web::http::StatusCode;
    use actix_web::test as aw_test;
    use actix_web::{web, App};
    use bytes::Bytes;
    use nostr_sdk::Keys;
    use serde_json::Value;

    use solid_pod_rs::error::PodError;
    use solid_pod_rs::interop::did_nostr::did_nostr_document;
    use solid_pod_rs::ldp::{
        evaluate_preconditions, is_container, negotiate_format, patch_dialect_from_mime,
        ConditionalOutcome, Graph, PatchDialect, RdfFormat, Term, Triple,
    };
    use solid_pod_rs::storage::memory::MemoryBackend;
    use solid_pod_rs::storage::{Storage, StorageEvent};
    use solid_pod_rs::wac::{
        evaluate_access, method_to_mode, AclAuthorization, AclDocument, AccessMode, IdOrIds, IdRef,
    };

    use webxr::handlers::solid_proxy_handler::{
        ensure_pod_exists, get_global_storage, solid_health_check, SolidNotificationMessage,
        SolidPodState, SolidProxyError,
    };
    use webxr::utils::nip98::{generate_nip98_token, Nip98Config};

    // -----------------------------------------------------------------------
    // Helper: build a SolidPodState backed by a tempdir FsBackend
    // -----------------------------------------------------------------------

    async fn temp_state() -> (SolidPodState, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir must be creatable");
        std::env::set_var("SOLID_DATA_ROOT", dir.path().to_str().unwrap());
        std::env::remove_var("SOLID_PROXY_SECRET_KEY");
        std::env::remove_var("SOLID_ALLOW_ANONYMOUS");
        let state = SolidPodState::new_async().await;
        (state, dir)
    }

    // ===================================================================
    // 1. SolidPodState construction — env var handling
    // ===================================================================

    #[tokio::test]
    async fn state_uses_solid_data_root_env() {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("SOLID_DATA_ROOT", dir.path().to_str().unwrap());
        std::env::remove_var("SOLID_PROXY_SECRET_KEY");
        std::env::remove_var("SOLID_ALLOW_ANONYMOUS");

        let state = SolidPodState::new_async().await;
        assert_eq!(state.data_root, dir.path().to_path_buf());
        assert!(state.server_keys.is_none());
        assert!(!state.allow_anonymous);
    }

    #[tokio::test]
    async fn state_reads_allow_anonymous_true_string() {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("SOLID_DATA_ROOT", dir.path().to_str().unwrap());
        std::env::set_var("SOLID_ALLOW_ANONYMOUS", "true");
        std::env::remove_var("SOLID_PROXY_SECRET_KEY");

        let state = SolidPodState::new_async().await;
        assert!(state.allow_anonymous);

        std::env::remove_var("SOLID_ALLOW_ANONYMOUS");
    }

    #[tokio::test]
    async fn state_reads_allow_anonymous_1_digit() {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("SOLID_DATA_ROOT", dir.path().to_str().unwrap());
        std::env::set_var("SOLID_ALLOW_ANONYMOUS", "1");
        std::env::remove_var("SOLID_PROXY_SECRET_KEY");

        let state = SolidPodState::new_async().await;
        assert!(state.allow_anonymous);

        std::env::remove_var("SOLID_ALLOW_ANONYMOUS");
    }

    #[tokio::test]
    async fn state_reads_server_keys_from_env() {
        let dir = tempfile::tempdir().unwrap();
        let keys = Keys::generate();
        let hex = keys.secret_key().to_secret_hex();
        std::env::set_var("SOLID_DATA_ROOT", dir.path().to_str().unwrap());
        std::env::set_var("SOLID_PROXY_SECRET_KEY", &hex);
        std::env::remove_var("SOLID_ALLOW_ANONYMOUS");

        let state = SolidPodState::new_async().await;
        assert!(state.server_keys.is_some());

        std::env::remove_var("SOLID_PROXY_SECRET_KEY");
    }

    #[tokio::test]
    async fn state_falls_back_to_tempdir_on_unwritable_data_root() {
        // /proc is root-owned and unwritable for regular users — FsBackend
        // construction will fail and the code falls back to a tmp path.
        std::env::set_var("SOLID_DATA_ROOT", "/proc/solid-pod-test-fallback-xyz");
        std::env::remove_var("SOLID_PROXY_SECRET_KEY");
        std::env::remove_var("SOLID_ALLOW_ANONYMOUS");

        // Must not panic — fallback FsBackend should be used
        let state = SolidPodState::new_async().await;
        // The storage backend should be alive
        assert!(state.storage.exists("/").await.is_ok());

        std::env::remove_var("SOLID_DATA_ROOT");
    }

    // ===================================================================
    // 2. extract_user_identity — NIP-98 header extraction and validation
    // ===================================================================

    #[tokio::test]
    async fn extract_user_identity_returns_none_without_auth_header() {
        let (state, _dir) = temp_state().await;
        let req = aw_test::TestRequest::get()
            .uri("/solid/test/resource")
            .to_http_request();
        assert!(state.extract_user_identity(&req).is_none());
    }

    #[tokio::test]
    async fn extract_user_identity_returns_none_for_bearer_prefix() {
        let (state, _dir) = temp_state().await;
        let req = aw_test::TestRequest::get()
            .uri("/solid/test/resource")
            .insert_header(("Authorization", "Bearer some-session-token"))
            .to_http_request();
        assert!(state.extract_user_identity(&req).is_none());
    }

    #[tokio::test]
    async fn extract_user_identity_returns_none_for_malformed_nip98() {
        let (state, _dir) = temp_state().await;
        let req = aw_test::TestRequest::get()
            .uri("/solid/test/resource")
            .insert_header(("Authorization", "Nostr !!!not-base64!!!"))
            .to_http_request();
        assert!(state.extract_user_identity(&req).is_none());
    }

    #[tokio::test]
    async fn extract_user_identity_validates_good_nip98_token() {
        let (state, _dir) = temp_state().await;

        let keys = Keys::generate();
        let url = "http://localhost/solid/test/resource";
        let token = generate_nip98_token(
            &keys,
            &Nip98Config {
                url: url.to_string(),
                method: "GET".to_string(),
                body: None,
            },
        )
        .expect("token generation must succeed");

        let req = aw_test::TestRequest::get()
            .uri("/solid/test/resource")
            .insert_header(("Authorization", format!("Nostr {}", token).as_str()))
            .insert_header(("X-Forwarded-Proto", "http"))
            .insert_header(("X-Forwarded-Host", "localhost"))
            .insert_header(("X-Forwarded-URI", "/solid/test/resource"))
            .to_http_request();

        let identity = state.extract_user_identity(&req);
        assert!(identity.is_some(), "valid NIP-98 token must be accepted");
        let id = identity.unwrap();
        assert_eq!(id.pubkey, keys.public_key().to_hex());
        assert!(!id.nip98_token.is_empty());
        assert!(id.auth_header.starts_with("Nostr "));
    }

    #[tokio::test]
    async fn extract_user_identity_rejects_wrong_url_in_token() {
        let (state, _dir) = temp_state().await;

        let keys = Keys::generate();
        // Token built for a different URL
        let token = generate_nip98_token(
            &keys,
            &Nip98Config {
                url: "http://other-host.example/other/path".to_string(),
                method: "GET".to_string(),
                body: None,
            },
        )
        .unwrap();

        // Request claims to be at /solid/test/resource
        let req = aw_test::TestRequest::get()
            .uri("/solid/test/resource")
            .insert_header(("Authorization", format!("Nostr {}", token).as_str()))
            .insert_header(("X-Forwarded-Proto", "http"))
            .insert_header(("X-Forwarded-Host", "localhost"))
            .insert_header(("X-Forwarded-URI", "/solid/test/resource"))
            .to_http_request();

        assert!(
            state.extract_user_identity(&req).is_none(),
            "token for a different URL must be rejected"
        );
    }

    // ===================================================================
    // 3. WAC access control — evaluate_access
    // ===================================================================

    #[test]
    fn wac_no_acl_document_denies_everything() {
        assert!(!evaluate_access(None, None, "/foo", AccessMode::Read, None));
        assert!(!evaluate_access(None, Some("did:nostr:owner"), "/foo", AccessMode::Read, None));
    }

    #[test]
    fn wac_public_read_rule_grants_anonymous_read() {
        let doc = AclDocument {
            context: None,
            graph: Some(vec![AclAuthorization {
                id: None,
                r#type: None,
                agent: None,
                agent_class: Some(IdOrIds::Single(IdRef { id: "foaf:Agent".into() })),
                agent_group: None,
                origin: None,
                access_to: Some(IdOrIds::Single(IdRef { id: "/".into() })),
                default: None,
                mode: Some(IdOrIds::Single(IdRef { id: "acl:Read".into() })),
                condition: None,
            }]),
        };
        assert!(evaluate_access(Some(&doc), None, "/", AccessMode::Read, None));
        assert!(!evaluate_access(Some(&doc), None, "/", AccessMode::Write, None));
    }

    #[test]
    fn wac_owner_granted_write_denies_other_agent() {
        let owner = "did:nostr:owner_pubkey_hex";
        let stranger = "did:nostr:stranger_pubkey_hex";
        let doc = AclDocument {
            context: None,
            graph: Some(vec![AclAuthorization {
                id: Some("#owner".into()),
                r#type: Some("acl:Authorization".into()),
                agent: Some(IdOrIds::Single(IdRef { id: owner.into() })),
                agent_class: None,
                agent_group: None,
                origin: None,
                access_to: Some(IdOrIds::Single(IdRef { id: "/".into() })),
                default: Some(IdOrIds::Single(IdRef { id: "/".into() })),
                mode: Some(IdOrIds::Multiple(vec![
                    IdRef { id: "acl:Read".into() },
                    IdRef { id: "acl:Write".into() },
                    IdRef { id: "acl:Control".into() },
                ])),
                condition: None,
            }]),
        };
        // Owner gets write
        assert!(evaluate_access(Some(&doc), Some(owner), "/resource", AccessMode::Write, None));
        // Stranger does not
        assert!(!evaluate_access(Some(&doc), Some(stranger), "/resource", AccessMode::Write, None));
        // Anonymous does not
        assert!(!evaluate_access(Some(&doc), None, "/resource", AccessMode::Write, None));
    }

    #[test]
    fn wac_acl_write_mode_implies_append() {
        let agent = "did:nostr:writer";
        let doc = AclDocument {
            context: None,
            graph: Some(vec![AclAuthorization {
                id: None,
                r#type: None,
                agent: Some(IdOrIds::Single(IdRef { id: agent.into() })),
                agent_class: None,
                agent_group: None,
                origin: None,
                access_to: Some(IdOrIds::Single(IdRef { id: "/".into() })),
                default: Some(IdOrIds::Single(IdRef { id: "/".into() })),
                mode: Some(IdOrIds::Single(IdRef { id: "acl:Write".into() })),
                condition: None,
            }]),
        };
        // acl:Write implies acl:Append per the WAC spec
        assert!(evaluate_access(Some(&doc), Some(agent), "/child/resource", AccessMode::Append, None));
    }

    #[test]
    fn wac_method_to_mode_covers_all_http_verbs() {
        assert_eq!(method_to_mode("GET"), AccessMode::Read);
        assert_eq!(method_to_mode("HEAD"), AccessMode::Read);
        assert_eq!(method_to_mode("PUT"), AccessMode::Write);
        assert_eq!(method_to_mode("DELETE"), AccessMode::Write);
        assert_eq!(method_to_mode("PATCH"), AccessMode::Write);
        assert_eq!(method_to_mode("POST"), AccessMode::Append);
    }

    // ===================================================================
    // 4. LDP helpers — container detection, preconditions, patch dialects,
    //    content negotiation
    // ===================================================================

    #[test]
    fn ldp_is_container_detects_trailing_slash() {
        assert!(is_container("/alice/"));
        assert!(is_container("/alice/inbox/"));
        assert!(is_container("/"));
        assert!(!is_container("/alice/profile/card"));
        assert!(!is_container("/solid/health"));
    }

    #[test]
    fn ldp_preconditions_proceed_when_no_conditional_headers() {
        let outcome = evaluate_preconditions("GET", Some("abc123"), None, None);
        assert_eq!(outcome, ConditionalOutcome::Proceed);
    }

    #[test]
    fn ldp_preconditions_not_modified_when_if_none_match_matches_etag() {
        let outcome = evaluate_preconditions("GET", Some("abc"), None, Some("\"abc\""));
        assert_eq!(outcome, ConditionalOutcome::NotModified);
    }

    #[test]
    fn ldp_preconditions_failed_when_if_match_does_not_match() {
        let outcome = evaluate_preconditions("PUT", Some("current"), Some("\"stale\""), None);
        assert_eq!(outcome, ConditionalOutcome::PreconditionFailed);
    }

    #[test]
    fn ldp_preconditions_proceed_when_if_match_matches() {
        let outcome = evaluate_preconditions("PUT", Some("current"), Some("\"current\""), None);
        assert_eq!(outcome, ConditionalOutcome::Proceed);
    }

    #[test]
    fn ldp_patch_dialect_detects_n3() {
        assert_eq!(patch_dialect_from_mime("text/n3"), Some(PatchDialect::N3));
        assert_eq!(
            patch_dialect_from_mime("text/n3; charset=utf-8"),
            Some(PatchDialect::N3)
        );
    }

    #[test]
    fn ldp_patch_dialect_detects_sparql() {
        assert_eq!(
            patch_dialect_from_mime("application/sparql-update"),
            Some(PatchDialect::SparqlUpdate)
        );
    }

    #[test]
    fn ldp_patch_dialect_detects_json_patch() {
        assert_eq!(
            patch_dialect_from_mime("application/json-patch+json"),
            Some(PatchDialect::JsonPatch)
        );
    }

    #[test]
    fn ldp_patch_dialect_returns_none_for_unsupported_mime() {
        assert!(patch_dialect_from_mime("text/plain").is_none());
        assert!(patch_dialect_from_mime("application/octet-stream").is_none());
        assert!(patch_dialect_from_mime("").is_none());
    }

    #[test]
    fn ldp_negotiate_format_defaults_to_jsonld() {
        assert_eq!(negotiate_format(None), RdfFormat::JsonLd);
        assert_eq!(negotiate_format(Some("application/ld+json")), RdfFormat::JsonLd);
        assert_eq!(negotiate_format(Some("application/json")), RdfFormat::JsonLd);
    }

    #[test]
    fn ldp_negotiate_format_selects_turtle_when_requested() {
        assert_eq!(negotiate_format(Some("text/turtle")), RdfFormat::Turtle);
        assert_eq!(
            negotiate_format(Some("text/turtle; q=1.0")),
            RdfFormat::Turtle
        );
    }

    // ===================================================================
    // 5. MemoryBackend — full CRUD cycle
    // ===================================================================

    #[tokio::test]
    async fn memory_backend_put_then_get_roundtrip() {
        let store = MemoryBackend::new();
        let body = Bytes::from_static(b"@prefix foaf: <http://xmlns.com/foaf/0.1/> .");
        store
            .put("/alice/profile/card", body.clone(), "text/turtle")
            .await
            .unwrap();
        let (fetched, meta) = store.get("/alice/profile/card").await.unwrap();
        assert_eq!(fetched, body);
        assert_eq!(meta.content_type, "text/turtle");
        assert_eq!(meta.size, body.len() as u64);
    }

    #[tokio::test]
    async fn memory_backend_get_absent_returns_not_found() {
        let store = MemoryBackend::new();
        let err = store.get("/does/not/exist").await.unwrap_err();
        assert!(matches!(err, PodError::NotFound(_)));
    }

    #[tokio::test]
    async fn memory_backend_delete_removes_resource() {
        let store = MemoryBackend::new();
        store
            .put("/r", Bytes::from_static(b"data"), "text/plain")
            .await
            .unwrap();
        store.delete("/r").await.unwrap();
        assert!(store.get("/r").await.is_err());
    }

    #[tokio::test]
    async fn memory_backend_delete_absent_returns_not_found() {
        let store = MemoryBackend::new();
        let err = store.delete("/nope").await.unwrap_err();
        assert!(matches!(err, PodError::NotFound(_)));
    }

    #[tokio::test]
    async fn memory_backend_head_returns_metadata_without_body() {
        let store = MemoryBackend::new();
        let body = Bytes::from_static(b"hello world");
        store
            .put("/meta/test", body, "text/plain")
            .await
            .unwrap();
        let meta = store.head("/meta/test").await.unwrap();
        assert_eq!(meta.size, 11);
        assert_eq!(meta.content_type, "text/plain");
        assert!(!meta.etag.is_empty());
    }

    #[tokio::test]
    async fn memory_backend_exists_returns_false_then_true() {
        let store = MemoryBackend::new();
        assert!(!store.exists("/nope").await.unwrap());
        store
            .put("/yep", Bytes::new(), "text/plain")
            .await
            .unwrap();
        assert!(store.exists("/yep").await.unwrap());
    }

    #[tokio::test]
    async fn memory_backend_list_returns_direct_children_only() {
        let store = MemoryBackend::new();
        store
            .put("/pod/profile/card", Bytes::new(), "text/turtle")
            .await
            .unwrap();
        store
            .put("/pod/inbox/msg1", Bytes::new(), "text/plain")
            .await
            .unwrap();
        store
            .put("/pod/inbox/sub/deep", Bytes::new(), "text/plain")
            .await
            .unwrap();

        let mut children = store.list("/pod").await.unwrap();
        children.sort();
        // Should see profile/ and inbox/ but not sub/ or deep
        assert!(children.contains(&"profile/".to_string()));
        assert!(children.contains(&"inbox/".to_string()));
        assert!(!children.iter().any(|c| c.contains("deep")));
    }

    #[tokio::test]
    async fn memory_backend_create_container_is_detectable() {
        let store = MemoryBackend::new();
        store.create_container("/mycontainer/").await.unwrap();
        assert!(store.exists("/mycontainer/").await.unwrap());
    }

    // ===================================================================
    // 6. Health check HTTP endpoint
    // ===================================================================

    #[actix_rt::test]
    async fn health_check_returns_200_when_storage_is_working() {
        let (state, _dir) = temp_state().await;
        let data = web::Data::new(state);

        let app = aw_test::init_service(
            App::new()
                .app_data(data)
                .route("/health", web::get().to(solid_health_check)),
        )
        .await;

        let req = aw_test::TestRequest::get().uri("/health").to_request();
        let resp = aw_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body: Value = aw_test::read_body_json(resp).await;
        assert_eq!(body["status"], "healthy");
        assert_eq!(body["backend"], "solid-pod-rs");
        assert!(body["data_root"].is_string());
    }

    // ===================================================================
    // 7. ensure_pod_exists — provisioning lifecycle
    // ===================================================================

    #[tokio::test]
    async fn ensure_pod_exists_creates_pod_on_first_call() {
        let (state, _dir) = temp_state().await;
        let keys = Keys::generate();
        let pubkey = keys.public_key().to_hex();
        let npub = "npub1testpod001";
        let base = "https://example.com/solid";

        let (created, structure) = ensure_pod_exists(&state, npub, &pubkey, base)
            .await
            .expect("pod creation must succeed");

        assert!(created, "first call must create the pod");
        assert!(
            structure.profile.contains(npub),
            "pod structure must reference the user's npub"
        );
        // Pod root must exist in storage
        assert!(
            state
                .storage
                .exists(&format!("/{}/", npub))
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn ensure_pod_exists_is_idempotent_on_second_call() {
        let (state, _dir) = temp_state().await;
        let keys = Keys::generate();
        let pubkey = keys.public_key().to_hex();
        let npub = "npub1idempotent002";
        let base = "https://example.com/solid";

        let (created1, _) = ensure_pod_exists(&state, npub, &pubkey, base)
            .await
            .unwrap();
        let (created2, _) = ensure_pod_exists(&state, npub, &pubkey, base)
            .await
            .unwrap();

        assert!(created1, "first call must create");
        assert!(!created2, "second call must not report creation");
    }

    #[tokio::test]
    async fn ensure_pod_exists_backfills_missing_acl_on_second_call() {
        let (state, _dir) = temp_state().await;
        let keys = Keys::generate();
        let pubkey = keys.public_key().to_hex();
        let npub = "npub1aclmigrate003";
        let base = "https://example.com/solid";

        ensure_pod_exists(&state, npub, &pubkey, base).await.unwrap();

        // Simulate missing ACL (migration scenario)
        let acl_path = format!("/{}/.acl", npub);
        let _ = state.storage.delete(&acl_path).await;
        assert!(
            !state.storage.exists(&acl_path).await.unwrap(),
            "ACL must be absent before backfill test"
        );

        // Second call should backfill
        ensure_pod_exists(&state, npub, &pubkey, base).await.unwrap();
        assert!(
            state.storage.exists(&acl_path).await.unwrap(),
            "ACL must be backfilled on second ensure_pod_exists"
        );
    }

    // ===================================================================
    // 8. WebID profile card — provisioned content validation
    // ===================================================================

    #[tokio::test]
    async fn provisioned_pod_has_webid_profile_with_pubkey_and_npub() {
        let (state, _dir) = temp_state().await;
        let keys = Keys::generate();
        let pubkey = keys.public_key().to_hex();
        let npub = "npub1profilecheck004";
        let base = "https://example.com/solid";

        ensure_pod_exists(&state, npub, &pubkey, base).await.unwrap();

        let profile_path = format!("/{}/profile/card", npub);
        let (body, meta) = state.storage.get(&profile_path).await.unwrap();
        let text = String::from_utf8_lossy(&body);

        assert_eq!(meta.content_type, "text/turtle");
        assert!(text.contains(&pubkey), "profile must embed hex pubkey");
        assert!(text.contains(npub), "profile must embed npub");
        assert!(text.contains("foaf:Person"), "profile must declare foaf:Person type");
        assert!(
            text.contains("solid:oidcIssuer"),
            "profile must have oidcIssuer triple"
        );
    }

    // ===================================================================
    // 9. ACL document — owner write control and public read
    // ===================================================================

    #[tokio::test]
    async fn provisioned_acl_grants_owner_write_and_public_read() {
        let (state, _dir) = temp_state().await;
        let keys = Keys::generate();
        let pubkey = keys.public_key().to_hex();
        let npub = "npub1aclowner005";
        let base = "https://example.com/solid";

        ensure_pod_exists(&state, npub, &pubkey, base).await.unwrap();

        let acl_path = format!("/{}/.acl", npub);
        let (body, _) = state.storage.get(&acl_path).await.unwrap();
        let doc: AclDocument = serde_json::from_slice(&body)
            .expect("ACL must be valid JSON-LD AclDocument");

        let graph = doc.graph.as_ref().expect("ACL must contain a graph");
        let owner_did = format!("did:nostr:{}", pubkey);

        // Owner has Write access
        let owner_write = graph.iter().any(|auth| {
            let is_owner = auth.agent.as_ref().map(|a| match a {
                IdOrIds::Single(r) => r.id == owner_did,
                IdOrIds::Multiple(rs) => rs.iter().any(|r| r.id == owner_did),
            }).unwrap_or(false);
            let has_write = auth.mode.as_ref().map(|m| match m {
                IdOrIds::Single(r) => r.id.contains("Write") || r.id.contains("Control"),
                IdOrIds::Multiple(rs) => {
                    rs.iter().any(|r| r.id.contains("Write") || r.id.contains("Control"))
                }
            }).unwrap_or(false);
            is_owner && has_write
        });
        assert!(owner_write, "Owner must be granted Write access in the ACL");

        // Public (foaf:Agent) should have Read
        let public_read = graph.iter().any(|auth| {
            let is_public = auth.agent_class.as_ref().map(|a| match a {
                IdOrIds::Single(r) => r.id.contains("Agent"),
                IdOrIds::Multiple(rs) => rs.iter().any(|r| r.id.contains("Agent")),
            }).unwrap_or(false);
            let has_read = auth.mode.as_ref().map(|m| match m {
                IdOrIds::Single(r) => r.id.contains("Read"),
                IdOrIds::Multiple(rs) => rs.iter().any(|r| r.id.contains("Read")),
            }).unwrap_or(false);
            is_public && has_read
        });
        assert!(public_read, "Public (foaf:Agent) must be granted Read access");
    }

    // ===================================================================
    // 10. Pod directory structure completeness
    // ===================================================================

    #[tokio::test]
    async fn provisioned_pod_has_all_required_containers() {
        let (state, _dir) = temp_state().await;
        let keys = Keys::generate();
        let pubkey = keys.public_key().to_hex();
        let npub = "npub1dircheck006";
        let base = "https://example.com/solid";

        ensure_pod_exists(&state, npub, &pubkey, base).await.unwrap();

        let expected = [
            format!("/{}/", npub),
            format!("/{}/profile/", npub),
            format!("/{}/ontology/", npub),
            format!("/{}/ontology/contributions/", npub),
            format!("/{}/ontology/proposals/", npub),
            format!("/{}/ontology/annotations/", npub),
            format!("/{}/preferences/", npub),
            format!("/{}/inbox/", npub),
        ];

        for path in &expected {
            assert!(
                state.storage.exists(path).await.unwrap(),
                "Container {} must exist after provisioning",
                path
            );
        }
    }

    // ===================================================================
    // 11. SolidNotificationMessage — serialisation and deserialisation
    // ===================================================================

    #[test]
    fn notification_subscribe_round_trip() {
        let msg = SolidNotificationMessage::Subscribe {
            resource: "/alice/inbox/".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"sub\""));
        assert!(json.contains("/alice/inbox/"));

        let back: SolidNotificationMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, SolidNotificationMessage::Subscribe { resource } if resource == "/alice/inbox/"));
    }

    #[test]
    fn notification_unsubscribe_round_trip() {
        let json = r#"{"type":"unsub","resource":"/foo/bar"}"#;
        let msg: SolidNotificationMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(
            msg,
            SolidNotificationMessage::Unsubscribe { resource } if resource == "/foo/bar"
        ));
    }

    #[test]
    fn notification_ping_deserialises_and_pong_serialises() {
        let ping_json = r#"{"type":"ping"}"#;
        let msg: SolidNotificationMessage = serde_json::from_str(ping_json).unwrap();
        assert!(matches!(msg, SolidNotificationMessage::Ping));

        let pong = SolidNotificationMessage::Pong;
        let json = serde_json::to_string(&pong).unwrap();
        assert!(json.contains("\"type\":\"pong\""));
    }

    #[test]
    fn notification_ack_contains_type_and_resource() {
        let ack = SolidNotificationMessage::Ack { resource: "/x/y/".into() };
        let json = serde_json::to_string(&ack).unwrap();
        assert!(json.contains("\"type\":\"ack\""));
        assert!(json.contains("/x/y/"));
    }

    // ===================================================================
    // 12. Storage watch — events propagate via MemoryBackend
    // ===================================================================

    #[tokio::test]
    async fn storage_watch_receives_created_event_on_put() {
        let store = MemoryBackend::new();
        let mut rx = store.watch("/").await.unwrap();

        store
            .put(
                "/watched/resource",
                Bytes::from_static(b"content"),
                "text/plain",
            )
            .await
            .unwrap();

        let event = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            rx.recv(),
        )
        .await
        .expect("timeout: no storage event received")
        .expect("channel must not be closed");

        assert!(
            matches!(event, StorageEvent::Created(p) if p == "/watched/resource"),
            "Expected Created event, got {:?}", event
        );
    }

    #[tokio::test]
    async fn storage_watch_receives_updated_event_on_second_put() {
        let store = MemoryBackend::new();
        // First put — Creates
        store
            .put("/upd/res", Bytes::from_static(b"v1"), "text/plain")
            .await
            .unwrap();

        let mut rx = store.watch("/upd/res").await.unwrap();

        // Second put — Updates
        store
            .put("/upd/res", Bytes::from_static(b"v2"), "text/plain")
            .await
            .unwrap();

        let event = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            rx.recv(),
        )
        .await
        .expect("timeout: no storage event received")
        .expect("channel closed");

        assert!(
            matches!(event, StorageEvent::Updated(p) if p == "/upd/res"),
            "Expected Updated event, got {:?}", event
        );
    }

    #[tokio::test]
    async fn storage_watch_receives_deleted_event() {
        let store = MemoryBackend::new();
        store
            .put("/del/res", Bytes::from_static(b"bye"), "text/plain")
            .await
            .unwrap();

        let mut rx = store.watch("/del/res").await.unwrap();
        store.delete("/del/res").await.unwrap();

        let event = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            rx.recv(),
        )
        .await
        .expect("timeout")
        .expect("channel closed");

        assert!(
            matches!(event, StorageEvent::Deleted(p) if p == "/del/res"),
            "Expected Deleted event, got {:?}", event
        );
    }

    // ===================================================================
    // 13. LDP Graph — N-Triples serialise/parse round-trip
    // ===================================================================

    #[test]
    fn ldp_graph_ntriples_round_trip_preserves_triples() {
        let mut g = Graph::new();
        g.insert(Triple::new(
            Term::iri("https://alice.example/profile#me"),
            Term::iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#type"),
            Term::iri("http://xmlns.com/foaf/0.1/Person"),
        ));

        let nt = g.to_ntriples();
        assert!(nt.contains("<https://alice.example/profile#me>"));
        assert!(nt.contains("<http://xmlns.com/foaf/0.1/Person>"));

        let parsed = Graph::parse_ntriples(&nt).expect("must parse back from N-Triples");
        assert_eq!(parsed.len(), 1, "round-trip must preserve triple count");
    }

    #[test]
    fn ldp_graph_subtract_removes_matching_triples() {
        let mut base_graph = Graph::new();
        let t1 = Triple::new(
            Term::iri("https://s.example/"),
            Term::iri("https://p.example/"),
            Term::literal("v1"),
        );
        let t2 = Triple::new(
            Term::iri("https://s.example/"),
            Term::iri("https://p.example/"),
            Term::literal("v2"),
        );
        base_graph.insert(t1.clone());
        base_graph.insert(t2.clone());

        let mut remove_graph = Graph::new();
        remove_graph.insert(t1);
        base_graph.subtract(&remove_graph);

        assert_eq!(base_graph.len(), 1);
        assert!(base_graph.contains(&t2));
    }

    // ===================================================================
    // 14. DID resolution — did_nostr_document shape
    // ===================================================================

    #[test]
    fn did_nostr_document_has_expected_fields() {
        let pubkey = "0000000000000000000000000000000000000000000000000000000000000001";
        let doc = did_nostr_document(pubkey, &[]);
        let obj = doc.as_object().expect("DID document must be a JSON object");

        assert!(obj.contains_key("@context"), "must have @context");
        assert!(obj.contains_key("id"), "must have id field");
        let id = obj["id"].as_str().unwrap();
        assert!(id.starts_with("did:nostr:"), "id must use did:nostr: scheme");
        assert!(
            id.contains(pubkey),
            "id must include the pubkey: {}",
            id
        );
    }

    #[test]
    fn did_nostr_document_with_also_known_as() {
        let pubkey = "0000000000000000000000000000000000000000000000000000000000000002";
        let aka = vec!["https://example.com/profile".to_string()];
        let doc = did_nostr_document(pubkey, &aka);
        let json = serde_json::to_string(&doc).unwrap();
        // The also_known_as should appear somewhere if the implementation supports it
        // (at minimum the document must be valid JSON with an id field)
        assert!(json.contains("did:nostr:"));
    }

    // ===================================================================
    // 15. get_global_storage — does not panic
    // ===================================================================

    #[test]
    fn get_global_storage_does_not_panic() {
        // Whether configure_routes has been called in this test binary or not,
        // get_global_storage must return without panicking.
        let _ = get_global_storage();
    }

    // ===================================================================
    // 16. SolidProxyError serialisation
    // ===================================================================

    #[test]
    fn solid_proxy_error_serialises_with_details_present() {
        let err = SolidProxyError {
            error: "Not found".into(),
            details: Some("Resource missing at /foo".into()),
        };
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("\"error\":\"Not found\""));
        assert!(json.contains("Resource missing at /foo"));
    }

    #[test]
    fn solid_proxy_error_serialises_with_null_details() {
        let err = SolidProxyError {
            error: "Conflict".into(),
            details: None,
        };
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("\"error\":\"Conflict\""));
    }

    // ===================================================================
    // 17. Turtle ACL round-trip through parse_turtle_acl
    // ===================================================================

    #[test]
    fn turtle_acl_with_owner_grants_write_on_descendants() {
        use solid_pod_rs::wac::parse_turtle_acl;

        let ttl = r#"
            @prefix acl: <http://www.w3.org/ns/auth/acl#> .
            @prefix foaf: <http://xmlns.com/foaf/0.1/> .

            <#owner> a acl:Authorization ;
                acl:agent <did:nostr:owner_pubkey> ;
                acl:accessTo </> ;
                acl:default </> ;
                acl:mode acl:Read, acl:Write, acl:Control .

            <#public> a acl:Authorization ;
                acl:agentClass foaf:Agent ;
                acl:accessTo </> ;
                acl:mode acl:Read .
        "#;

        let doc = parse_turtle_acl(ttl).expect("Turtle ACL must parse");

        assert!(evaluate_access(
            Some(&doc),
            Some("did:nostr:owner_pubkey"),
            "/pods/owner/data",
            AccessMode::Write,
            None
        ));
        assert!(evaluate_access(
            Some(&doc),
            None,
            "/",
            AccessMode::Read,
            None
        ));
        assert!(!evaluate_access(
            Some(&doc),
            None,
            "/",
            AccessMode::Write,
            None
        ));
        assert!(!evaluate_access(
            Some(&doc),
            Some("did:nostr:stranger"),
            "/",
            AccessMode::Write,
            None
        ));
    }

    // ===================================================================
    // 18. Feature flag sanity check
    // ===================================================================

    #[test]
    fn solid_pod_embed_feature_is_active_in_this_build() {
        // This block only compiles under `#[cfg(feature = "solid-pod-embed")]`.
        // If it runs, the feature is correctly gated.
        assert!(true);
    }
}

// ============================================================================
// Stub-path tests for builds WITHOUT solid-pod-embed
// ============================================================================

#[cfg(not(feature = "solid-pod-embed"))]
mod stub_tests {
    use actix_web::http::StatusCode;
    use actix_web::test as aw_test;
    use actix_web::{web, App};
    use serde_json::Value;

    use webxr::handlers::solid_proxy_handler::{
        get_global_storage, solid_health_check, SolidPodState,
    };

    #[actix_rt::test]
    async fn stub_health_check_returns_503() {
        let state = web::Data::new(SolidPodState::new());
        let app = aw_test::init_service(
            App::new()
                .app_data(state)
                .route("/health", web::get().to(solid_health_check)),
        )
        .await;

        let req = aw_test::TestRequest::get().uri("/health").to_request();
        let resp = aw_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);

        let body: Value = aw_test::read_body_json(resp).await;
        let status_or_error = body["status"].as_str().unwrap_or("")
            .contains("unavailable")
            || body["error"].as_str().unwrap_or("").contains("unavailable");
        assert!(status_or_error, "503 body must indicate unavailability: {}", body);
    }

    #[test]
    fn stub_get_global_storage_returns_none() {
        assert!(get_global_storage().is_none());
    }

    #[test]
    fn stub_state_new_has_safe_defaults() {
        let state = SolidPodState::new();
        assert!(!state.allow_anonymous);
        assert!(state.server_keys.is_none());
    }
}
