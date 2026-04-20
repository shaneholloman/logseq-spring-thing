//! Inbox service — writes `InboxItem` JSON-LD documents under
//! `/inbox/{agent-ns}/{item-id}.jsonld` and runs the retention sweeper.
//!
//! Implements design 03 §6:
//!
//! - 4 triage states (spec lists 5 — `created` is the initial state and
//!   `triaged` is the post-open umbrella; the operational state machine
//!   uses: `created → triaged → {accepted, dismissed, escalated, snoozed}`)
//! - Provenance manifest bound to the NIP-26 cap (spec §6.4)
//! - Retention policy: 500 items or 30 days per contributor inbox
//!   (overrides the 14-day in §6 in favour of PRD §14 R13 hard cap;
//!   items beyond retention are moved to `/inbox/.dlq/`)
//!
//! Writes are staged via the [`InboxWriter`] trait so this module is
//! testable without a live Pod.

use async_trait::async_trait;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

/// Retention window — 30 days (PRD §14 R13).
pub const RETENTION_DAYS: i64 = 30;

/// Max items per inbox before the sweeper moves the oldest to `/inbox/.dlq/`.
pub const RETENTION_MAX_ITEMS: usize = 500;

/// Default TTL from `created_at` before auto-archive — matches spec §6.1.
pub const DEFAULT_TTL_DAYS: i64 = 14;

/// Triage state machine.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum InboxStatus {
    Created,
    Triaged,
    Accepted,
    Dismissed,
    Escalated,
    Snoozed,
}

impl InboxStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, InboxStatus::Accepted | InboxStatus::Dismissed | InboxStatus::Escalated)
    }
}

/// Who wrote this item — routine, agent, or system.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InboxSource {
    Routine {
        ref_: String,
        agent_webid: String,
    },
    Agent {
        agent_webid: String,
    },
    System {
        reason: String,
    },
}

/// One step in the provenance manifest (spec §6.2).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProvenanceStep {
    pub step: String,
    pub at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signed_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evaluation_id: Option<String>,
}

/// InboxItem schema (design 03 §6.2).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InboxItem {
    #[serde(rename = "@type")]
    pub item_type: String,
    pub item_id: String,
    pub created_at: DateTime<Utc>,
    pub source: InboxSource,
    pub topic: String,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_ref: Option<String>,
    #[serde(default)]
    pub suggested_actions: Vec<SuggestedAction>,
    pub provenance_chain: Vec<ProvenanceStep>,
    pub ttl: String,
    pub priority: String,
    pub status: InboxStatus,
    /// Set by the sweeper; read-only to callers.
    #[serde(default)]
    pub archived: bool,
    /// Set by `mark_snoozed`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wake_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SuggestedAction {
    pub action_id: String,
    pub label: String,
    /// MUST be a Studio-registered deep-link scheme; HTTP(S) rejected
    /// at render time (spec §6.2).
    pub target: String,
}

impl InboxItem {
    /// Build a new `created` item with a generated UUID and the given
    /// cap id glued into the provenance manifest (spec §6.4).
    pub fn new_from_routine(
        topic: impl Into<String>,
        summary: impl Into<String>,
        routine_ref: impl Into<String>,
        agent_webid: impl Into<String>,
        cap_id: impl Into<String>,
        content_ref: Option<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            item_type: "InboxItem".to_string(),
            item_id: format!("item-{}", Uuid::new_v4()),
            created_at: now,
            source: InboxSource::Routine {
                ref_: routine_ref.into(),
                agent_webid: agent_webid.into(),
            },
            topic: topic.into(),
            summary: summary.into(),
            content_ref,
            suggested_actions: Vec::new(),
            provenance_chain: vec![ProvenanceStep {
                step: "routine-scheduled".to_string(),
                at: now,
                signed_by: Some(cap_id.into()),
                tier: None,
                evaluation_id: None,
            }],
            ttl: format!("P{}D", DEFAULT_TTL_DAYS),
            priority: "normal".to_string(),
            status: InboxStatus::Created,
            archived: false,
            wake_at: None,
        }
    }

    /// System-authored notification (e.g. cap-renewal nudge). No cap id
    /// is required — the server is the author.
    pub fn new_system_notice(
        topic: impl Into<String>,
        summary: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            item_type: "InboxItem".to_string(),
            item_id: format!("sys-{}", Uuid::new_v4()),
            created_at: now,
            source: InboxSource::System {
                reason: reason.into(),
            },
            topic: topic.into(),
            summary: summary.into(),
            content_ref: None,
            suggested_actions: Vec::new(),
            provenance_chain: vec![ProvenanceStep {
                step: "system-notice".to_string(),
                at: now,
                signed_by: None,
                tier: None,
                evaluation_id: None,
            }],
            ttl: format!("P{}D", DEFAULT_TTL_DAYS),
            priority: "normal".to_string(),
            status: InboxStatus::Created,
            archived: false,
            wake_at: None,
        }
    }
}

