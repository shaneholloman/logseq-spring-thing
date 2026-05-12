//! `ServerNostrActor` — actor wrapper around [`ServerIdentity`].
//!
//! Provides message-based, non-blocking access to the server's Nostr signing
//! capability from other actors (physics, ontology, migration, bead lifecycle,
//! audit, broker). Exposes nine strongly-typed message variants:
//!
//!   * `SignMigrationApproval`     → kind 30023
//!   * `SignBridgePromotion`       → kind 30100
//!   * `SignBeadStamp`             → kind 30200
//!   * `SignAuditRecord`           → kind 30300
//!   * `SignEnrichmentProposal`    → kind 30301 (PRD-013 G7)
//!   * `SignBrokerDecision`        → kind 30300 (PRD-013 G7, enrichment variant)
//!   * `SignSealedDM`              → kind 14 (NIP-17 chat message)
//!   * `PublishGovernancePanel`    → kind 31400 (Agent Control Surface panel definition)
//!   * `PublishActionRequest`      → kind 31402 (Agent Control Surface action request)
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

use crate::services::metrics::{MetricsRegistry, NostrKind, NostrKindLabels};
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
    prom: Option<Arc<MetricsRegistry>>,
}

impl ServerNostrActor {
    /// Construct a new actor over the given identity. Use
    /// `actix::Actor::start` to get an `Addr<Self>`.
    pub fn new(identity: Arc<ServerIdentity>) -> Self {
        Self {
            identity,
            prom: None,
        }
    }

    /// Attach a Prometheus metrics registry. Returns `self` for fluent use.
    pub fn with_prom(mut self, prom: Arc<MetricsRegistry>) -> Self {
        self.prom = Some(prom);
        self
    }

