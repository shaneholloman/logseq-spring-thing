//! Integration tests for [`webxr::services::server_identity::ServerIdentity`]
//! and [`webxr::actors::ServerNostrActor`].
//!
//! These run with the `test-utils` feature enabled so the `for_test`
//! constructor is accessible from outside the crate.
//!
//! Build:
//!   cargo test --release --features test-utils --test server_identity

#![cfg(feature = "test-utils")]

use std::sync::{Arc, Mutex};

use actix::Actor;
use nostr_sdk::prelude::*;
use serde_json::json;
use uuid::Uuid;

use webxr::actors::{
    ServerNostrActor, SignAuditRecord, SignBeadStamp, SignBridgePromotion,
    SignMigrationApproval,
};
use webxr::services::server_identity::{ServerIdentity, SUPPORTED_KINDS};

/// Serialise env-var mutations so the tests never race each other.
/// `std::env::set_var` is process-global.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn with_env_vars<F: FnOnce()>(pairs: &[(&str, Option<&str>)], f: F) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());

    // Remember previous values so we can restore them.
    let previous: Vec<(String, Option<String>)> = pairs
        .iter()
        .map(|(k, _)| (k.to_string(), std::env::var(k).ok()))
        .collect();

    for (k, v) in pairs {
        match v {
            Some(val) => std::env::set_var(k, val),
            None => std::env::remove_var(k),
        }
    }

    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));

    for (k, v) in previous {
        match v {
            Some(val) => std::env::set_var(&k, val),
            None => std::env::remove_var(&k),
        }
    }

    if let Err(e) = res {
        std::panic::resume_unwind(e);
    }
}

// ── from_env ─────────────────────────────────────────────────────────

#[test]
fn from_env_loads_hex_privkey() {
    with_env_vars(
        &[
            (
                "SERVER_NOSTR_PRIVKEY",
                Some("4242424242424242424242424242424242424242424242424242424242424242"),
            ),
            ("SERVER_NOSTR_AUTO_GENERATE", Some("false")),
            ("APP_ENV", Some("development")),
            ("NOSTR_RELAY_URLS", None),
        ],
        || {
            let identity = ServerIdentity::from_env().expect("must load");
            assert_eq!(identity.pubkey_hex().len(), 64);
            assert!(identity.pubkey_npub().starts_with("npub1"));
            assert!(identity.relay_urls().is_empty());
        },
    );
}

#[test]
fn from_env_loads_nsec_privkey() {
    let sk = SecretKey::from_hex(
        "5151515151515151515151515151515151515151515151515151515151515151",
    )
    .unwrap();
    let nsec = sk.to_bech32().unwrap();
    with_env_vars(
        &[
            ("SERVER_NOSTR_PRIVKEY", Some(nsec.as_str())),
            ("SERVER_NOSTR_AUTO_GENERATE", Some("false")),
            ("APP_ENV", Some("development")),
            ("NOSTR_RELAY_URLS", None),
        ],
        || {
            let identity = ServerIdentity::from_env().expect("nsec must load");
            assert_eq!(identity.pubkey_hex().len(), 64);
        },
    );
}

#[test]
fn from_env_auto_generate_in_dev() {
    with_env_vars(
        &[
            ("SERVER_NOSTR_PRIVKEY", None),
            ("SERVER_NOSTR_AUTO_GENERATE", Some("true")),
            ("APP_ENV", Some("development")),
            ("NOSTR_RELAY_URLS", None),
        ],
        || {
            let identity = ServerIdentity::from_env().expect("auto-generate must succeed");
            assert_eq!(identity.pubkey_hex().len(), 64);
        },
    );
}

#[test]
fn from_env_rejects_missing_key_in_production() {
    with_env_vars(
        &[
            ("SERVER_NOSTR_PRIVKEY", None),
            ("SERVER_NOSTR_AUTO_GENERATE", Some("true")),
            ("APP_ENV", Some("production")),
            ("NOSTR_RELAY_URLS", None),
        ],
        || {
            let result = ServerIdentity::from_env();
            assert!(
                result.is_err(),
                "production with missing key must fail even if auto-generate=true"
            );
        },
    );
}

#[test]
fn from_env_rejects_missing_key_in_dev_without_auto_generate() {
    with_env_vars(
        &[
            ("SERVER_NOSTR_PRIVKEY", None),
            ("SERVER_NOSTR_AUTO_GENERATE", Some("false")),
            ("APP_ENV", Some("development")),
            ("NOSTR_RELAY_URLS", None),
        ],
        || {
            assert!(ServerIdentity::from_env().is_err());
        },
    );
}

#[test]
fn from_env_rejects_invalid_privkey() {
    with_env_vars(
        &[
            ("SERVER_NOSTR_PRIVKEY", Some("not-a-valid-key")),
            ("SERVER_NOSTR_AUTO_GENERATE", Some("false")),
            ("APP_ENV", Some("development")),
            ("NOSTR_RELAY_URLS", None),
        ],
        || {
            assert!(ServerIdentity::from_env().is_err());
        },
    );
}

#[test]
fn from_env_parses_relay_urls() {
    with_env_vars(
        &[
            (
                "SERVER_NOSTR_PRIVKEY",
                Some("4242424242424242424242424242424242424242424242424242424242424242"),
            ),
            (
                "NOSTR_RELAY_URLS",
                Some("wss://relay.damus.io, wss://nos.lol, http://evil.example"),
            ),
            ("APP_ENV", Some("development")),
        ],
        || {
            let identity = ServerIdentity::from_env().unwrap();
            // http:// relay must have been filtered out.
            assert_eq!(
                identity.relay_urls(),
                &[
                    "wss://relay.damus.io".to_string(),
                    "wss://nos.lol".to_string(),
                ]
            );
        },
    );
}

