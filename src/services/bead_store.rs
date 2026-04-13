//! Bead Store — trait + Neo4j implementation.
//!
//! Inspired by NEEDLE's BeadStore async trait pattern. The trait abstracts
//! storage so the lifecycle orchestrator and tests don't depend on Neo4j directly.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use log::{debug, warn};
use neo4rs::{Graph, Query};
use std::collections::HashMap;
use std::sync::Arc;

use super::bead_types::*;

/// Async storage trait for bead lifecycle persistence.
#[async_trait]
pub trait BeadStore: Send + Sync {
    /// Persist a new bead in Created state.
    async fn create(&self, metadata: &BeadMetadata) -> Result<(), BeadStoreError>;

    /// Transition bead to a new state, recording the appropriate timestamp.
    async fn update_state(&self, bead_id: &str, state: BeadState) -> Result<(), BeadStoreError>;

    /// Record a publish outcome against a bead.
    async fn update_outcome(
        &self,
        bead_id: &str,
        outcome: &BeadOutcome,
    ) -> Result<(), BeadStoreError>;

    /// Link a Nostr event ID to a bead after successful relay publish.
    async fn set_nostr_event_id(
        &self,
        bead_id: &str,
        event_id: &str,
    ) -> Result<(), BeadStoreError>;

    /// Fetch a single bead by ID.
    async fn get(&self, bead_id: &str) -> Result<Option<BeadMetadata>, BeadStoreError>;

    /// List all beads in a given state.
    async fn list_by_state(&self, state: &BeadState) -> Result<Vec<BeadMetadata>, BeadStoreError>;

    /// List all beads in any Failed state.
    async fn list_failed(&self) -> Result<Vec<BeadMetadata>, BeadStoreError>;

    /// Count beads grouped by state label.
    async fn count_by_state(&self) -> Result<HashMap<String, u64>, BeadStoreError>;

    /// Persist a learning entry linked to a bead.
    async fn store_learning(&self, learning: &BeadLearning) -> Result<(), BeadStoreError>;

    /// Retrieve all learnings for a bead.
    async fn get_learnings(&self, bead_id: &str) -> Result<Vec<BeadLearning>, BeadStoreError>;

    /// Archive beads created before the cutoff. Returns count archived.
    async fn archive_before(&self, cutoff: DateTime<Utc>) -> Result<u64, BeadStoreError>;

    /// Health check: connectivity + state counts.
    async fn health_check(&self) -> Result<BeadHealthStatus, BeadStoreError>;

    /// Increment retry count atomically. Returns the new count.
    async fn increment_retry(&self, bead_id: &str) -> Result<u8, BeadStoreError>;
}

// ---------------------------------------------------------------------------
// No-op store (default when no persistent backend is configured)
// ---------------------------------------------------------------------------

/// No-op bead store that discards all writes.
///
/// Used as the default when no persistent backend is configured. Allows the
/// orchestrator to run without a database while preserving the full lifecycle
/// state machine in logs.
pub struct NoopBeadStore;

#[async_trait]
impl BeadStore for NoopBeadStore {
    async fn create(&self, _metadata: &BeadMetadata) -> Result<(), BeadStoreError> {
        Ok(())
    }

    async fn update_state(&self, _bead_id: &str, _state: BeadState) -> Result<(), BeadStoreError> {
        Ok(())
    }

    async fn update_outcome(
        &self,
        _bead_id: &str,
        _outcome: &BeadOutcome,
    ) -> Result<(), BeadStoreError> {
        Ok(())
    }

    async fn set_nostr_event_id(
        &self,
        _bead_id: &str,
        _event_id: &str,
    ) -> Result<(), BeadStoreError> {
        Ok(())
    }

    async fn get(&self, _bead_id: &str) -> Result<Option<BeadMetadata>, BeadStoreError> {
        Ok(None)
    }

    async fn list_by_state(
        &self,
        _state: &BeadState,
    ) -> Result<Vec<BeadMetadata>, BeadStoreError> {
        Ok(Vec::new())
    }

    async fn list_failed(&self) -> Result<Vec<BeadMetadata>, BeadStoreError> {
        Ok(Vec::new())
    }

    async fn count_by_state(&self) -> Result<HashMap<String, u64>, BeadStoreError> {
        Ok(HashMap::new())
    }

    async fn store_learning(&self, _learning: &BeadLearning) -> Result<(), BeadStoreError> {
        Ok(())
    }

    async fn get_learnings(&self, _bead_id: &str) -> Result<Vec<BeadLearning>, BeadStoreError> {
        Ok(Vec::new())
    }