    /// Increment the `signed_total` counter for the given Nostr event kind,
    /// or the broadcast-errors counter if the signing outcome was an error.
    fn observe_sign_outcome(
        prom: &Option<Arc<MetricsRegistry>>,
        kind: NostrKind,
        outcome: &Result<Event>,
    ) {
        let Some(prom) = prom.as_ref() else { return };
        match outcome {
            Ok(_) => {
                prom.server_nostr_signed_total
                    .get_or_create(&NostrKindLabels { kind })
                    .inc();
            }
            Err(_) => {
                prom.server_nostr_broadcast_errors_total.inc();
            }
        }
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
        let prom = self.prom.clone();
        Box::pin(async move {
            let content = json!({
                "migration_id": msg.migration_id.to_string(),
                "bridge_iri": msg.bridge_iri,
                "confidence": msg.confidence,
                "approved_at": chrono::Utc::now().to_rfc3339(),
            })
            .to_string();

            let tags = vec![
                Tag::custom(TagKind::Custom("h".into()), vec![SERVER_H_TAG.to_string()]),
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

            let out = identity.sign_and_broadcast(30023, content, tags).await;
            Self::observe_sign_outcome(&prom, NostrKind::K30023, &out);
            out
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
        let prom = self.prom.clone();
        Box::pin(async move {
            let content = json!({
                "from_kg": msg.from_kg,
                "to_owl": msg.to_owl,
                "signals": msg.signals,
                "promoted_at": chrono::Utc::now().to_rfc3339(),
            })
            .to_string();

            let tags = vec![
                Tag::custom(TagKind::Custom("h".into()), vec![SERVER_H_TAG.to_string()]),
                Tag::custom(
                    TagKind::Custom("d".into()),
                    vec![format!("bridge:{}→{}", msg.from_kg, msg.to_owl)],
                ),
                Tag::custom(TagKind::Custom("from_kg".into()), vec![msg.from_kg.clone()]),
                Tag::custom(TagKind::Custom("to_owl".into()), vec![msg.to_owl.clone()]),
                Tag::custom(
                    TagKind::Custom("event_type".into()),
                    vec!["bridge_promotion".to_string()],
                ),
            ];

            let out = identity.sign_and_broadcast(30100, content, tags).await;
            Self::observe_sign_outcome(&prom, NostrKind::K30100, &out);
            out
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
        let prom = self.prom.clone();
        Box::pin(async move {
            let content = json!({
                "bead_id": msg.bead_id,
                "payload_hash": msg.payload_hash,
                "stamped_at": chrono::Utc::now().to_rfc3339(),
            })
            .to_string();

            let tags = vec![
                Tag::custom(TagKind::Custom("h".into()), vec![SERVER_H_TAG.to_string()]),
                Tag::custom(
                    TagKind::Custom("d".into()),
                    vec![format!("bead-stamp:{}", msg.bead_id)],
                ),
                Tag::custom(TagKind::Custom("bead_id".into()), vec![msg.bead_id.clone()]),
                Tag::custom(
                    TagKind::Custom("payload_hash".into()),
                    vec![msg.payload_hash.clone()],
                ),
                Tag::custom(
                    TagKind::Custom("event_type".into()),
                    vec!["bead_stamp".to_string()],
                ),
            ];

            let out = identity.sign_and_broadcast(30200, content, tags).await;
            Self::observe_sign_outcome(&prom, NostrKind::K30200, &out);
            out
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
        let prom = self.prom.clone();
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
                Tag::custom(TagKind::Custom("h".into()), vec![SERVER_H_TAG.to_string()]),
                Tag::custom(
                    TagKind::Custom("d".into()),
                    vec![format!("audit:{}", audit_id)],
                ),
                Tag::custom(TagKind::Custom("action".into()), vec![msg.action.clone()]),
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

            let out = identity.sign_and_broadcast(30300, content, tags).await;
            Self::observe_sign_outcome(&prom, NostrKind::K30300, &out);
            out
        })
    }
}

// ── Message: enrichment proposal (kind 30301) ─────────────────────────

/// Sign an enrichment proposal event (PRD-013 G7 Nostr control plane).
///
/// Emitted when an agent (via agentbox git-bridge) submits a knowledge-graph
/// enrichment for broker gating. The signed event captures the proposal
/// metadata so third-party watchers on the relay can follow the enrichment
/// lifecycle.
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<Event>")]
pub struct SignEnrichmentProposal {
    pub case_id: String,
    pub agent_did: String,
    pub entity_urn: String,
    pub enrichment_type: String,
    pub target_path: String,
    pub reasoning_hash: String,
}

impl Handler<SignEnrichmentProposal> for ServerNostrActor {
    type Result = ResponseFuture<Result<Event>>;

    fn handle(&mut self, msg: SignEnrichmentProposal, _ctx: &mut Self::Context) -> Self::Result {
        let identity = Arc::clone(&self.identity);
        let prom = self.prom.clone();
        Box::pin(async move {
            let content = json!({
                "case_id": msg.case_id,
                "agent_did": msg.agent_did,
                "entity_urn": msg.entity_urn,
                "enrichment_type": msg.enrichment_type,
                "target_path": msg.target_path,
                "reasoning_hash": msg.reasoning_hash,
                "proposed_at": chrono::Utc::now().to_rfc3339(),
            })
            .to_string();

            // Extract the agent pubkey hex from the DID if it follows
            // `did:nostr:<hex>` convention; fall back to the raw string.
            let agent_pubkey_hex = msg
                .agent_did
                .strip_prefix("did:nostr:")
                .unwrap_or(&msg.agent_did)
                .to_string();

            let tags = vec![
                Tag::custom(TagKind::Custom("d".into()), vec![msg.case_id.clone()]),
                Tag::custom(TagKind::Custom("p".into()), vec![agent_pubkey_hex]),
                Tag::custom(TagKind::Custom("urn".into()), vec![msg.entity_urn.clone()]),
                Tag::custom(
                    TagKind::Custom("enrichment_type".into()),
                    vec![msg.enrichment_type.clone()],
                ),
                Tag::custom(
                    TagKind::Custom("target_path".into()),
                    vec![msg.target_path.clone()],
                ),
                Tag::custom(TagKind::Custom("h".into()), vec![SERVER_H_TAG.to_string()]),
                Tag::custom(
                    TagKind::Custom("event_type".into()),
                    vec!["enrichment_proposal".to_string()],
                ),
            ];

            let out = identity.sign_and_broadcast(30301, content, tags).await;
            Self::observe_sign_outcome(&prom, NostrKind::K30301, &out);
            out
        })
    }
}

// ── Message: broker decision on enrichment (kind 30300) ───────────────

/// Sign a broker decision event specifically for `KnowledgeEnrichment` cases
/// (PRD-013 G7). Reuses kind 30300 (audit) with enrichment-specific tags so
/// the relay can correlate proposals (30301) with their decisions.
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<Event>")]
pub struct SignBrokerDecision {
    pub case_id: String,
    pub decision_id: String,
    pub outcome_action: String,
    pub broker_pubkey: String,
    pub entity_urn: String,
    pub reasoning: String,
}

impl Handler<SignBrokerDecision> for ServerNostrActor {
    type Result = ResponseFuture<Result<Event>>;

    fn handle(&mut self, msg: SignBrokerDecision, _ctx: &mut Self::Context) -> Self::Result {
        let identity = Arc::clone(&self.identity);
        let prom = self.prom.clone();
        Box::pin(async move {
            let content = json!({
                "case_id": msg.case_id,
                "decision_id": msg.decision_id,
                "outcome_action": msg.outcome_action,
                "broker_pubkey": msg.broker_pubkey,
                "entity_urn": msg.entity_urn,
                "reasoning": msg.reasoning,
                "decided_at": chrono::Utc::now().to_rfc3339(),
            })
            .to_string();

            let tags = vec![
                Tag::custom(TagKind::Custom("d".into()), vec![msg.case_id.clone()]),
                Tag::custom(
                    TagKind::Custom("decision".into()),
                    vec![msg.outcome_action.clone()],
                ),
                Tag::custom(
                    TagKind::Custom("broker".into()),
                    vec![msg.broker_pubkey.clone()],
                ),
                Tag::custom(TagKind::Custom("urn".into()), vec![msg.entity_urn.clone()]),
                Tag::custom(TagKind::Custom("h".into()), vec![SERVER_H_TAG.to_string()]),
                Tag::custom(
                    TagKind::Custom("event_type".into()),
                    vec!["broker_decision".to_string()],
                ),
            ];

            let out = identity.sign_and_broadcast(30300, content, tags).await;
            Self::observe_sign_outcome(&prom, NostrKind::K30300, &out);
            out
        })
    }
}

// ── Message: NIP-17 sealed direct message (kind 14) ───────────────────

/// Send a sealed DM to a specific recipient via NIP-17.
///
/// Used for broker-to-agent dialogue (e.g. requesting clarification on an
/// enrichment proposal). The inner message is a kind 14 chat event with
/// a `p` tag addressing the recipient. Full NIP-17 gift-wrapping (kind 1059)
/// can be layered on top by the relay or a future middleware; the actor signs
/// the plaintext kind 14 rumor which is the semantic payload.
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<Event>")]
pub struct SignSealedDM {
    pub recipient_pubkey_hex: String,
    pub content: String,
    pub subject: Option<String>,
}

impl Handler<SignSealedDM> for ServerNostrActor {
    type Result = ResponseFuture<Result<Event>>;

    fn handle(&mut self, msg: SignSealedDM, _ctx: &mut Self::Context) -> Self::Result {
        let identity = Arc::clone(&self.identity);
        let prom = self.prom.clone();
        Box::pin(async move {
            let mut tags = vec![Tag::custom(
                TagKind::Custom("p".into()),
                vec![msg.recipient_pubkey_hex.clone()],
            )];
            if let Some(ref subj) = msg.subject {
                tags.push(Tag::custom(
                    TagKind::Custom("subject".into()),
                    vec![subj.clone()],
                ));
            }

            let out = identity.sign_and_broadcast(14, msg.content, tags).await;
            Self::observe_sign_outcome(&prom, NostrKind::K14, &out);
            out
        })
    }
}

// ── Agent Control Surface Protocol types ──────────────────────────────
//
// Local struct definitions that produce JSON matching the nostr-bbs-core
// PanelDefinition / ActionRequest schema. Kept local to avoid coupling
// VisionClaw's dependency tree to the nostr-bbs crate.

/// Panel schema type. Matches `nostr-bbs-core::governance::PanelSchema`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PanelSchema {
    ActionInbox,
    Dashboard,
    ConfigForm,
    StatusBoard,
    ChatBridge,
}

/// Layout hint. Matches `nostr-bbs-core::governance::LayoutHint`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum LayoutHint {
    InboxTable,
    Kanban,
    CardGrid,
    SplitDetail,
}

/// Field type tag. Matches `nostr-bbs-core::governance::FieldType`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum FieldType {
    String,
    Int,
    Float,
    Bool,
    Json,
    Enum,
    Timestamp,
}

/// Field definition inside a panel.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FieldDef {
    pub name: String,
    pub field_type: FieldType,
    pub label: String,
}

/// Button style for panel actions.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ActionStyle {
    Primary,
    Secondary,
    Destructive,
}

