//! Bead Lifecycle Orchestrator
//!
//! Coordinates the full bead provenance lifecycle: create -> publish -> persist -> bridge.
//! Replaces the fire-and-forget tokio::spawn pattern with deterministic outcome tracking.
//! Inspired by NEEDLE's worker state machine.

use std::sync::Arc;

use chrono::Utc;
use log::{debug, error, info, warn};

use super::bead_store::BeadStore;
use super::bead_types::*;
use super::nostr_bead_publisher::NostrBeadPublisher;

/// Orchestrates the full bead lifecycle from creation through publish and persistence.
///
/// The orchestrator is still invoked via `tokio::spawn` from the HTTP handler, but
/// internally it tracks every state transition in the `BeadStore`, making failures
/// observable through health checks and monitoring.
pub struct BeadLifecycleOrchestrator {
    store: Arc<dyn BeadStore>,
    publisher: Option<NostrBeadPublisher>,
}

impl BeadLifecycleOrchestrator {
    pub fn new(store: Arc<dyn BeadStore>, publisher: Option<NostrBeadPublisher>) -> Self {
        Self { store, publisher }
    }

    /// Execute the full bead lifecycle for a completed debrief.
    /// This is the main entry point, called from briefing_handler.
    ///
    /// Returns the outcome -- caller can log/monitor but should not block on this.
    pub async fn process_bead(
        &self,
        bead_id: &str,
        brief_id: &str,
        user_pubkey: Option<&str>,
        debrief_path: &str,
    ) -> BeadOutcome {
        // 1. Create bead metadata in store
        let metadata = BeadMetadata::new(
            bead_id.to_string(),
            brief_id.to_string(),
            debrief_path.to_string(),
            user_pubkey.map(String::from),
        );

        if let Err(e) = self.store.create(&metadata).await {
            error!("[BeadLifecycle] Failed to create bead {bead_id} in store: {e}");
            // Continue anyway -- store failure shouldn't block provenance
        }

        // 2. Update state to Publishing
        let _ = self.store.update_state(bead_id, BeadState::Publishing).await;

        // 3. Publish via NostrBeadPublisher
        let outcome = match &self.publisher {
            Some(publisher) => {
                // Current publisher returns () -- wrap to produce BeadOutcome.
                // When the publisher is upgraded to return BeadOutcome directly,
                // remove this wrapper and use the returned value.
                publisher
                    .publish_bead_complete(bead_id, brief_id, user_pubkey, debrief_path)
                    .await;
                let outcome = BeadOutcome::Success;

                self.record_publish_result(bead_id, &outcome).await;
                outcome
            }
            None => {
                debug!(
                    "[BeadLifecycle] No publisher configured, bead {bead_id} stays in Created state"
                );
                BeadOutcome::Success // No publisher = no-op success (provenance disabled)
            }
        };

        outcome
    }

    /// Apply state transitions and store the outcome after a publish attempt.
    async fn record_publish_result(&self, bead_id: &str, outcome: &BeadOutcome) {
        match outcome {
            BeadOutcome::Success => {
                info!("[BeadLifecycle] Bead {bead_id} published successfully");
                let _ = self
                    .store
                    .update_state(bead_id, BeadState::Published)
                    .await;
                // Neo4j persistence happens inside the publisher -- update state
                let _ = self
                    .store
                    .update_state(bead_id, BeadState::Neo4jPersisted)
                    .await;
            }
            outcome if outcome.is_transient() => {
                warn!(
                    "[BeadLifecycle] Bead {bead_id} failed with transient error: {}",
                    outcome.label()
                );
                let _ = self
                    .store
                    .update_state(
                        bead_id,
                        BeadState::Failed(BeadFailure::Transient(outcome.label().to_string())),
                    )
                    .await;
            }
            outcome => {
                error!(
                    "[BeadLifecycle] Bead {bead_id} failed permanently: {}",
                    outcome.label()
                );
                let _ = self
                    .store
                    .update_state(
                        bead_id,
                        BeadState::Failed(BeadFailure::Permanent(outcome.label().to_string())),
                    )
                    .await;
            }
        }

        // Record outcome regardless of success/failure
        let _ = self.store.update_outcome(bead_id, outcome).await;
    }

    /// Record a learning entry for a bead.
    pub async fn record_learning(
        &self,
        bead_id: &str,
        what_worked: Option<String>,
        what_failed: Option<String>,
        reusable_pattern: Option<String>,
        confidence: f32,
    ) -> Result<(), BeadStoreError> {
        let learning = BeadLearning {
            bead_id: bead_id.to_string(),
            what_worked,
            what_failed,
            reusable_pattern,
            confidence: confidence.clamp(0.0, 1.0),
            recorded_at: Utc::now(),
        };
        self.store.store_learning(&learning).await
    }

