//! `ServerNostrActor` — actor wrapper around [`ServerIdentity`].
//!
//! Provides message-based, non-blocking access to the server's Nostr signing
//! capability from other actors (physics, ontology, migration, bead lifecycle,
//! audit). Exposes four strongly-typed message variants:
//!
//!   * `SignMigrationApproval` → kind 30023
//!   * `SignBridgePromotion`   → kind 30100
//!   * `SignBeadStamp`         → kind 30200
//!   * `SignAuditRecord`       → kind 30300
//!
//! Each handler assembles the appropriate tags + JSON content, signs via the
//! shared [`ServerIdentity`] (no key copy — identity is held behind `Arc`),
//! and best-effort broadcasts to configured relays. The signed
//! [`nostr_sdk::Event`] is returned to the caller regardless of relay outcome.

use std::sync::Arc;

use actix::prelude::*;
use anyhow::Result;
use log::info;
use nostr_sdk::prelude::*;
use serde_json::json;
use uuid::Uuid;

use crate::services::server_identity::ServerIdentity;

/// Default value of the NIP-29 group `h` tag applied to every server-signed
/// event so Nostr forum relays (DreamLab Forum relay) accept them.
const SERVER_H_TAG: &str = "visionclaw-server";

// ── Actor ──────────────────────────────────────────────────────────────

/// Actor wrapping the server's Nostr identity.
///
/// Cheap to clone (identity is behind `Arc`). Register one instance in the
/// supervisor / `SystemRegistry` and send signing messages to it from any
/// other actor.
pub struct ServerNostrActor {
    identity: Arc<ServerIdentity>,
}

impl ServerNostrActor {
    /// Construct a new actor over the given identity. Use
    /// `actix::Actor::start` to get an `Addr<Self>`.
    pub fn new(identity: Arc<ServerIdentity>) -> Self {
        Self { identity }
    }

    /// Shared pubkey (hex) — useful for log lines.
    pub fn pubkey_hex(&self) -> String {
        self.identity.pubkey_hex()
    }
}

impl Actor for ServerNostrActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        info!(
            "[ServerNostrActor] Started. pubkey={}",
            self.identity.pubkey_hex()
        );
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        info!("[ServerNostrActor] Stopped.");
    }
}

// ── Message: migration approval (kind 30023) ──────────────────────────

/// Sign a migration approval event.
///
/// Emitted when the server confirms promotion of a migration candidate
/// (bridge IRI, confidence) to the authoritative OWL layer.
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<Event>")]
pub struct SignMigrationApproval {
    pub migration_id: Uuid,
    pub bridge_iri: String,
    pub confidence: f64,
}

impl Handler<SignMigrationApproval> for ServerNostrActor {
    type Result = ResponseFuture<Result<Event>>;

    fn handle(&mut self, msg: SignMigrationApproval, _ctx: &mut Self::Context) -> Self::Result {
        let identity = Arc::clone(&self.identity);
        Box::pin(async move {
            let content = json!({
                "migration_id": msg.migration_id.to_string(),
                "bridge_iri": msg.bridge_iri,
                "confidence": msg.confidence,
                "approved_at": chrono::Utc::now().to_rfc3339(),
            })
            .to_string();

            let tags = vec![
                Tag::custom(
                    TagKind::Custom("h".into()),
                    vec![SERVER_H_TAG.to_string()],
                ),
                Tag::custom(
                    TagKind::Custom("d".into()),
                    vec![format!("migration:{}", msg.migration_id)],
                ),
                Tag::custom(
                    TagKind::Custom("migration_id".into()),
                    vec![msg.migration_id.to_string()],
                ),
                Tag::custom(
                    TagKind::Custom("bridge_iri".into()),
                    vec![msg.bridge_iri.clone()],
                ),
                Tag::custom(
                    TagKind::Custom("confidence".into()),
                    vec![format!("{:.6}", msg.confidence)],
                ),
                Tag::custom(
                    TagKind::Custom("event_type".into()),
                    vec!["migration_approval".to_string()],
                ),
            ];

            identity.sign_and_broadcast(30023, content, tags).await
        })
    }
}