    async fn archive_before(&self, _cutoff: DateTime<Utc>) -> Result<u64, BeadStoreError> {
        Ok(0)
    }

    async fn health_check(&self) -> Result<BeadHealthStatus, BeadStoreError> {
        Ok(BeadHealthStatus {
            relay_connected: false,
            bridge_connected: false,
            neo4j_connected: false,
            last_publish_at: None,
            last_publish_outcome: None,
            beads_by_state: HashMap::new(),
            relay_latency_ms: None,
        })
    }

    async fn increment_retry(&self, _bead_id: &str) -> Result<u8, BeadStoreError> {
        Ok(0)
    }
}

// ---------------------------------------------------------------------------
// Neo4j-backed store
// ---------------------------------------------------------------------------

/// Neo4j-backed bead store using the shared connection pool.
pub struct Neo4jBeadStore {
    graph: Arc<Graph>,
}

impl Neo4jBeadStore {
    pub fn new(graph: Arc<Graph>) -> Self {
        Self { graph }
    }
}

#[async_trait]
impl BeadStore for Neo4jBeadStore {
    async fn create(&self, metadata: &BeadMetadata) -> Result<(), BeadStoreError> {
        let state_str = metadata.state.to_string();
        let query = Query::new(
            "MERGE (b:Bead {bead_id: $bead_id}) \
             ON CREATE SET \
               b.brief_id = $brief_id, \
               b.debrief_path = $debrief_path, \
               b.user_pubkey = $user_pubkey, \
               b.state = $state, \
               b.created_at = $created_at, \
               b.retry_count = 0"
                .to_string(),
        )
        .param("bead_id", metadata.bead_id.clone())
        .param("brief_id", metadata.brief_id.clone())
        .param("debrief_path", metadata.debrief_path.clone())
        .param(
            "user_pubkey",
            metadata.user_pubkey.clone().unwrap_or_default(),
        )
        .param("state", state_str)
        .param("created_at", metadata.created_at.to_rfc3339());

        self.graph.run(query).await.map_err(|e| {
            warn!("[BeadStore] create failed for {}: {e}", metadata.bead_id);
            BeadStoreError::QueryFailed(e.to_string())
        })
    }

    async fn update_state(&self, bead_id: &str, state: BeadState) -> Result<(), BeadStoreError> {
        let state_str = state.to_string();
        let now = Utc::now().to_rfc3339();

        let timestamp_clause = match &state {
            BeadState::Published => ", b.published_at = $ts",
            BeadState::Neo4jPersisted => ", b.persisted_at = $ts",
            BeadState::Bridged => ", b.bridged_at = $ts",
            BeadState::Archived => ", b.archived_at = $ts",
            _ => "",
        };

        let cypher = format!(
            "MATCH (b:Bead {{bead_id: $bead_id}}) \
             SET b.state = $state{timestamp_clause} \
             RETURN b.bead_id"
        );

        let mut query = Query::new(cypher)
            .param("bead_id", bead_id.to_string())
            .param("state", state_str);

        if !timestamp_clause.is_empty() {
            query = query.param("ts", now);
        }

        let mut result = self.graph.execute(query).await.map_err(|e| {
            warn!("[BeadStore] update_state failed for {bead_id}: {e}");
            BeadStoreError::QueryFailed(e.to_string())
        })?;

        if result
            .next()
            .await
            .map_err(|e| BeadStoreError::QueryFailed(e.to_string()))?
            .is_none()
        {
            return Err(BeadStoreError::NotFound(bead_id.to_string()));
        }

        debug!(
            "[BeadStore] state updated: {bead_id} -> {}",
            state.to_string()
        );
        Ok(())
    }

    async fn update_outcome(
        &self,
        bead_id: &str,
        outcome: &BeadOutcome,
    ) -> Result<(), BeadStoreError> {
        let outcome_json =
            serde_json::to_string(outcome).unwrap_or_else(|_| "unknown".to_string());

        let query = Query::new(
            "MATCH (b:Bead {bead_id: $bead_id}) \
             SET b.outcome = $outcome \
             RETURN b.bead_id"
                .to_string(),
        )
        .param("bead_id", bead_id.to_string())
        .param("outcome", outcome_json);

        let mut result = self.graph.execute(query).await.map_err(|e| {
            warn!("[BeadStore] update_outcome failed for {bead_id}: {e}");
            BeadStoreError::QueryFailed(e.to_string())
        })?;

        if result
            .next()
            .await
            .map_err(|e| BeadStoreError::QueryFailed(e.to_string()))?
            .is_none()
        {
            return Err(BeadStoreError::NotFound(bead_id.to_string()));
        }

        Ok(())
    }