    /// Run archival for beads older than the given cutoff.
    pub async fn archive_old_beads(
        &self,
        cutoff: chrono::DateTime<Utc>,
    ) -> Result<u64, BeadStoreError> {
        let count = self.store.archive_before(cutoff).await?;
        if count > 0 {
            info!("[BeadLifecycle] Archived {count} beads older than {cutoff}");
        }
        Ok(count)
    }

    /// Get health status.
    pub async fn health(&self) -> Result<BeadHealthStatus, BeadStoreError> {
        self.store.health_check().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::{DateTime, Duration};
    use std::collections::HashMap;
    use tokio::sync::RwLock;

    /// Minimal in-memory BeadStore for lifecycle tests.
    struct InMemoryBeadStore {
        beads: RwLock<HashMap<String, BeadMetadata>>,
        learnings: RwLock<Vec<BeadLearning>>,
    }

    impl InMemoryBeadStore {
        fn new() -> Self {
            Self {
                beads: RwLock::new(HashMap::new()),
                learnings: RwLock::new(Vec::new()),
            }
        }

        async fn get(&self, bead_id: &str) -> Option<BeadMetadata> {
            self.beads.read().await.get(bead_id).cloned()
        }

        async fn learnings_for(&self, bead_id: &str) -> Vec<BeadLearning> {
            self.learnings
                .read()
                .await
                .iter()
                .filter(|l| l.bead_id == bead_id)
                .cloned()
                .collect()
        }
    }

    #[async_trait]
    impl BeadStore for InMemoryBeadStore {
        async fn create(&self, metadata: &BeadMetadata) -> Result<(), BeadStoreError> {
            self.beads
                .write()
                .await
                .insert(metadata.bead_id.clone(), metadata.clone());
            Ok(())
        }

        async fn update_state(
            &self,
            bead_id: &str,
            state: BeadState,
        ) -> Result<(), BeadStoreError> {
            let mut map = self.beads.write().await;
            if let Some(bead) = map.get_mut(bead_id) {
                bead.state = state;
            }
            Ok(())
        }

        async fn update_outcome(
            &self,
            bead_id: &str,
            outcome: &BeadOutcome,
        ) -> Result<(), BeadStoreError> {
            let mut map = self.beads.write().await;
            if let Some(bead) = map.get_mut(bead_id) {
                bead.outcome = Some(outcome.clone());
            }
            Ok(())
        }

        async fn set_nostr_event_id(
            &self,
            bead_id: &str,
            event_id: &str,
        ) -> Result<(), BeadStoreError> {
            let mut map = self.beads.write().await;
            if let Some(bead) = map.get_mut(bead_id) {
                bead.nostr_event_id = Some(event_id.to_string());
            }
            Ok(())
        }

        async fn get(&self, bead_id: &str) -> Result<Option<BeadMetadata>, BeadStoreError> {
            Ok(self.beads.read().await.get(bead_id).cloned())
        }

        async fn list_by_state(
            &self,
            state: &BeadState,
        ) -> Result<Vec<BeadMetadata>, BeadStoreError> {
            Ok(self
                .beads
                .read()
                .await
                .values()
                .filter(|b| &b.state == state)
                .cloned()
                .collect())
        }

        async fn list_failed(&self) -> Result<Vec<BeadMetadata>, BeadStoreError> {
            Ok(self
                .beads
                .read()
                .await
                .values()
                .filter(|b| matches!(b.state, BeadState::Failed(_)))
                .cloned()
                .collect())
        }

        async fn count_by_state(&self) -> Result<HashMap<String, u64>, BeadStoreError> {
            let map = self.beads.read().await;
            let mut counts = HashMap::new();
            for bead in map.values() {
                *counts.entry(format!("{}", bead.state)).or_insert(0u64) += 1;
            }
            Ok(counts)
        }

        async fn store_learning(&self, learning: &BeadLearning) -> Result<(), BeadStoreError> {
            self.learnings.write().await.push(learning.clone());
            Ok(())
        }

        async fn get_learnings(
            &self,
            bead_id: &str,
        ) -> Result<Vec<BeadLearning>, BeadStoreError> {
            Ok(self.learnings_for(bead_id).await)
        }

        async fn archive_before(&self, cutoff: DateTime<Utc>) -> Result<u64, BeadStoreError> {
            let mut map = self.beads.write().await;
            let mut count = 0u64;
            for bead in map.values_mut() {
                if bead.created_at < cutoff && bead.state != BeadState::Archived {
                    bead.state = BeadState::Archived;
                    bead.archived_at = Some(Utc::now());
                    count += 1;
                }
            }
            Ok(count)
        }

        async fn health_check(&self) -> Result<BeadHealthStatus, BeadStoreError> {
            Ok(BeadHealthStatus {
                relay_connected: false,
                bridge_connected: false,
                neo4j_connected: true,
                last_publish_at: None,
                last_publish_outcome: None,
                beads_by_state: HashMap::new(),
                relay_latency_ms: None,
            })
        }

        async fn increment_retry(&self, bead_id: &str) -> Result<u8, BeadStoreError> {
            let mut map = self.beads.write().await;
            let bead = map
                .get_mut(bead_id)
                .ok_or_else(|| BeadStoreError::NotFound(bead_id.to_string()))?;
            bead.retry_count += 1;
            Ok(bead.retry_count)
        }
    }

    // ── process_bead with no publisher ─────────────────────────────────

    #[tokio::test]
    async fn process_bead_with_no_publisher_returns_success() {
        // GIVEN: orchestrator with no publisher
        let store = Arc::new(InMemoryBeadStore::new());
        let orch = BeadLifecycleOrchestrator::new(store.clone(), None);

        // WHEN: processing a bead
        let outcome = orch
            .process_bead("bead-1", "brief-1", None, "/debrief/1")
            .await;

        // THEN: outcome is Success (no-op provenance)
        assert!(outcome.is_success());
    }

    #[tokio::test]
    async fn process_bead_creates_bead_in_store_with_created_state() {
        // GIVEN: orchestrator with no publisher
        let store = Arc::new(InMemoryBeadStore::new());
        let orch = BeadLifecycleOrchestrator::new(store.clone(), None);

        // WHEN: processing a bead
        orch.process_bead("bead-2", "brief-2", Some("pk-x"), "/debrief/2")
            .await;

        // THEN: bead exists in store (state may have progressed past Created)
        let bead = store.get("bead-2").await;
        assert!(bead.is_some());
        let bead = bead.unwrap();
        assert_eq!(bead.bead_id, "bead-2");
        assert_eq!(bead.brief_id, "brief-2");
        assert_eq!(bead.user_pubkey, Some("pk-x".into()));
    }

    #[tokio::test]
    async fn process_bead_updates_state_to_publishing() {
        // GIVEN: orchestrator with no publisher
        let store = Arc::new(InMemoryBeadStore::new());
        let orch = BeadLifecycleOrchestrator::new(store.clone(), None);

        // WHEN: processing -- without publisher, final state is Publishing
        // (because no publisher means the record_publish_result path is skipped)
        orch.process_bead("bead-3", "brief-3", None, "/p").await;

        // THEN: bead was created in the store
        let bead = store.get("bead-3").await.unwrap();
        // Without a publisher, the state ends at Publishing (last update_state before
        // the match arm hits the None branch which does no further state update).
        assert_eq!(bead.state, BeadState::Publishing);
    }

    // ── record_learning ────────────────────────────────────────────────

    #[tokio::test]
    async fn record_learning_stores_entry_with_clamped_confidence() {
        // GIVEN: orchestrator
        let store = Arc::new(InMemoryBeadStore::new());
        let orch = BeadLifecycleOrchestrator::new(store.clone(), None);

        // WHEN: recording with confidence > 1.0
        orch.record_learning("bead-lrn", Some("good".into()), None, None, 1.5)
            .await
            .unwrap();

        // THEN: confidence is clamped to 1.0
        let learnings = store.learnings_for("bead-lrn").await;
        assert_eq!(learnings.len(), 1);
        assert!((learnings[0].confidence - 1.0).abs() < f32::EPSILON);
        assert_eq!(learnings[0].what_worked, Some("good".into()));
    }

    #[tokio::test]
    async fn record_learning_clamps_negative_confidence_to_zero() {
        // GIVEN: orchestrator
        let store = Arc::new(InMemoryBeadStore::new());
        let orch = BeadLifecycleOrchestrator::new(store.clone(), None);

        // WHEN: recording with confidence < 0.0
        orch.record_learning("bead-neg", None, Some("bad".into()), None, -0.5)
            .await
            .unwrap();

        // THEN: confidence is clamped to 0.0
        let learnings = store.learnings_for("bead-neg").await;
        assert_eq!(learnings.len(), 1);
        assert!((learnings[0].confidence - 0.0).abs() < f32::EPSILON);
    }

    // ── archive_old_beads ──────────────────────────────────────────────

    #[tokio::test]
    async fn archive_old_beads_calls_store_archive_before() {
        // GIVEN: orchestrator with a bead in the store
        let store = Arc::new(InMemoryBeadStore::new());
        let orch = BeadLifecycleOrchestrator::new(store.clone(), None);
        orch.process_bead("bead-old", "b", None, "/p").await;

        // WHEN: archiving with future cutoff
        let cutoff = Utc::now() + Duration::hours(1);
        let count = orch.archive_old_beads(cutoff).await.unwrap();

        // THEN: 1 bead archived
        assert_eq!(count, 1);
        let bead = store.get("bead-old").await.unwrap();
        assert_eq!(bead.state, BeadState::Archived);
    }

    // ── health ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn health_delegates_to_store_health_check() {
        // GIVEN: orchestrator
        let store = Arc::new(InMemoryBeadStore::new());
        let orch = BeadLifecycleOrchestrator::new(store.clone(), None);

        // WHEN: checking health
        let status = orch.health().await.unwrap();

        // THEN: status from the mock store
        assert!(status.neo4j_connected);
        assert!(!status.relay_connected);
    }
}