/// Errors from inbox operations.
#[derive(Debug, thiserror::Error)]
pub enum InboxError {
    #[error("invalid status transition from {from:?} to {to:?}")]
    InvalidTransition { from: InboxStatus, to: InboxStatus },
    #[error("item not found: {0}")]
    NotFound(String),
    #[error("writer error: {0}")]
    Writer(String),
    #[error("content_ref must resolve under /private/** or /shared/**; got {0}")]
    InvalidContentRef(String),
    #[error("suggested action target must be a studio: deep-link; got {0}")]
    InvalidActionTarget(String),
    #[error("provenance chain missing routine-scheduled step with signed_by")]
    MissingProvenance,
}

/// Namespaced inbox path: `/inbox/{agent_ns}/`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct InboxPath {
    pub owner_webid: String,
    pub agent_ns: String,
}

impl InboxPath {
    pub fn new(owner_webid: impl Into<String>, agent_ns: impl Into<String>) -> Self {
        Self {
            owner_webid: owner_webid.into(),
            agent_ns: agent_ns.into(),
        }
    }

    pub fn container(&self) -> String {
        format!("/inbox/{}/", self.agent_ns)
    }

    pub fn item_path(&self, item_id: &str) -> String {
        format!("/inbox/{}/{}.jsonld", self.agent_ns, item_id)
    }

    pub fn dlq_path(&self, item_id: &str) -> String {
        format!("/inbox/.dlq/{}/{}.jsonld", self.agent_ns, item_id)
    }
}

/// Backend port — the real impl calls `PodClient`; tests use
/// [`InMemoryInboxWriter`].
#[async_trait]
pub trait InboxWriter: Send + Sync {
    async fn append(&self, path: &InboxPath, item: InboxItem) -> Result<(), InboxError>;
    async fn list(&self, path: &InboxPath) -> Result<Vec<InboxItem>, InboxError>;
    async fn update_status(
        &self,
        path: &InboxPath,
        item_id: &str,
        status: InboxStatus,
        wake_at: Option<DateTime<Utc>>,
    ) -> Result<(), InboxError>;
    async fn move_to_dlq(&self, path: &InboxPath, item_id: &str) -> Result<(), InboxError>;
}

/// In-memory writer used in tests and the service-under-test.
#[derive(Default, Clone)]
pub struct InMemoryInboxWriter {
    items: Arc<RwLock<HashMap<(String, String), Vec<InboxItem>>>>,
    dlq: Arc<RwLock<HashMap<(String, String), Vec<InboxItem>>>>,
}

impl InMemoryInboxWriter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn count(&self, path: &InboxPath) -> usize {
        self.items
            .read()
            .ok()
            .and_then(|m| m.get(&(path.owner_webid.clone(), path.agent_ns.clone())).map(|v| v.len()))
            .unwrap_or(0)
    }

    pub fn dlq_count(&self, path: &InboxPath) -> usize {
        self.dlq
            .read()
            .ok()
            .and_then(|m| m.get(&(path.owner_webid.clone(), path.agent_ns.clone())).map(|v| v.len()))
            .unwrap_or(0)
    }
}

#[async_trait]
impl InboxWriter for InMemoryInboxWriter {
    async fn append(&self, path: &InboxPath, item: InboxItem) -> Result<(), InboxError> {
        let mut g = self
            .items
            .write()
            .map_err(|e| InboxError::Writer(e.to_string()))?;
        g.entry((path.owner_webid.clone(), path.agent_ns.clone()))
            .or_default()
            .push(item);
        Ok(())
    }

    async fn list(&self, path: &InboxPath) -> Result<Vec<InboxItem>, InboxError> {
        let g = self
            .items
            .read()
            .map_err(|e| InboxError::Writer(e.to_string()))?;
        Ok(g.get(&(path.owner_webid.clone(), path.agent_ns.clone()))
            .cloned()
            .unwrap_or_default())
    }