    async fn set_nostr_event_id(
        &self,
        bead_id: &str,
        event_id: &str,
    ) -> Result<(), BeadStoreError> {
        let query = Query::new(
            "MATCH (b:Bead {bead_id: $bead_id}) \
             SET b.nostr_event_id = $event_id \
             RETURN b.bead_id"
                .to_string(),
        )
        .param("bead_id", bead_id.to_string())
        .param("event_id", event_id.to_string());

        let mut result = self.graph.execute(query).await.map_err(|e| {
            warn!("[BeadStore] set_nostr_event_id failed for {bead_id}: {e}");
            BeadStoreError::QueryFailed(e.to_string())
        })?;

        if result
            .next()
            .await
            .map_err(|e| BeadStoreError::QueryFailed(e.to_string()))?
            .is_none()
        {
            return Err(BeadStoreError::NotFound(bead_id.to_string()));
        }

        debug!("[BeadStore] nostr_event_id set: {bead_id} -> {event_id}");
        Ok(())
    }

    async fn get(&self, bead_id: &str) -> Result<Option<BeadMetadata>, BeadStoreError> {
        let query = Query::new(
            "MATCH (b:Bead {bead_id: $bead_id}) \
             RETURN b.bead_id AS bead_id, \
                    b.brief_id AS brief_id, \
                    b.debrief_path AS debrief_path, \
                    b.user_pubkey AS user_pubkey, \
                    b.state AS state, \
                    b.outcome AS outcome, \
                    b.created_at AS created_at, \
                    b.published_at AS published_at, \
                    b.persisted_at AS persisted_at, \
                    b.bridged_at AS bridged_at, \
                    b.archived_at AS archived_at, \
                    b.retry_count AS retry_count, \
                    b.nostr_event_id AS nostr_event_id"
                .to_string(),
        )
        .param("bead_id", bead_id.to_string());

        let mut result = self.graph.execute(query).await.map_err(|e| {
            warn!("[BeadStore] get failed for {bead_id}: {e}");
            BeadStoreError::QueryFailed(e.to_string())
        })?;

        let row = match result
            .next()
            .await
            .map_err(|e| BeadStoreError::QueryFailed(e.to_string()))?
        {
            Some(r) => r,
            None => return Ok(None),
        };

        Ok(Some(row_to_bead_metadata(&row)?))
    }

    async fn list_by_state(&self, state: &BeadState) -> Result<Vec<BeadMetadata>, BeadStoreError> {
        let state_str = state.to_string();
        let query = Query::new(
            "MATCH (b:Bead {state: $state}) \
             RETURN b.bead_id AS bead_id, \
                    b.brief_id AS brief_id, \
                    b.debrief_path AS debrief_path, \
                    b.user_pubkey AS user_pubkey, \
                    b.state AS state, \
                    b.outcome AS outcome, \
                    b.created_at AS created_at, \
                    b.published_at AS published_at, \
                    b.persisted_at AS persisted_at, \
                    b.bridged_at AS bridged_at, \
                    b.archived_at AS archived_at, \
                    b.retry_count AS retry_count, \
                    b.nostr_event_id AS nostr_event_id \
             ORDER BY b.created_at DESC"
                .to_string(),
        )
        .param("state", state_str);

        let mut result = self
            .graph
            .execute(query)
            .await
            .map_err(|e| BeadStoreError::QueryFailed(e.to_string()))?;

        let mut beads = Vec::new();
        while let Some(row) = result
            .next()
            .await
            .map_err(|e| BeadStoreError::QueryFailed(e.to_string()))?
        {
            beads.push(row_to_bead_metadata(&row)?);
        }
        Ok(beads)
    }

    async fn list_failed(&self) -> Result<Vec<BeadMetadata>, BeadStoreError> {
        let query = Query::new(
            "MATCH (b:Bead) WHERE b.state STARTS WITH 'Failed' \
             RETURN b.bead_id AS bead_id, \
                    b.brief_id AS brief_id, \
                    b.debrief_path AS debrief_path, \
                    b.user_pubkey AS user_pubkey, \
                    b.state AS state, \
                    b.outcome AS outcome, \
                    b.created_at AS created_at, \
                    b.published_at AS published_at, \
                    b.persisted_at AS persisted_at, \
                    b.bridged_at AS bridged_at, \
                    b.archived_at AS archived_at, \
                    b.retry_count AS retry_count, \
                    b.nostr_event_id AS nostr_event_id \
             ORDER BY b.created_at DESC"
                .to_string(),
        );

        let mut result = self
            .graph
            .execute(query)
            .await
            .map_err(|e| BeadStoreError::QueryFailed(e.to_string()))?;

        let mut beads = Vec::new();
        while let Some(row) = result
            .next()
            .await
            .map_err(|e| BeadStoreError::QueryFailed(e.to_string()))?
        {
            beads.push(row_to_bead_metadata(&row)?);
        }
        Ok(beads)
    }