/// An action button on a panel.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ActionDef {
    pub id: String,
    pub label: String,
    pub style: ActionStyle,
}

/// Full panel definition payload — serialised to JSON as the `content` of
/// a kind 31400 event. Wire-compatible with `nostr-bbs-core`'s
/// `PanelDefinition`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PanelDefinitionPayload {
    pub title: String,
    pub description: String,
    #[serde(default = "default_panel_version")]
    pub version: String,
    pub schema: PanelSchema,
    pub fields: Vec<FieldDef>,
    pub actions: Vec<ActionDef>,
    pub layout: LayoutHint,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default = "default_refresh_secs")]
    pub refresh_secs: u32,
}

fn default_panel_version() -> String {
    "1.0.0".into()
}

fn default_refresh_secs() -> u32 {
    30
}

/// Priority level for action requests.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActionPriority {
    Critical,
    High,
    Medium,
    Low,
}

// ── Message: publish governance panel definition (kind 31400) ─────────

/// Publish a PanelDefinition event (kind 31400) to the relay.
///
/// The BrokerActor sends this when it wants to register or update a
/// control panel on the Nostr forum. The `panel_id` becomes the `d` tag
/// so the event is a NIP-33 parameterized replaceable event.
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<Event>")]
pub struct PublishGovernancePanel {
    /// Stable identifier for the panel (`d` tag value).
    pub panel_id: String,
    /// The full panel definition payload.
    pub panel: PanelDefinitionPayload,
}