// ── sign_event: each kind round-trips ────────────────────────────────

fn fixed_identity() -> Arc<ServerIdentity> {
    // Hermetic: no env, no relays.
    let sk = SecretKey::from_hex(
        "7777777777777777777777777777777777777777777777777777777777777777",
    )
    .unwrap();
    Arc::new(ServerIdentity::for_test(sk))
}

#[tokio::test]
async fn sign_event_kind_30023_verifies() {
    let id = fixed_identity();
    let event = id
        .sign_event(
            30023,
            json!({"migration_id": "abc"}).to_string(),
            vec![],
        )
        .await
        .unwrap();
    event.verify().unwrap();
    assert_eq!(event.kind.as_u16(), 30023);
    assert_eq!(event.pubkey.to_hex(), id.pubkey_hex());
}

#[tokio::test]
async fn sign_event_all_supported_kinds() {
    let id = fixed_identity();
    for &kind in SUPPORTED_KINDS {
        let event = id
            .sign_event(kind, format!("{{\"k\":{kind}}}"), vec![])
            .await
            .unwrap();
        event.verify().unwrap();
        assert_eq!(event.kind.as_u16(), kind);
    }
}

// ── ServerNostrActor round-trip for each message ─────────────────────

#[actix::test]
async fn actor_migration_approval_tags_present() {
    let addr = ServerNostrActor::new(fixed_identity()).start();
    let migration_id = Uuid::new_v4();
    let event = addr
        .send(SignMigrationApproval {
            migration_id,
            bridge_iri: "http://example.com/bridge/kg:X→owl:Y".into(),
            confidence: 0.88,
        })
        .await
        .unwrap()
        .unwrap();

    event.verify().unwrap();
    assert_eq!(event.kind.as_u16(), 30023);

    let has_migration_id = event
        .tags
        .iter()
        .any(|t| t.kind() == TagKind::Custom("migration_id".into()));
    let has_bridge_iri = event
        .tags
        .iter()
        .any(|t| t.kind() == TagKind::Custom("bridge_iri".into()));
    let has_h = event
        .tags
        .iter()
        .any(|t| t.kind() == TagKind::Custom("h".into()));
    assert!(has_migration_id, "migration_id tag missing");
    assert!(has_bridge_iri, "bridge_iri tag missing");
    assert!(has_h, "h tag missing (NIP-29 group)");

    // Content is valid JSON and contains the migration_id.
    let parsed: serde_json::Value = serde_json::from_str(&event.content).unwrap();
    assert_eq!(
        parsed["migration_id"].as_str().unwrap(),
        migration_id.to_string()
    );
}

#[actix::test]
async fn actor_bridge_promotion_tags_present() {
    let addr = ServerNostrActor::new(fixed_identity()).start();
    let event = addr
        .send(SignBridgePromotion {
            from_kg: "kg:Org".into(),
            to_owl: "schema:Organization".into(),
            signals: vec![0.9, 0.8, 0.7],
        })
        .await
        .unwrap()
        .unwrap();
    event.verify().unwrap();
    assert_eq!(event.kind.as_u16(), 30100);

    let parsed: serde_json::Value = serde_json::from_str(&event.content).unwrap();
    assert_eq!(parsed["from_kg"], "kg:Org");
    assert_eq!(parsed["to_owl"], "schema:Organization");
    assert_eq!(parsed["signals"].as_array().unwrap().len(), 3);
}

#[actix::test]
async fn actor_bead_stamp_tags_present() {
    let addr = ServerNostrActor::new(fixed_identity()).start();
    let event = addr
        .send(SignBeadStamp {
            bead_id: "bead-42".into(),
            payload_hash: "blake3:cafebabe".into(),
        })
        .await
        .unwrap()
        .unwrap();
    event.verify().unwrap();
    assert_eq!(event.kind.as_u16(), 30200);

    let has_bead_id = event
        .tags
        .iter()
        .any(|t| t.kind() == TagKind::Custom("bead_id".into()));
    let has_payload_hash = event
        .tags
        .iter()
        .any(|t| t.kind() == TagKind::Custom("payload_hash".into()));
    assert!(has_bead_id);
    assert!(has_payload_hash);
}

#[actix::test]
async fn actor_audit_record_includes_actor_pubkey_when_present() {
    let addr = ServerNostrActor::new(fixed_identity()).start();
    let event = addr
        .send(SignAuditRecord {
            action: "migration_rollback".into(),
            actor_pubkey: Some(
                "0000000000000000000000000000000000000000000000000000000000000001"
                    .into(),
            ),
            details: json!({"reason": "low confidence"}),
        })
        .await
        .unwrap()
        .unwrap();
    event.verify().unwrap();
    assert_eq!(event.kind.as_u16(), 30300);

    let has_actor_pubkey = event
        .tags
        .iter()
        .any(|t| t.kind() == TagKind::Custom("actor_pubkey".into()));
    assert!(has_actor_pubkey);
}

#[actix::test]
async fn actor_audit_record_without_actor_pubkey() {
    let addr = ServerNostrActor::new(fixed_identity()).start();
    let event = addr
        .send(SignAuditRecord {
            action: "cron_sweep".into(),
            actor_pubkey: None,
            details: json!({"swept": 42}),
        })
        .await
        .unwrap()
        .unwrap();
    event.verify().unwrap();
    let has_actor_pubkey = event
        .tags
        .iter()
        .any(|t| t.kind() == TagKind::Custom("actor_pubkey".into()));
    assert!(
        !has_actor_pubkey,
        "actor_pubkey tag must be absent when actor_pubkey is None"
    );
}