    async fn count_by_state(&self) -> Result<HashMap<String, u64>, BeadStoreError> {
        let query = Query::new(
            "MATCH (b:Bead) \
             RETURN b.state AS state, count(b) AS cnt"
                .to_string(),
        );

        let mut result = self
            .graph
            .execute(query)
            .await
            .map_err(|e| BeadStoreError::QueryFailed(e.to_string()))?;

        let mut counts = HashMap::new();
        while let Some(row) = result
            .next()
            .await
            .map_err(|e| BeadStoreError::QueryFailed(e.to_string()))?
        {
            let state: String = row.get("state").unwrap_or_default();
            let cnt: i64 = row.get("cnt").unwrap_or(0);
            counts.insert(state, cnt as u64);
        }
        Ok(counts)
    }

    async fn store_learning(&self, learning: &BeadLearning) -> Result<(), BeadStoreError> {
        let query = Query::new(
            "MATCH (b:Bead {bead_id: $bead_id}) \
             CREATE (l:BeadLearning { \
               bead_id: $bead_id, \
               what_worked: $what_worked, \
               what_failed: $what_failed, \
               reusable_pattern: $reusable_pattern, \
               confidence: $confidence, \
               recorded_at: $recorded_at \
             }) \
             MERGE (l)-[:LEARNING_OF]->(b)"
                .to_string(),
        )
        .param("bead_id", learning.bead_id.clone())
        .param(
            "what_worked",
            learning.what_worked.clone().unwrap_or_default(),
        )
        .param(
            "what_failed",
            learning.what_failed.clone().unwrap_or_default(),
        )
        .param(
            "reusable_pattern",
            learning.reusable_pattern.clone().unwrap_or_default(),
        )
        .param("confidence", learning.confidence as f64)
        .param("recorded_at", learning.recorded_at.to_rfc3339());

        self.graph.run(query).await.map_err(|e| {
            warn!(
                "[BeadStore] store_learning failed for {}: {e}",
                learning.bead_id
            );
            BeadStoreError::QueryFailed(e.to_string())
        })
    }

    async fn get_learnings(&self, bead_id: &str) -> Result<Vec<BeadLearning>, BeadStoreError> {
        let query = Query::new(
            "MATCH (l:BeadLearning {bead_id: $bead_id}) \
             RETURN l.bead_id AS bead_id, \
                    l.what_worked AS what_worked, \
                    l.what_failed AS what_failed, \
                    l.reusable_pattern AS reusable_pattern, \
                    l.confidence AS confidence, \
                    l.recorded_at AS recorded_at \
             ORDER BY l.recorded_at DESC"
                .to_string(),
        )
        .param("bead_id", bead_id.to_string());

        let mut result = self
            .graph
            .execute(query)
            .await
            .map_err(|e| BeadStoreError::QueryFailed(e.to_string()))?;

        let mut learnings = Vec::new();
        while let Some(row) = result
            .next()
            .await
            .map_err(|e| BeadStoreError::QueryFailed(e.to_string()))?
        {
            let what_worked: String = row.get("what_worked").unwrap_or_default();
            let what_failed: String = row.get("what_failed").unwrap_or_default();
            let reusable_pattern: String = row.get("reusable_pattern").unwrap_or_default();
            let confidence: f64 = row.get("confidence").unwrap_or(0.0);
            let recorded_at_str: String = row.get("recorded_at").unwrap_or_default();

            learnings.push(BeadLearning {
                bead_id: bead_id.to_string(),
                what_worked: non_empty(what_worked),
                what_failed: non_empty(what_failed),
                reusable_pattern: non_empty(reusable_pattern),
                confidence: confidence as f32,
                recorded_at: recorded_at_str
                    .parse::<DateTime<Utc>>()
                    .unwrap_or_else(|_| Utc::now()),
            });
        }
        Ok(learnings)
    }