    async fn update_status(
        &self,
        path: &InboxPath,
        item_id: &str,
        status: InboxStatus,
        wake_at: Option<DateTime<Utc>>,
    ) -> Result<(), InboxError> {
        let mut g = self
            .items
            .write()
            .map_err(|e| InboxError::Writer(e.to_string()))?;
        let list = g
            .get_mut(&(path.owner_webid.clone(), path.agent_ns.clone()))
            .ok_or_else(|| InboxError::NotFound(item_id.to_string()))?;
        let it = list
            .iter_mut()
            .find(|i| i.item_id == item_id)
            .ok_or_else(|| InboxError::NotFound(item_id.to_string()))?;
        it.status = status;
        it.wake_at = wake_at;
        Ok(())
    }

    async fn move_to_dlq(&self, path: &InboxPath, item_id: &str) -> Result<(), InboxError> {
        let mut live = self
            .items
            .write()
            .map_err(|e| InboxError::Writer(e.to_string()))?;
        let list = live
            .get_mut(&(path.owner_webid.clone(), path.agent_ns.clone()))
            .ok_or_else(|| InboxError::NotFound(item_id.to_string()))?;
        let pos = list
            .iter()
            .position(|i| i.item_id == item_id)
            .ok_or_else(|| InboxError::NotFound(item_id.to_string()))?;
        let mut item = list.remove(pos);
        item.archived = true;
        drop(live);
        let mut dlq = self
            .dlq
            .write()
            .map_err(|e| InboxError::Writer(e.to_string()))?;
        dlq.entry((path.owner_webid.clone(), path.agent_ns.clone()))
            .or_default()
            .push(item);
        Ok(())
    }
}

/// Service over a concrete writer.
pub struct InboxService<W: InboxWriter> {
    writer: W,
    /// Max age before sweep (override for tests).
    pub retention_days: i64,
    /// Max items per inbox before oldest are DLQ'd.
    pub retention_max_items: usize,
}

impl<W: InboxWriter> InboxService<W> {
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            retention_days: RETENTION_DAYS,
            retention_max_items: RETENTION_MAX_ITEMS,
        }
    }

    pub fn with_retention(mut self, days: i64, max_items: usize) -> Self {
        self.retention_days = days;
        self.retention_max_items = max_items;
        self
    }

    pub fn writer(&self) -> &W {
        &self.writer
    }

    /// Write a routine-authored item — validates content_ref + suggested_actions
    /// before calling the writer.
    pub async fn write_item(
        &self,
        path: &InboxPath,
        item: InboxItem,
    ) -> Result<String, InboxError> {
        validate_item(&item)?;
        let item_id = item.item_id.clone();
        self.writer.append(path, item).await?;
        Ok(item_id)
    }

    /// Triage actions (spec §6.3).
    pub async fn accept(&self, path: &InboxPath, item_id: &str) -> Result<(), InboxError> {
        self.transition(path, item_id, InboxStatus::Accepted, None).await
    }

    pub async fn dismiss(&self, path: &InboxPath, item_id: &str) -> Result<(), InboxError> {
        self.transition(path, item_id, InboxStatus::Dismissed, None).await
    }

    pub async fn escalate(&self, path: &InboxPath, item_id: &str) -> Result<(), InboxError> {
        self.transition(path, item_id, InboxStatus::Escalated, None).await
    }

    pub async fn snooze(
        &self,
        path: &InboxPath,
        item_id: &str,
        wake_at: DateTime<Utc>,
    ) -> Result<(), InboxError> {
        // §6.1 — snooze.wake_at ≤ created + 30 d. We enforce ≤ 30 d from now
        // as a proxy (the pod-read layer carries the exact created_at).
        let max = Utc::now() + ChronoDuration::days(30);
        if wake_at > max {
            return Err(InboxError::InvalidTransition {
                from: InboxStatus::Created,
                to: InboxStatus::Snoozed,
            });
        }
        self.transition(path, item_id, InboxStatus::Snoozed, Some(wake_at))
            .await
    }

    async fn transition(
        &self,
        path: &InboxPath,
        item_id: &str,
        to: InboxStatus,
        wake_at: Option<DateTime<Utc>>,
    ) -> Result<(), InboxError> {
        // Fetch current to reject terminal → anything transitions.
        let items = self.writer.list(path).await?;
        let cur = items
            .iter()
            .find(|i| i.item_id == item_id)
            .ok_or_else(|| InboxError::NotFound(item_id.to_string()))?;
        if cur.status.is_terminal() {
            return Err(InboxError::InvalidTransition {
                from: cur.status,
                to,
            });
        }
        self.writer.update_status(path, item_id, to, wake_at).await
    }

    /// Retention sweep — moves items older than `retention_days` OR any
    /// items beyond `retention_max_items` (oldest-first) to DLQ.
    ///
    /// Returns `(moved_count, kept_count)`.
    pub async fn sweep(
        &self,
        path: &InboxPath,
        now: DateTime<Utc>,
    ) -> Result<SweepReport, InboxError> {
        let mut items = self.writer.list(path).await?;
        // Oldest-first so we can trim head.
        items.sort_by_key(|i| i.created_at);
        let mut to_move: Vec<String> = Vec::new();
        let cutoff = now - ChronoDuration::days(self.retention_days);

        // Age-based
        for it in &items {
            if it.created_at < cutoff && !it.archived {
                to_move.push(it.item_id.clone());
            }
        }
        // Count-based — trim oldest remaining live items until count ≤ max.
        let live_after_age = items
            .iter()
            .filter(|it| !to_move.contains(&it.item_id))
            .count();
        if live_after_age > self.retention_max_items {
            let overflow = live_after_age - self.retention_max_items;
            for it in items.iter().filter(|it| !to_move.contains(&it.item_id)).take(overflow) {
                to_move.push(it.item_id.clone());
            }
        }

        let mut moved = 0usize;
        for id in &to_move {
            self.writer.move_to_dlq(path, id).await?;
            moved += 1;
        }

        let kept = items.len().saturating_sub(moved);
        Ok(SweepReport { moved, kept })
    }
}