impl Handler<PublishGovernancePanel> for ServerNostrActor {
    type Result = ResponseFuture<Result<Event>>;

    fn handle(&mut self, msg: PublishGovernancePanel, _ctx: &mut Self::Context) -> Self::Result {
        let identity = Arc::clone(&self.identity);
        let prom = self.prom.clone();
        Box::pin(async move {
            let content = serde_json::to_string(&msg.panel)
                .unwrap_or_else(|e| format!("{{\"error\":\"{}\"}}", e));

            let tags = vec![
                Tag::custom(TagKind::Custom("h".into()), vec![SERVER_H_TAG.to_string()]),
                Tag::custom(TagKind::Custom("d".into()), vec![msg.panel_id.clone()]),
                Tag::custom(
                    TagKind::Custom("schema".into()),
                    vec![serde_json::to_value(&msg.panel.schema)
                        .ok()
                        .and_then(|v| v.as_str().map(|s| s.to_string()))
                        .unwrap_or_else(|| "action-inbox".to_string())],
                ),
                Tag::custom(
                    TagKind::Custom("layout".into()),
                    vec![serde_json::to_value(&msg.panel.layout)
                        .ok()
                        .and_then(|v| v.as_str().map(|s| s.to_string()))
                        .unwrap_or_else(|| "inbox-table".to_string())],
                ),
                Tag::custom(
                    TagKind::Custom("event_type".into()),
                    vec!["panel_definition".to_string()],
                ),
            ];

            let out = identity.sign_and_broadcast(31400, content, tags).await;
            Self::observe_sign_outcome(&prom, NostrKind::K31400, &out);
            out
        })
    }
}

// ── Message: publish action request (kind 31402) ──────────────────────

/// Publish an ActionRequest event (kind 31402) to the relay.
///
/// The BrokerActor sends this when a new case needs human review. The
/// event content is a JSON object with the case fields and reasoning,
/// matching the `ActionRequest` schema from nostr-bbs-core.
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<Event>")]
pub struct PublishActionRequest {
    /// Case identifier — used as `d` tag value.
    pub case_id: String,
    /// Human-readable title for the action request.
    pub title: String,
    /// Case category (e.g. "knowledge_enrichment", "manual_submission").
    pub category: String,
    /// Priority level.
    pub priority: ActionPriority,
    /// Structured fields for the action (JSON object).
    pub fields: serde_json::Value,
    /// Agent's reasoning / justification for the request.
    pub reasoning: String,
}

impl Handler<PublishActionRequest> for ServerNostrActor {
    type Result = ResponseFuture<Result<Event>>;