    async fn archive_before(&self, cutoff: DateTime<Utc>) -> Result<u64, BeadStoreError> {
        let query = Query::new(
            "MATCH (b:Bead) \
             WHERE b.created_at < $cutoff AND b.state <> 'Archived' \
             SET b.state = 'Archived', b.archived_at = $now \
             RETURN count(b) AS cnt"
                .to_string(),
        )
        .param("cutoff", cutoff.to_rfc3339())
        .param("now", Utc::now().to_rfc3339());

        let mut result = self
            .graph
            .execute(query)
            .await
            .map_err(|e| BeadStoreError::QueryFailed(e.to_string()))?;

        let cnt: i64 = result
            .next()
            .await
            .map_err(|e| BeadStoreError::QueryFailed(e.to_string()))?
            .and_then(|row| row.get("cnt").ok())
            .unwrap_or(0);

        debug!("[BeadStore] archived {cnt} beads before {cutoff}");
        Ok(cnt as u64)
    }

    async fn health_check(&self) -> Result<BeadHealthStatus, BeadStoreError> {
        let probe = Query::new("RETURN 1 AS ok".to_string());
        let neo4j_connected = self.graph.execute(probe).await.is_ok();

        let beads_by_state = if neo4j_connected {
            self.count_by_state().await.unwrap_or_default()
        } else {
            HashMap::new()
        };

        Ok(BeadHealthStatus {
            relay_connected: false,
            bridge_connected: false,
            neo4j_connected,
            last_publish_at: None,
            last_publish_outcome: None,
            beads_by_state,
            relay_latency_ms: None,
        })
    }