/// Outcome of a retention sweep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SweepReport {
    pub moved: usize,
    pub kept: usize,
}

/// Validate per-item integrity rules (spec §6.2).
pub fn validate_item(item: &InboxItem) -> Result<(), InboxError> {
    if let Some(ref r) = item.content_ref {
        if !(r.starts_with("pod:/private/") || r.starts_with("pod:/shared/")) {
            return Err(InboxError::InvalidContentRef(r.clone()));
        }
    }
    for a in &item.suggested_actions {
        if !a.target.starts_with("studio:") {
            return Err(InboxError::InvalidActionTarget(a.target.clone()));
        }
    }
    // Routine-authored items require a signed_by on provenance[0].
    if let InboxSource::Routine { .. } = item.source {
        let has = item
            .provenance_chain
            .first()
            .map(|p| p.signed_by.is_some())
            .unwrap_or(false);
        if !has {
            return Err(InboxError::MissingProvenance);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_item(cap: &str) -> InboxItem {
        InboxItem::new_from_routine(
            "test",
            "summary",
            "urn:routine:rt-001",
            "https://alice.pods/agents/research-brief#id",
            cap,
            Some("pod:/private/workspaces/briefs/x.md".into()),
        )
    }

    #[tokio::test]
    async fn write_and_list() {
        let svc = InboxService::new(InMemoryInboxWriter::new());
        let path = InboxPath::new("https://alice.pods/profile/card#me", "research-brief");
        let id = svc.write_item(&path, mk_item("urn:nip26:cap:1")).await.unwrap();
        let items = svc.writer.list(&path).await.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].item_id, id);
        assert_eq!(items[0].status, InboxStatus::Created);
    }

    #[tokio::test]
    async fn reject_http_action_target() {
        let svc = InboxService::new(InMemoryInboxWriter::new());
        let path = InboxPath::new("https://alice.pods/profile/card#me", "research-brief");
        let mut item = mk_item("urn:nip26:cap:1");
        item.suggested_actions.push(SuggestedAction {
            action_id: "a1".into(),
            label: "phish".into(),
            target: "https://evil.example/".into(),
        });
        let err = svc.write_item(&path, item).await.unwrap_err();
        assert!(matches!(err, InboxError::InvalidActionTarget(_)));
    }

    #[tokio::test]
    async fn transition_created_to_accepted() {
        let svc = InboxService::new(InMemoryInboxWriter::new());
        let path = InboxPath::new("o", "ns");
        let id = svc.write_item(&path, mk_item("cap-1")).await.unwrap();
        svc.accept(&path, &id).await.unwrap();
        let items = svc.writer.list(&path).await.unwrap();
        assert_eq!(items[0].status, InboxStatus::Accepted);
    }

    #[tokio::test]
    async fn terminal_blocks_retransition() {
        let svc = InboxService::new(InMemoryInboxWriter::new());
        let path = InboxPath::new("o", "ns");
        let id = svc.write_item(&path, mk_item("cap-1")).await.unwrap();
        svc.dismiss(&path, &id).await.unwrap();
        let err = svc.accept(&path, &id).await.unwrap_err();
        assert!(matches!(err, InboxError::InvalidTransition { .. }));
    }
}
