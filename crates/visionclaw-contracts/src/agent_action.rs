//! `AgentActionEnvelope` — outbound message from VisionClaw to agentbox/forum
//! dispatched when the user interacts with an agent node in the 3D scene.
//!
//! Canonical specification: ADR-10 §D3 and `_resolutions/T4-T6-T7-api-contracts.md`
//! §T7. Supersedes ADR-07 §D8's `RequestAgentControlSurface` intent.
//!
//! ## Transport
//!
//! Transport selection happens **once per session at handshake time**:
//!
//! - same-origin: `BroadcastChannel(AGENT_ACTION_CHANNEL)`
//! - cross-origin: `window.open(deep-link)`
//! - embedded (`window.parent !== window`): `window.parent.postMessage`
//!
//! ## Receiver contract
//!
//! Receivers **MUST**:
//!
//! 1. Verify `type === "visionclaw:agent-action"`
//! 2. Verify `schema_version === 1` (refuse with a structured log otherwise)
//! 3. For postMessage delivery: verify `event.origin` against the
//!    [`AgentActionTargetOrigin`] allowlist
//! 4. Treat any unknown `kind` as a no-op (forward-compatible)
//!
//! Receivers **MUST NOT**:
//!
//! - Re-broadcast the envelope (one-way contract)
//! - Trust `issued_by_pubkey` as auth (informational only; authentication is
//!   established via the ADR-10 §D4 bridge JWT)

use serde::{Deserialize, Serialize};

#[cfg(feature = "typescript-export")]
use ts_rs::TS;

pub use crate::version::SCHEMA_VERSION;

/// BroadcastChannel name for same-origin agent-action dispatch.
///
/// Both VisionClaw and the receiver (agentbox / forum) import this literal so
/// the channel name is never spelled out twice. Convention is enforced by
/// CI: every `BroadcastChannel(` literal in `client/src/` matches
/// `visionclaw:[a-z-]+` (ADR-10 §"BroadcastChannel naming convention").
pub const AGENT_ACTION_CHANNEL: &str = "visionclaw:agent-actions";

/// Deep-link template used when the receiver is cross-origin and
/// BroadcastChannel is unavailable.
///
/// Receivers MUST treat every field in the resulting URL as untrusted user
/// input. `bridge_id` is the only field linking the click to an authenticated
/// session; if absent or invalid, the receiver SHOULD challenge for re-auth
/// before honouring `kind`.
pub const AGENT_ACTION_DEEP_LINK_TEMPLATE: &str =
    "/agents/{agent_id}?source=visionclaw&kind={kind}\
     &issued_at={issued_at_ms}&issued_by={issued_by_pubkey}\
     &message_id={message_id}&node_class={node_class}\
     &bridge_id={bridge_id?}&swarm_id={swarm_id?}";

/// Discriminator literal for the envelope `type` field.
///
/// Always emitted exactly as this string. Receivers compare against this
/// constant rather than free-form string literals.
pub const AGENT_ACTION_TYPE: &str = "visionclaw:agent-action";

// ---------------------------------------------------------------------------
// Envelope
// ---------------------------------------------------------------------------

/// Top-level envelope dispatched on every agent-node interaction.
///
/// Serde serialises this as a discriminated union with `"type":
/// "visionclaw:agent-action"` at the top level. The enum has a single variant
/// today; future incompatible shapes would be introduced by bumping
/// `schema_version` rather than by adding a sibling variant.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(
    feature = "typescript-export",
    derive(TS),
    ts(export, export_to = "AgentActionEnvelope.ts")
)]
#[serde(tag = "type")]
pub enum AgentActionEnvelope {
    /// The one and only envelope variant at `schema_version = 1`.
    #[serde(rename = "visionclaw:agent-action")]
    V1(AgentAction),
}

/// Payload for an `agent-action` envelope at schema version 1.
///
/// Fields match the canonical TypeScript schema in
/// `_resolutions/T4-T6-T7-api-contracts.md` §T7 byte-for-byte (serde rename
/// rules below preserve the wire shape).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(
    feature = "typescript-export",
    derive(TS),
    ts(export, export_to = "AgentAction.ts")
)]
pub struct AgentAction {
    /// Schema version. Bumped per ADR-10 §D8 when payload semantics change.
    pub schema_version: u32,

    /// UUID v4, generated per click. Receivers MAY use this for dedup.
    pub message_id: String,

    /// Unix milliseconds at click time.
    pub issued_at_ms: i64,

    /// Pubkey of the clicking user, npub format. Informational only —
    /// the receiver verifies authorisation via its own bridge session.
    pub issued_by_pubkey: String,

    /// Action kind. Forward-compatible: receivers ignore unknown variants
    /// (serde tolerates this only on the consumer side via
    /// `#[serde(other)]` adapters; producers emit only the variants below).
    pub kind: ActionKind,

    /// Agent identity. Required for every kind.
    pub agent_id: String,

    /// Swarm identity, when known. Not all agents belong to swarms.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub swarm_id: Option<String>,

    /// Class flag bits from the V3 node id (ADR-07 §D3 + ADR-08 §D6).
    /// Lets the receiver short-circuit if the click target is not actually
    /// an agent.
    pub node_class: NodeClass,

    /// Click modifiers, for receivers that distinguish primary / secondary
    /// actions. All inner flags optional; absent struct defaults to primary
    /// semantics.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modifiers: Option<ModifierKeys>,

    /// Cursor in scene world-space at click time. Used by the receiver to
    /// position popovers when rendering inside the same browser tab.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor_world_position: Option<WorldPosition>,

    /// Bridge session id from the ADR-10 §D4 auth flow. Receivers MAY use
    /// this to correlate the click with the bridge session that issued the
    /// `Authorization` for the originating VisionClaw tab.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bridge_id: Option<String>,
}