// ── Message: bridge promotion (kind 30100) ─────────────────────────────

/// Sign a BRIDGE_TO promotion event: KG edge → OWL ObjectProperty/Subclass.
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<Event>")]
pub struct SignBridgePromotion {
    pub from_kg: String,
    pub to_owl: String,
    pub signals: Vec<f64>,
}

impl Handler<SignBridgePromotion> for ServerNostrActor {
    type Result = ResponseFuture<Result<Event>>;

    fn handle(&mut self, msg: SignBridgePromotion, _ctx: &mut Self::Context) -> Self::Result {
        let identity = Arc::clone(&self.identity);
        Box::pin(async move {
            let content = json!({
                "from_kg": msg.from_kg,
                "to_owl": msg.to_owl,
                "signals": msg.signals,
                "promoted_at": chrono::Utc::now().to_rfc3339(),
            })
            .to_string();

            let tags = vec![
                Tag::custom(
                    TagKind::Custom("h".into()),
                    vec![SERVER_H_TAG.to_string()],
                ),
                Tag::custom(
                    TagKind::Custom("d".into()),
                    vec![format!("bridge:{}→{}", msg.from_kg, msg.to_owl)],
                ),
                Tag::custom(
                    TagKind::Custom("from_kg".into()),
                    vec![msg.from_kg.clone()],
                ),
                Tag::custom(
                    TagKind::Custom("to_owl".into()),
                    vec![msg.to_owl.clone()],
                ),
                Tag::custom(
                    TagKind::Custom("event_type".into()),
                    vec!["bridge_promotion".to_string()],
                ),
            ];

            identity.sign_and_broadcast(30100, content, tags).await
        })
    }
}

// ── Message: bead provenance stamp (kind 30200) ────────────────────────

/// Sign a bead provenance stamp. Distinct from `NostrBeadPublisher`
/// (which uses kind 30001 on JSS). Server-issued kind 30200 asserts that
/// **the server** has witnessed and hashed the bead payload.
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<Event>")]
pub struct SignBeadStamp {
    pub bead_id: String,
    pub payload_hash: String,
}

impl Handler<SignBeadStamp> for ServerNostrActor {
    type Result = ResponseFuture<Result<Event>>;

    fn handle(&mut self, msg: SignBeadStamp, _ctx: &mut Self::Context) -> Self::Result {
        let identity = Arc::clone(&self.identity);
        Box::pin(async move {
            let content = json!({
                "bead_id": msg.bead_id,
                "payload_hash": msg.payload_hash,
                "stamped_at": chrono::Utc::now().to_rfc3339(),
            })
            .to_string();

            let tags = vec![
                Tag::custom(
                    TagKind::Custom("h".into()),
                    vec![SERVER_H_TAG.to_string()],
                ),
                Tag::custom(
                    TagKind::Custom("d".into()),
                    vec![format!("bead-stamp:{}", msg.bead_id)],
                ),
                Tag::custom(
                    TagKind::Custom("bead_id".into()),
                    vec![msg.bead_id.clone()],
                ),
                Tag::custom(
                    TagKind::Custom("payload_hash".into()),
                    vec![msg.payload_hash.clone()],
                ),
                Tag::custom(
                    TagKind::Custom("event_type".into()),
                    vec!["bead_stamp".to_string()],
                ),
            ];

            identity.sign_and_broadcast(30200, content, tags).await
        })
    }
}

// ── Message: audit record (kind 30300) ─────────────────────────────────

/// Sign an audit record. `actor_pubkey` is optional because some audit events
/// (server-initiated cron jobs, reconciliations) have no originating user.
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<Event>")]
pub struct SignAuditRecord {
    pub action: String,
    pub actor_pubkey: Option<String>,
    pub details: serde_json::Value,
}

impl Handler<SignAuditRecord> for ServerNostrActor {
    type Result = ResponseFuture<Result<Event>>;