    async fn increment_retry(&self, bead_id: &str) -> Result<u8, BeadStoreError> {
        let query = Query::new(
            "MATCH (b:Bead {bead_id: $bead_id}) \
             SET b.retry_count = coalesce(b.retry_count, 0) + 1 \
             RETURN b.retry_count AS retry_count"
                .to_string(),
        )
        .param("bead_id", bead_id.to_string());

        let mut result = self.graph.execute(query).await.map_err(|e| {
            warn!("[BeadStore] increment_retry failed for {bead_id}: {e}");
            BeadStoreError::QueryFailed(e.to_string())
        })?;

        let count: i64 = result
            .next()
            .await
            .map_err(|e| BeadStoreError::QueryFailed(e.to_string()))?
            .ok_or_else(|| BeadStoreError::NotFound(bead_id.to_string()))?
            .get("retry_count")
            .unwrap_or(0);

        Ok(count as u8)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse a Neo4j row into `BeadMetadata`.
fn row_to_bead_metadata(row: &neo4rs::Row) -> Result<BeadMetadata, BeadStoreError> {
    let bead_id: String = row.get("bead_id").unwrap_or_default();
    let brief_id: String = row.get("brief_id").unwrap_or_default();
    let debrief_path: String = row.get("debrief_path").unwrap_or_default();
    let user_pubkey: String = row.get("user_pubkey").unwrap_or_default();
    let state_str: String = row.get("state").unwrap_or_default();
    let outcome_str: String = row.get("outcome").unwrap_or_default();
    let created_at_str: String = row.get("created_at").unwrap_or_default();
    let published_at_str: String = row.get("published_at").unwrap_or_default();
    let persisted_at_str: String = row.get("persisted_at").unwrap_or_default();
    let bridged_at_str: String = row.get("bridged_at").unwrap_or_default();
    let archived_at_str: String = row.get("archived_at").unwrap_or_default();
    let retry_count: i64 = row.get("retry_count").unwrap_or(0);
    let nostr_event_id: String = row.get("nostr_event_id").unwrap_or_default();

    let state = parse_bead_state(&state_str);
    let outcome = if outcome_str.is_empty() {
        None
    } else {
        serde_json::from_str(&outcome_str).ok()
    };

    Ok(BeadMetadata {
        bead_id,
        brief_id,
        debrief_path,
        user_pubkey: non_empty(user_pubkey),
        state,
        outcome,
        created_at: parse_datetime_or_now(&created_at_str),
        published_at: parse_optional_datetime(&published_at_str),
        persisted_at: parse_optional_datetime(&persisted_at_str),
        bridged_at: parse_optional_datetime(&bridged_at_str),
        archived_at: parse_optional_datetime(&archived_at_str),
        retry_count: retry_count as u8,
        nostr_event_id: non_empty(nostr_event_id),
    })
}

/// Parse a state string back into `BeadState`.
fn parse_bead_state(s: &str) -> BeadState {
    match s {
        "Created" => BeadState::Created,
        "Publishing" => BeadState::Publishing,
        "Published" => BeadState::Published,
        "Neo4jPersisted" => BeadState::Neo4jPersisted,
        "Bridged" => BeadState::Bridged,
        "Archived" => BeadState::Archived,
        other if other.starts_with("Failed(Transient:") => {
            let msg = other
                .trim_start_matches("Failed(Transient: ")
                .trim_end_matches(')');
            BeadState::Failed(BeadFailure::Transient(msg.to_string()))
        }
        other if other.starts_with("Failed(Permanent:") => {
            let msg = other
                .trim_start_matches("Failed(Permanent: ")
                .trim_end_matches(')');
            BeadState::Failed(BeadFailure::Permanent(msg.to_string()))
        }
        other => BeadState::Failed(BeadFailure::Permanent(format!("unknown state: {other}"))),
    }
}

fn parse_datetime_or_now(s: &str) -> DateTime<Utc> {
    s.parse::<DateTime<Utc>>().unwrap_or_else(|_| Utc::now())
}

fn parse_optional_datetime(s: &str) -> Option<DateTime<Utc>> {
    if s.is_empty() {
        None
    } else {
        s.parse::<DateTime<Utc>>().ok()
    }
}

/// Convert empty string to None, non-empty to Some.
fn non_empty(s: String) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use std::collections::HashMap;
    use tokio::sync::RwLock;

    /// In-memory mock implementation of `BeadStore` for testing.
    struct MockBeadStore {
        beads: RwLock<HashMap<String, BeadMetadata>>,
        learnings: RwLock<Vec<BeadLearning>>,
    }

    impl MockBeadStore {
        fn new() -> Self {
            Self {
                beads: RwLock::new(HashMap::new()),
                learnings: RwLock::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl BeadStore for MockBeadStore {
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
            let bead = map
                .get_mut(bead_id)
                .ok_or_else(|| BeadStoreError::NotFound(bead_id.to_string()))?;
            bead.state = state;
            Ok(())
        }

        async fn update_outcome(
            &self,
            bead_id: &str,
            outcome: &BeadOutcome,
        ) -> Result<(), BeadStoreError> {
            let mut map = self.beads.write().await;
            let bead = map
                .get_mut(bead_id)
                .ok_or_else(|| BeadStoreError::NotFound(bead_id.to_string()))?;
            bead.outcome = Some(outcome.clone());
            Ok(())
        }

        async fn set_nostr_event_id(
            &self,
            bead_id: &str,
            event_id: &str,
        ) -> Result<(), BeadStoreError> {
            let mut map = self.beads.write().await;
            let bead = map
                .get_mut(bead_id)
                .ok_or_else(|| BeadStoreError::NotFound(bead_id.to_string()))?;
            bead.nostr_event_id = Some(event_id.to_string());
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
                *counts.entry(bead.state.to_string()).or_insert(0u64) += 1;
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
            Ok(self
                .learnings
                .read()
                .await
                .iter()
                .filter(|l| l.bead_id == bead_id)
                .cloned()
                .collect())
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
            let counts = self.count_by_state().await?;
            Ok(BeadHealthStatus {
                relay_connected: false,
                bridge_connected: false,
                neo4j_connected: true,
                last_publish_at: None,
                last_publish_outcome: None,
                beads_by_state: counts,
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

    fn make_bead(id: &str) -> BeadMetadata {
        BeadMetadata::new(id.into(), "brief-1".into(), "/debrief".into(), None)
    }

    // ── create and get ─────────────────────────────────────────────────

    #[tokio::test]
    async fn create_and_get_returns_the_bead() {
        // GIVEN: an empty store
        let store = MockBeadStore::new();
        let bead = make_bead("bead-1");

        // WHEN: creating and retrieving
        store.create(&bead).await.unwrap();
        let got = store.get("bead-1").await.unwrap().expect("should exist");

        // THEN: the bead is returned with correct fields
        assert_eq!(got.bead_id, "bead-1");
        assert_eq!(got.state, BeadState::Created);
    }

    #[tokio::test]
    async fn get_nonexistent_returns_none() {
        // GIVEN: an empty store
        let store = MockBeadStore::new();

        // WHEN: getting a bead that doesn't exist
        let result = store.get("no-such-bead").await.unwrap();

        // THEN: None is returned
        assert!(result.is_none());
    }

    // ── update_state ───────────────────────────────────────────────────

    #[tokio::test]
    async fn update_state_changes_the_state() {
        // GIVEN: a bead in Created state
        let store = MockBeadStore::new();
        store.create(&make_bead("bead-2")).await.unwrap();

        // WHEN: updating to Publishing
        store
            .update_state("bead-2", BeadState::Publishing)
            .await
            .unwrap();

        // THEN: state is Publishing
        let bead = store.get("bead-2").await.unwrap().unwrap();
        assert_eq!(bead.state, BeadState::Publishing);
    }

    // ── update_outcome ─────────────────────────────────────────────────

    #[tokio::test]
    async fn update_outcome_records_the_outcome() {
        // GIVEN: a bead in the store
        let store = MockBeadStore::new();
        store.create(&make_bead("bead-3")).await.unwrap();

        // WHEN: recording a Success outcome
        store
            .update_outcome("bead-3", &BeadOutcome::Success)
            .await
            .unwrap();

        // THEN: outcome is Success
        let bead = store.get("bead-3").await.unwrap().unwrap();
        assert_eq!(bead.outcome, Some(BeadOutcome::Success));
    }

    // ── list_by_state ──────────────────────────────────────────────────

    #[tokio::test]
    async fn list_by_state_filters_correctly() {
        // GIVEN: beads in different states
        let store = MockBeadStore::new();
        store.create(&make_bead("a")).await.unwrap();
        store.create(&make_bead("b")).await.unwrap();
        store.create(&make_bead("c")).await.unwrap();
        store
            .update_state("b", BeadState::Published)
            .await
            .unwrap();

        // WHEN: listing Created beads
        let created = store.list_by_state(&BeadState::Created).await.unwrap();

        // THEN: only a and c are returned
        assert_eq!(created.len(), 2);
        assert!(created.iter().all(|b| b.state == BeadState::Created));

        // WHEN: listing Published beads
        let published = store.list_by_state(&BeadState::Published).await.unwrap();
        assert_eq!(published.len(), 1);
        assert_eq!(published[0].bead_id, "b");
    }

    // ── list_failed ────────────────────────────────────────────────────

    #[tokio::test]
    async fn list_failed_returns_only_failed_beads() {
        // GIVEN: mix of states including failed
        let store = MockBeadStore::new();
        store.create(&make_bead("ok")).await.unwrap();
        store.create(&make_bead("fail")).await.unwrap();
        store
            .update_state(
                "fail",
                BeadState::Failed(BeadFailure::Transient("timeout".into())),
            )
            .await
            .unwrap();

        // WHEN: listing failed
        let failed = store.list_failed().await.unwrap();

        // THEN: only the failed bead
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].bead_id, "fail");
    }

    // ── count_by_state ─────────────────────────────────────────────────

    #[tokio::test]
    async fn count_by_state_returns_correct_counts() {
        // GIVEN: beads in different states
        let store = MockBeadStore::new();
        store.create(&make_bead("x1")).await.unwrap();
        store.create(&make_bead("x2")).await.unwrap();
        store.create(&make_bead("x3")).await.unwrap();
        store
            .update_state("x3", BeadState::Published)
            .await
            .unwrap();

        // WHEN: counting
        let counts = store.count_by_state().await.unwrap();

        // THEN: 2 Created, 1 Published
        assert_eq!(counts.get("Created"), Some(&2));
        assert_eq!(counts.get("Published"), Some(&1));
    }

    // ── store_learning and get_learnings ────────────────────────────────

    #[tokio::test]
    async fn store_and_get_learnings_roundtrip() {
        // GIVEN: a learning entry
        let store = MockBeadStore::new();
        let learning = BeadLearning {
            bead_id: "bead-learn".into(),
            what_worked: Some("retry helped".into()),
            what_failed: None,
            reusable_pattern: None,
            confidence: 0.8,
            recorded_at: Utc::now(),
        };

        // WHEN: storing and retrieving
        store.store_learning(&learning).await.unwrap();
        let got = store.get_learnings("bead-learn").await.unwrap();

        // THEN: one entry with matching fields
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].what_worked, Some("retry helped".into()));
        assert!((got[0].confidence - 0.8).abs() < f32::EPSILON);
    }

    #[tokio::test]
    async fn get_learnings_filters_by_bead_id() {
        // GIVEN: learnings for different beads
        let store = MockBeadStore::new();
        let l1 = BeadLearning {
            bead_id: "a".into(),
            what_worked: None,
            what_failed: None,
            reusable_pattern: None,
            confidence: 0.5,
            recorded_at: Utc::now(),
        };
        let l2 = BeadLearning {
            bead_id: "b".into(),
            what_worked: None,
            what_failed: None,
            reusable_pattern: None,
            confidence: 0.9,
            recorded_at: Utc::now(),
        };
        store.store_learning(&l1).await.unwrap();
        store.store_learning(&l2).await.unwrap();

        // WHEN: getting learnings for bead "a"
        let got = store.get_learnings("a").await.unwrap();

        // THEN: only bead "a" learning
        assert_eq!(got.len(), 1);
        assert!((got[0].confidence - 0.5).abs() < f32::EPSILON);
    }

    // ── archive_before ─────────────────────────────────────────────────

    #[tokio::test]
    async fn archive_before_moves_old_beads_to_archived() {
        // GIVEN: beads in the store (created_at is ~Utc::now())
        let store = MockBeadStore::new();
        store.create(&make_bead("old")).await.unwrap();

        // WHEN: archiving with a cutoff in the future
        let cutoff = Utc::now() + Duration::hours(1);
        let count = store.archive_before(cutoff).await.unwrap();

        // THEN: bead is archived
        assert_eq!(count, 1);
        let bead = store.get("old").await.unwrap().unwrap();
        assert_eq!(bead.state, BeadState::Archived);
        assert!(bead.archived_at.is_some());
    }

    #[tokio::test]
    async fn archive_before_skips_already_archived() {
        // GIVEN: an already-archived bead
        let store = MockBeadStore::new();
        store.create(&make_bead("done")).await.unwrap();
        store
            .update_state("done", BeadState::Archived)
            .await
            .unwrap();

        // WHEN: archiving again
        let cutoff = Utc::now() + Duration::hours(1);
        let count = store.archive_before(cutoff).await.unwrap();

        // THEN: count is 0 -- not re-archived
        assert_eq!(count, 0);
    }

    // ── increment_retry ────────────────────────────────────────────────

    #[tokio::test]
    async fn increment_retry_increases_count() {
        // GIVEN: a bead with retry_count=0
        let store = MockBeadStore::new();
        store.create(&make_bead("r1")).await.unwrap();

        // WHEN: incrementing twice
        let c1 = store.increment_retry("r1").await.unwrap();
        let c2 = store.increment_retry("r1").await.unwrap();

        // THEN: counts are 1 and 2
        assert_eq!(c1, 1);
        assert_eq!(c2, 2);
    }

    #[tokio::test]
    async fn increment_retry_on_nonexistent_returns_error() {
        // GIVEN: empty store
        let store = MockBeadStore::new();

        // WHEN: incrementing nonexistent bead
        let result = store.increment_retry("ghost").await;

        // THEN: NotFound error
        assert!(result.is_err());
    }

    // ── health_check ───────────────────────────────────────────────────

    #[tokio::test]
    async fn health_check_returns_status() {
        // GIVEN: a mock store
        let store = MockBeadStore::new();

        // WHEN: checking health
        let status = store.health_check().await.unwrap();

        // THEN: returns a status (mock says neo4j_connected=true)
        assert!(status.neo4j_connected);
    }

    // ── set_nostr_event_id ─────────────────────────────────────────────

    #[tokio::test]
    async fn set_nostr_event_id_persists() {
        let store = MockBeadStore::new();
        store.create(&make_bead("ev-1")).await.unwrap();

        store
            .set_nostr_event_id("ev-1", "abc123")
            .await
            .unwrap();

        let bead = store.get("ev-1").await.unwrap().unwrap();
        assert_eq!(bead.nostr_event_id, Some("abc123".to_string()));
    }

    // ── parse_bead_state ──────────────────────────────────────────────

    #[test]
    fn parse_bead_state_roundtrips_simple_variants() {
        let states = vec![
            BeadState::Created,
            BeadState::Publishing,
            BeadState::Published,
            BeadState::Neo4jPersisted,
            BeadState::Bridged,
            BeadState::Archived,
        ];
        for state in states {
            let s = state.to_string();
            assert_eq!(parse_bead_state(&s), state);
        }
    }

    #[test]
    fn parse_bead_state_failed_transient() {
        let state = BeadState::Failed(BeadFailure::Transient("relay down".to_string()));
        let s = state.to_string();
        let parsed = parse_bead_state(&s);
        assert!(matches!(
            parsed,
            BeadState::Failed(BeadFailure::Transient(_))
        ));
    }

    #[test]
    fn parse_bead_state_unknown_falls_to_permanent() {
        let parsed = parse_bead_state("Bogus");
        assert!(matches!(
            parsed,
            BeadState::Failed(BeadFailure::Permanent(_))
        ));
    }

    // ── NoopBeadStore ─────────────────────────────────────────────────

    #[tokio::test]
    async fn noop_store_returns_empty() {
        let store = NoopBeadStore;
        assert!(store.get("x").await.unwrap().is_none());
        assert!(store
            .list_by_state(&BeadState::Created)
            .await
            .unwrap()
            .is_empty());
        assert!(store.list_failed().await.unwrap().is_empty());
        assert!(store.count_by_state().await.unwrap().is_empty());
        assert!(store.get_learnings("x").await.unwrap().is_empty());
        assert_eq!(store.archive_before(Utc::now()).await.unwrap(), 0);
        assert_eq!(store.increment_retry("x").await.unwrap(), 0);

        let health = store.health_check().await.unwrap();
        assert!(!health.neo4j_connected);
    }
}