// ---------------------------------------------------------------------------
// Field types
// ---------------------------------------------------------------------------

/// Discriminated action kind. Forward-compatible — receivers MUST treat
/// any unknown value as a no-op (ADR-10 §D3).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "typescript-export", derive(TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    /// Primary action — open the agent's control panel.
    OpenPanel,
    /// Open the agent's log view.
    ShowLogs,
    /// Open the parent swarm overview.
    ShowSwarm,
    /// Open the agent's parent-chain trace.
    ShowLineage,
}

/// Class flag bits derived from the V3 node id, projected onto the small set
/// of receiver-actionable categories. Receivers ignore the full ontology
/// taxonomy; only agent vs. non-agent matters at click time.
///
/// Wire encoding is `snake_case`. Underlying bit layout owned by ADR-08 §D6.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "typescript-export", derive(TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum NodeClass {
    /// `0x40000000` — knowledge graph page node.
    KnowledgePage,
    /// `0x20000000` — OWL2 class node.
    OntologyClass,
    /// `0x10000000` — OWL2 property node.
    OntologyProperty,
    /// `0x04000000` — Axiom node.
    Axiom,
    /// `0x80000000` — Agent capsule.
    Agent,
    /// `0x08000000` — `[[wikilink]]` target with no source file.
    LinkedPage,
}

/// Modifier-key state at click time. All fields default to `false` (primary
/// click semantics with no modifiers).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "typescript-export", derive(TS), ts(export))]
pub struct ModifierKeys {
    #[serde(default)]
    pub ctrl: bool,
    #[serde(default)]
    pub shift: bool,
    #[serde(default)]
    pub alt: bool,
    #[serde(default)]
    pub meta: bool,
    /// 0 = primary, 1 = middle, 2 = secondary. Absent => primary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub button: Option<u8>,
}

/// Cursor world-space position. `f64` on the wire to match the canonical
/// TypeScript `number` semantics (IEEE-754 double-precision). The renderer
/// downcasts to `f32` internally; the contract leaves precision to the
/// receiver.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "typescript-export", derive(TS), ts(export))]
pub struct WorldPosition {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

/// Allowed `postMessage` target origins.
///
/// The bridge handshake (ADR-10 §D4) establishes this list at session start;
/// the receiver enforces it. Same-origin BroadcastChannel does not require
/// origin verification (the browser guarantees same-origin); deep-link
/// transport carries no origin and falls back to bridge-id verification.
///
/// We model this as a string-typed enum on the Rust side so consumer code
/// matches exhaustively; the TS export keeps the literal union shape
/// receivers can `===` against.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "typescript-export", derive(TS), ts(export))]
#[serde(untagged)]
pub enum AgentActionTargetOrigin {
    /// Free-form origin string. The allowlist itself is configured per
    /// deployment; the type exists to make the allowlist a typed value
    /// rather than a stringly-typed tuple.
    Origin(String),
}

// ---------------------------------------------------------------------------
// Constructors
// ---------------------------------------------------------------------------

impl AgentAction {
    /// Build a v1 action with sensible defaults. Caller is responsible for
    /// supplying `message_id` (a fresh UUIDv4 per click), `issued_at_ms`,
    /// `issued_by_pubkey`, `agent_id`, `node_class`, and `kind`.
    pub fn new(
        message_id: String,
        issued_at_ms: i64,
        issued_by_pubkey: String,
        agent_id: String,
        node_class: NodeClass,
        kind: ActionKind,
    ) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            message_id,
            issued_at_ms,
            issued_by_pubkey,
            kind,
            agent_id,
            swarm_id: None,
            node_class,
            modifiers: None,
            cursor_world_position: None,
            bridge_id: None,
        }
    }

    /// Wrap this action in the discriminated envelope. The envelope is what
    /// is serialised to the wire.
    pub fn into_envelope(self) -> AgentActionEnvelope {
        AgentActionEnvelope::V1(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_serialises_with_type_discriminator() {
        let action = AgentAction::new(
            "00000000-0000-4000-8000-000000000000".into(),
            1_715_856_000_000,
            "npub1example".into(),
            "agent-abc123".into(),
            NodeClass::Agent,
            ActionKind::OpenPanel,
        );
        let envelope = action.into_envelope();
        let json = serde_json::to_value(&envelope).unwrap();
        assert_eq!(json["type"], "visionclaw:agent-action");
        assert_eq!(json["schema_version"], 1);
        assert_eq!(json["kind"], "open_panel");
        assert_eq!(json["node_class"], "agent");
    }

    #[test]
    fn envelope_round_trips() {
        let action = AgentAction::new(
            "00000000-0000-4000-8000-000000000000".into(),
            1_715_856_000_000,
            "npub1example".into(),
            "agent-abc123".into(),
            NodeClass::Agent,
            ActionKind::ShowSwarm,
        )
        .into_envelope();
        let s = serde_json::to_string(&action).unwrap();
        let back: AgentActionEnvelope = serde_json::from_str(&s).unwrap();
        assert_eq!(action, back);
    }

    #[test]
    fn optional_fields_skip_when_none() {
        let action = AgentAction::new(
            "00000000-0000-4000-8000-000000000000".into(),
            1_715_856_000_000,
            "npub1example".into(),
            "agent-abc123".into(),
            NodeClass::Agent,
            ActionKind::OpenPanel,
        )
        .into_envelope();
        let s = serde_json::to_string(&action).unwrap();
        assert!(!s.contains("swarm_id"));
        assert!(!s.contains("modifiers"));
        assert!(!s.contains("cursor_world_position"));
        assert!(!s.contains("bridge_id"));
    }
}