    fn handle(&mut self, msg: SignAuditRecord, _ctx: &mut Self::Context) -> Self::Result {
        let identity = Arc::clone(&self.identity);
        Box::pin(async move {
            let content = json!({
                "action": msg.action,
                "actor_pubkey": msg.actor_pubkey,
                "details": msg.details,
                "recorded_at": chrono::Utc::now().to_rfc3339(),
            })
            .to_string();

            // Every audit record gets a unique `d` so nothing is silently replaced.
            let audit_id = Uuid::new_v4();

            let mut tags = vec![
                Tag::custom(
                    TagKind::Custom("h".into()),
                    vec![SERVER_H_TAG.to_string()],
                ),
                Tag::custom(
                    TagKind::Custom("d".into()),
                    vec![format!("audit:{}", audit_id)],
                ),
                Tag::custom(
                    TagKind::Custom("action".into()),
                    vec![msg.action.clone()],
                ),
                Tag::custom(
                    TagKind::Custom("audit_id".into()),
                    vec![audit_id.to_string()],
                ),
                Tag::custom(
                    TagKind::Custom("event_type".into()),
                    vec!["audit_record".to_string()],
                ),
            ];
            if let Some(pk) = msg.actor_pubkey.as_ref() {
                tags.push(Tag::custom(
                    TagKind::Custom("actor_pubkey".into()),
                    vec![pk.clone()],
                ));
            }

            identity.sign_and_broadcast(30300, content, tags).await
        })
    }
}

// ── tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn test_identity() -> Arc<ServerIdentity> {
        // Fresh deterministic identity — tests never touch env.
        let sk = nostr_sdk::SecretKey::from_hex(
            "1111111111111111111111111111111111111111111111111111111111111111",
        )
        .unwrap();
        // SAFETY: constructor only takes env, so we build the actor via a
        // wrapper constructor exposed in tests.
        Arc::new(ServerIdentity::for_test(sk))
    }

    #[actix::test]
    async fn handles_migration_approval() {
        let addr = ServerNostrActor::new(test_identity()).start();
        let event = addr
            .send(SignMigrationApproval {
                migration_id: Uuid::nil(),
                bridge_iri: "http://example.com/bridge/Thing→foaf:Person".to_string(),
                confidence: 0.93,
            })
            .await
            .expect("mailbox")
            .expect("sign");
        event.verify().expect("signature");
        assert_eq!(event.kind.as_u16(), 30023);
    }

    #[actix::test]
    async fn handles_bridge_promotion() {
        let addr = ServerNostrActor::new(test_identity()).start();
        let event = addr
            .send(SignBridgePromotion {
                from_kg: "kg:Company".to_string(),
                to_owl: "schema:Organization".to_string(),
                signals: vec![0.91, 0.87, 0.99],
            })
            .await
            .expect("mailbox")
            .expect("sign");
        event.verify().expect("signature");
        assert_eq!(event.kind.as_u16(), 30100);
    }

    #[actix::test]
    async fn handles_bead_stamp() {
        let addr = ServerNostrActor::new(test_identity()).start();
        let event = addr
            .send(SignBeadStamp {
                bead_id: "bead-abc".to_string(),
                payload_hash: "blake3:deadbeef".to_string(),
            })
            .await
            .expect("mailbox")
            .expect("sign");
        event.verify().expect("signature");
        assert_eq!(event.kind.as_u16(), 30200);
    }

    #[actix::test]
    async fn handles_audit_record() {
        let addr = ServerNostrActor::new(test_identity()).start();
        let event = addr
            .send(SignAuditRecord {
                action: "migration_rollback".to_string(),
                actor_pubkey: Some(
                    "0000000000000000000000000000000000000000000000000000000000000001"
                        .to_string(),
                ),
                details: json!({"reason": "confidence below threshold"}),
            })
            .await
            .expect("mailbox")
            .expect("sign");
        event.verify().expect("signature");
        assert_eq!(event.kind.as_u16(), 30300);
        // Audit records must always carry an action tag.
        let has_action = event
            .tags
            .iter()
            .any(|t| t.kind() == TagKind::Custom("action".into()));
        assert!(has_action);
    }
}