    fn handle(&mut self, msg: PublishActionRequest, _ctx: &mut Self::Context) -> Self::Result {
        let identity = Arc::clone(&self.identity);
        let prom = self.prom.clone();
        Box::pin(async move {
            let content = json!({
                "fields": msg.fields,
                "reasoning": msg.reasoning,
            })
            .to_string();

            let priority_str = serde_json::to_value(&msg.priority)
                .ok()
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "medium".to_string());

            let tags = vec![
                Tag::custom(TagKind::Custom("h".into()), vec![SERVER_H_TAG.to_string()]),
                Tag::custom(TagKind::Custom("d".into()), vec![msg.case_id.clone()]),
                Tag::custom(
                    TagKind::Custom("title".into()),
                    vec![msg.title.clone()],
                ),
                Tag::custom(
                    TagKind::Custom("category".into()),
                    vec![msg.category.clone()],
                ),
                Tag::custom(
                    TagKind::Custom("priority".into()),
                    vec![priority_str],
                ),
                Tag::custom(
                    TagKind::Custom("event_type".into()),
                    vec!["action_request".to_string()],
                ),
            ];

            let out = identity.sign_and_broadcast(31402, content, tags).await;
            Self::observe_sign_outcome(&prom, NostrKind::K31402, &out);
            out
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
                    "0000000000000000000000000000000000000000000000000000000000000001".to_string(),
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

    #[actix::test]
    async fn handles_enrichment_proposal() {
        let addr = ServerNostrActor::new(test_identity()).start();
        let event = addr
            .send(SignEnrichmentProposal {
                case_id: "enrich-001".to_string(),
                agent_did:
                    "did:nostr:0000000000000000000000000000000000000000000000000000000000000002"
                        .to_string(),
                entity_urn: "urn:visionclaw:concept:abc123".to_string(),
                enrichment_type: "property_addition".to_string(),
                target_path: "pages/Quantum_Computing.md".to_string(),
                reasoning_hash: "blake3:cafebabe".to_string(),
            })
            .await
            .expect("mailbox")
            .expect("sign");
        event.verify().expect("signature");
        assert_eq!(event.kind.as_u16(), 30301);
        // Must carry the proposer `p` tag.
        let has_p = event
            .tags
            .iter()
            .any(|t| t.kind() == TagKind::Custom("p".into()));
        assert!(has_p);
        // Must carry the entity URN tag.
        let has_urn = event
            .tags
            .iter()
            .any(|t| t.kind() == TagKind::Custom("urn".into()));
        assert!(has_urn);
    }

    #[actix::test]
    async fn handles_broker_decision() {
        let addr = ServerNostrActor::new(test_identity()).start();
        let event = addr
            .send(SignBrokerDecision {
                case_id: "enrich-001".to_string(),
                decision_id: "dec-99".to_string(),
                outcome_action: "approve".to_string(),
                broker_pubkey: "0000000000000000000000000000000000000000000000000000000000000003"
                    .to_string(),
                entity_urn: "urn:visionclaw:concept:abc123".to_string(),
                reasoning: "enrichment validated against source".to_string(),
            })
            .await
            .expect("mailbox")
            .expect("sign");
        event.verify().expect("signature");
        assert_eq!(event.kind.as_u16(), 30300);
        // Must carry the decision tag.
        let has_decision = event
            .tags
            .iter()
            .any(|t| t.kind() == TagKind::Custom("decision".into()));
        assert!(has_decision);
        // Must carry the broker tag.
        let has_broker = event
            .tags
            .iter()
            .any(|t| t.kind() == TagKind::Custom("broker".into()));
        assert!(has_broker);
    }

    #[actix::test]
    async fn handles_sealed_dm() {
        let addr = ServerNostrActor::new(test_identity()).start();
        let event = addr
            .send(SignSealedDM {
                recipient_pubkey_hex:
                    "0000000000000000000000000000000000000000000000000000000000000004".to_string(),
                content: "Clarification needed on proposal enrich-001".to_string(),
                subject: Some("Re: enrichment proposal".to_string()),
            })
            .await
            .expect("mailbox")
            .expect("sign");
        event.verify().expect("signature");
        assert_eq!(event.kind.as_u16(), 14);
        // Must carry a `p` tag for the recipient.
        let has_p = event
            .tags
            .iter()
            .any(|t| t.kind() == TagKind::Custom("p".into()));
        assert!(has_p);
        // Must carry the subject tag when provided.
        let has_subject = event
            .tags
            .iter()
            .any(|t| t.kind() == TagKind::Custom("subject".into()));
        assert!(has_subject);
    }

    #[actix::test]
    async fn handles_sealed_dm_without_subject() {
        let addr = ServerNostrActor::new(test_identity()).start();
        let event = addr
            .send(SignSealedDM {
                recipient_pubkey_hex:
                    "0000000000000000000000000000000000000000000000000000000000000004".to_string(),
                content: "Simple message".to_string(),
                subject: None,
            })
            .await
            .expect("mailbox")
            .expect("sign");
        event.verify().expect("signature");
        assert_eq!(event.kind.as_u16(), 14);
        // No subject tag when None.
        let has_subject = event
            .tags
            .iter()
            .any(|t| t.kind() == TagKind::Custom("subject".into()));
        assert!(!has_subject);
    }

    #[actix::test]
    async fn handles_publish_governance_panel() {
        let addr = ServerNostrActor::new(test_identity()).start();
        let event = addr
            .send(PublishGovernancePanel {
                panel_id: "broker-inbox-v1".to_string(),
                panel: PanelDefinitionPayload {
                    title: "Broker Inbox".to_string(),
                    description: "Cases awaiting broker review".to_string(),
                    version: "1.0.0".to_string(),
                    schema: PanelSchema::ActionInbox,
                    fields: vec![
                        FieldDef {
                            name: "case_id".to_string(),
                            field_type: FieldType::String,
                            label: "Case ID".to_string(),
                        },
                        FieldDef {
                            name: "priority".to_string(),
                            field_type: FieldType::Enum,
                            label: "Priority".to_string(),
                        },
                    ],
                    actions: vec![
                        ActionDef {
                            id: "approve".to_string(),
                            label: "Approve".to_string(),
                            style: ActionStyle::Primary,
                        },
                        ActionDef {
                            id: "reject".to_string(),
                            label: "Reject".to_string(),
                            style: ActionStyle::Destructive,
                        },
                    ],
                    layout: LayoutHint::InboxTable,
                    capabilities: vec!["bulk-action".to_string(), "filter".to_string()],
                    refresh_secs: 15,
                },
            })
            .await
            .expect("mailbox")
            .expect("sign");
        event.verify().expect("signature");
        assert_eq!(event.kind.as_u16(), 31400);
        // Must carry a `d` tag with the panel id.
        let has_d = event
            .tags
            .iter()
            .any(|t| t.kind() == TagKind::Custom("d".into()));
        assert!(has_d);
        // Must carry the schema tag.
        let has_schema = event
            .tags
            .iter()
            .any(|t| t.kind() == TagKind::Custom("schema".into()));
        assert!(has_schema);
        // Content must be valid JSON containing the title.
        let parsed: serde_json::Value = serde_json::from_str(&event.content).expect("valid JSON");
        assert_eq!(parsed["title"], "Broker Inbox");
        assert_eq!(parsed["fields"].as_array().unwrap().len(), 2);
    }

    #[actix::test]
    async fn handles_publish_action_request() {
        let addr = ServerNostrActor::new(test_identity()).start();
        let event = addr
            .send(PublishActionRequest {
                case_id: "case-enrich-042".to_string(),
                title: "Review enrichment proposal for Quantum Computing".to_string(),
                category: "knowledge_enrichment".to_string(),
                priority: ActionPriority::High,
                fields: json!({
                    "entity_urn": "urn:visionclaw:concept:quantum_computing",
                    "enrichment_type": "property_addition",
                    "agent_did": "did:nostr:deadbeef",
                }),
                reasoning: "Agent proposes adding 3 new properties based on source analysis"
                    .to_string(),
            })
            .await
            .expect("mailbox")
            .expect("sign");
        event.verify().expect("signature");
        assert_eq!(event.kind.as_u16(), 31402);
        // Must carry `d` tag with case_id.
        let has_d = event
            .tags
            .iter()
            .any(|t| t.kind() == TagKind::Custom("d".into()));
        assert!(has_d);
        // Must carry category tag.
        let has_category = event
            .tags
            .iter()
            .any(|t| t.kind() == TagKind::Custom("category".into()));
        assert!(has_category);
        // Must carry priority tag.
        let has_priority = event
            .tags
            .iter()
            .any(|t| t.kind() == TagKind::Custom("priority".into()));
        assert!(has_priority);
        // Content must contain the fields and reasoning.
        let parsed: serde_json::Value = serde_json::from_str(&event.content).expect("valid JSON");
        assert!(parsed["fields"]["entity_urn"].is_string());
        assert_eq!(
            parsed["reasoning"],
            "Agent proposes adding 3 new properties based on source analysis"
        );
    }
}
