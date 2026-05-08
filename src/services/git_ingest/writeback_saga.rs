//! Write-Back Saga — PRD-013 G4.
//!
//! Orchestrates the reverse flow of the ingest pipeline: enrichments that have
//! been approved by the Judgment Broker are committed back to the source pod
//! or repository. The saga fetches the latest state, applies the enrichment,
//! commits with full provenance trailers, and pushes.
//!
//! # Saga phases
//!
//! 1. **Fetch** — `git fetch` latest from the remote to ensure no conflicts.
//! 2. **Apply** — Write the enrichment to the local worktree in the
//!    appropriate file format (`.ttl`, `.embeddings.json`, `.proposals.md`).
//! 3. **Commit** — Create a commit with provenance trailers (G3).
//! 4. **Push** — `git push` to the remote (auth per `RemoteAuth`).
//! 5. **Record** — Persist the push result in Neo4j (audit trail).
//!
//! # Feature gate
//!
//! Write-back is disabled by default (`WRITEBACK_ENABLED=false`). It must be
//! opted in per-remote (`GitRemote.writeback_enabled`) AND globally.

use std::path::Path;
use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::Mutex as AsyncMutex;

use chrono::Utc;
use git2::{
    Cred, FetchOptions, IndexAddOption, PushOptions, RemoteCallbacks, Repository, Signature,
};
use log::{debug, info, warn};
use neo4rs::{query, Graph};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::provenance::{encode_commit_message, ProvenanceTrailer};
use super::remote_registry::RemoteRegistry;
use super::RemoteAuth;

// ---------------------------------------------------------------------------
// Feature flag
// ---------------------------------------------------------------------------

pub const WRITEBACK_ENABLED_ENV: &str = "WRITEBACK_ENABLED";

pub fn writeback_enabled() -> bool {
    std::env::var(WRITEBACK_ENABLED_ENV)
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrichmentPayload {
    pub enrichment_type: EnrichmentType,
    pub target_path: String,
    pub content: String,
    pub commit_subject: String,
    pub commit_body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EnrichmentType {
    OntologyPromotion,
    EmbeddingUpdate,
    GapDetection,
    AgentAnnotation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionReport {
    pub case_id: String,
    pub decision: String,
    pub proposed_by: String,
    pub approved_by: String,
    pub reasoning: String,
    pub server_did: String,
    pub entity_urn: String,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum WriteBackError {
    #[error("write-back is globally disabled (set WRITEBACK_ENABLED=true)")]
    GloballyDisabled,

    #[error("write-back is disabled for remote {0}")]
    RemoteDisabled(String),

    #[error("git operation failed: {0}")]
    Git(String),

    #[error("enrichment apply failed: {0}")]
    ApplyFailed(String),

    #[error("push failed: {0}")]
    PushFailed(String),

    #[error("audit recording failed: {0}")]
    AuditFailed(String),

    #[error("conflict detected: local HEAD {local} is not ancestor of remote {remote}")]
    Conflict { local: String, remote: String },

    #[error("remote not found in registry: {0}")]
    RemoteNotFound(String),
}

impl WriteBackError {
    /// Returns `true` when the error represents a non-fast-forward conflict,
    /// i.e. the remote has diverged since the last fetch.
    pub fn is_conflict(&self) -> bool {
        matches!(self, WriteBackError::Conflict { .. })
    }
}

// ---------------------------------------------------------------------------
// WriteBackSaga
// ---------------------------------------------------------------------------

pub struct WriteBackSaga {
    registry: RemoteRegistry,
    graph: Arc<Graph>,
    /// Per-remote mutex prevents concurrent write-backs from corrupting git state.
    remote_locks: DashMap<String, Arc<AsyncMutex<()>>>,
}

impl WriteBackSaga {
    pub fn new(registry: RemoteRegistry, graph: Arc<Graph>) -> Self {
        Self {
            registry,
            graph,
            remote_locks: DashMap::new(),
        }
    }

    pub async fn execute(
        &self,
        remote_id: &str,
        enrichment: &EnrichmentPayload,
        decision: &DecisionReport,
    ) -> Result<WriteBackResult, WriteBackError> {
        if !writeback_enabled() {
            return Err(WriteBackError::GloballyDisabled);
        }

        let remote = self
            .registry
            .get(remote_id)
            .await
            .map_err(|_| WriteBackError::RemoteNotFound(remote_id.to_string()))?;

        if !remote.writeback_enabled {
            return Err(WriteBackError::RemoteDisabled(remote_id.to_string()));
        }

        // H3 fix: acquire per-remote mutex to serialise concurrent write-backs.
        let lock = self
            .remote_locks
            .entry(remote_id.to_string())
            .or_insert_with(|| Arc::new(AsyncMutex::new(())))
            .clone();
        let _guard = lock.lock().await;

        let local_path = remote.local_path();
        let url = remote.url.clone();
        let branch = remote.branch.clone();
        let auth = remote.auth.clone();
        let target_path = enrichment.target_path.clone();
        let content = enrichment.content.clone();
        let commit_subject = enrichment.commit_subject.clone();
        let commit_body = enrichment.commit_body.clone();

        let trailer = ProvenanceTrailer::new(
            &decision.entity_urn,
            &decision.proposed_by,
            &decision.approved_by,
            &decision.case_id,
            &decision.decision,
            &decision.reasoning,
            Utc::now(),
            &decision.server_did,
        );

        let case_id = decision.case_id.clone();
        let remote_id_owned = remote_id.to_string();

        let blocking_result = tokio::task::spawn_blocking(move || {
            writeback_blocking(
                &local_path,
                &url,
                &branch,
                &auth,
                &target_path,
                &content,
                &commit_subject,
                &commit_body,
                &trailer,
            )
        })
        .await
        .map_err(|e| WriteBackError::Git(format!("spawn_blocking join: {e}")))?;

        let commit_sha = match blocking_result {
            Ok(sha) => sha,
            Err(e) => {
                if e.is_conflict() {
                    warn!(
                        "write-back: case {} remote {} — non-fast-forward conflict; \
                         remote has diverged since last fetch",
                        case_id, remote_id_owned
                    );
                }
                return Err(e);
            }
        };

        let pushed_at = Utc::now();
        self.record_audit(&remote_id_owned, &commit_sha, &case_id, &pushed_at)
            .await?;

        info!(
            "write-back: saga complete for case {} → remote {} ({})",
            case_id, remote_id_owned, commit_sha
        );

        Ok(WriteBackResult {
            commit_sha,
            remote_id: remote_id_owned,
            case_id,
            pushed_at,
        })
    }

    async fn record_audit(
        &self,
        remote_id: &str,
        commit_sha: &str,
        case_id: &str,
        pushed_at: &chrono::DateTime<Utc>,
    ) -> Result<(), WriteBackError> {
        // H7 fix: single atomic query to create audit node + PUSHED_TO relationship.
        self.graph
            .run(
                query(
                    "MATCH (r:GitRemote {id: $remote_id}) \
                     CREATE (a:WriteBackAudit { \
                         remote_id: $remote_id, \
                         commit_sha: $commit_sha, \
                         case_id: $case_id, \
                         pushed_at: $pushed_at \
                     })-[:PUSHED_TO]->(r)",
                )
                .param("remote_id", remote_id)
                .param("commit_sha", commit_sha)
                .param("case_id", case_id)
                .param("pushed_at", pushed_at.to_rfc3339()),
            )
            .await
            .map_err(|e| WriteBackError::AuditFailed(e.to_string()))?;

        debug!(
            "write-back: audit recorded for case {} commit {}",
            case_id, commit_sha
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Blocking git2 operations (runs on spawn_blocking)
// ---------------------------------------------------------------------------

fn writeback_blocking(
    local_path: &Path,
    url: &str,
    branch: &str,
    auth: &RemoteAuth,
    target_path: &str,
    content: &str,
    commit_subject: &str,
    commit_body: &str,
    trailer: &ProvenanceTrailer,
) -> Result<String, WriteBackError> {
    // Sanitise commit subject — strip newlines to prevent trailer injection.
    let commit_subject = commit_subject.replace('\n', " ").replace('\r', " ");
    let commit_body = commit_body
        .lines()
        .filter(|line| {
            // Reject lines that look like git trailers to prevent injection.
            !line.contains(": ") || !line.starts_with(|c: char| c.is_ascii_uppercase())
        })
        .collect::<Vec<_>>()
        .join("\n");

    // Reject target_path with traversal components.
    if target_path.contains("..") {
        return Err(WriteBackError::ApplyFailed(
            "target_path must not contain '..'".to_string(),
        ));
    }

    // Phase 1: Fetch latest and verify fast-forward safety
    let repo =
        Repository::open(local_path).map_err(|e| WriteBackError::Git(format!("open: {e}")))?;

    let (callbacks, _) = build_push_callbacks(auth)?;
    let mut fetch_opts = FetchOptions::new();
    fetch_opts.remote_callbacks(callbacks);

    let mut origin = repo
        .find_remote("origin")
        .or_else(|_| repo.remote("origin", url))
        .map_err(|e| WriteBackError::Git(format!("find remote: {e}")))?;

    let refspec = format!("refs/heads/{b}:refs/remotes/origin/{b}", b = branch);
    origin
        .fetch(&[&refspec], Some(&mut fetch_opts), None)
        .map_err(|e| WriteBackError::Git(format!("fetch: {e}")))?;
    drop(origin);

    let remote_ref = repo
        .find_reference(&format!("refs/remotes/origin/{}", branch))
        .map_err(|e| WriteBackError::Git(format!("find remote ref: {e}")))?;
    let remote_oid = remote_ref
        .target()
        .ok_or_else(|| WriteBackError::Git("remote ref has no target".into()))?;

    let local_ref_name = format!("refs/heads/{}", branch);
    let local_oid = repo
        .find_reference(&local_ref_name)
        .ok()
        .and_then(|r| r.target());

    if let Some(local) = local_oid {
        if local != remote_oid {
            let (ahead, _behind) = repo
                .graph_ahead_behind(local, remote_oid)
                .map_err(|e| WriteBackError::Git(format!("graph_ahead_behind: {e}")))?;
            if ahead > 0 {
                warn!(
                    "write-back: local branch has {} unpushed commits; rebasing not implemented, \
                     resetting to remote HEAD",
                    ahead
                );
            }
        }
    }

    // Fast-forward local branch to remote HEAD
    let remote_commit = repo
        .find_commit(remote_oid)
        .map_err(|e| WriteBackError::Git(format!("find remote commit: {e}")))?;
    repo.reference(
        &local_ref_name,
        remote_oid,
        true,
        "write-back: fast-forward",
    )
    .map_err(|e| WriteBackError::Git(format!("ff reference: {e}")))?;
    repo.set_head(&local_ref_name)
        .map_err(|e| WriteBackError::Git(format!("set_head: {e}")))?;
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .map_err(|e| WriteBackError::Git(format!("checkout: {e}")))?;

    debug!("write-back: phase 1 complete — local at {}", remote_oid);

    // Phase 2: Apply enrichment to worktree
    // Guard: canonicalise and verify the target stays inside the repo root.
    let file_path = local_path.join(target_path);
    let canonical_root = local_path
        .canonicalize()
        .map_err(|e| WriteBackError::ApplyFailed(format!("canonicalize root: {e}")))?;
    if let Some(parent) = file_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| WriteBackError::ApplyFailed(format!("mkdir: {e}")))?;
    }
    let canonical_file = file_path
        .canonicalize()
        .or_else(|_| {
            // File doesn't exist yet — canonicalise the parent and append the filename.
            file_path.parent().map_or_else(
                || {
                    Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "no parent",
                    ))
                },
                |p| {
                    Ok(p.canonicalize()?
                        .join(file_path.file_name().unwrap_or_default()))
                },
            )
        })
        .map_err(|e| WriteBackError::ApplyFailed(format!("canonicalize target: {e}")))?;
    if !canonical_file.starts_with(&canonical_root) {
        return Err(WriteBackError::ApplyFailed(format!(
            "path traversal rejected: {} escapes repo root",
            target_path
        )));
    }
    std::fs::write(&file_path, content)
        .map_err(|e| WriteBackError::ApplyFailed(format!("write {}: {e}", target_path)))?;

    debug!("write-back: phase 2 complete — wrote {}", target_path);

    // Phase 3: Stage and commit with provenance trailers
    let mut index = repo
        .index()
        .map_err(|e| WriteBackError::Git(format!("index: {e}")))?;
    index
        .add_all([target_path].iter(), IndexAddOption::DEFAULT, None)
        .map_err(|e| WriteBackError::Git(format!("index add: {e}")))?;
    index
        .write()
        .map_err(|e| WriteBackError::Git(format!("index write: {e}")))?;
    let tree_oid = index
        .write_tree()
        .map_err(|e| WriteBackError::Git(format!("write_tree: {e}")))?;
    let tree = repo
        .find_tree(tree_oid)
        .map_err(|e| WriteBackError::Git(format!("find_tree: {e}")))?;

    let commit_message = encode_commit_message(&commit_subject, &commit_body, trailer);
    let sig = Signature::now("VisionClaw Ingest", "ingest@visionclaw.local")
        .map_err(|e| WriteBackError::Git(format!("signature: {e}")))?;

    let commit_oid = repo
        .commit(
            Some(&local_ref_name),
            &sig,
            &sig,
            &commit_message,
            &tree,
            &[&remote_commit],
        )
        .map_err(|e| WriteBackError::Git(format!("commit: {e}")))?;

    let commit_sha = commit_oid.to_string();
    debug!("write-back: phase 3 complete — committed {}", commit_sha);

    // Phase 4: Push to remote
    // H4: refuse to send private key material over non-HTTPS transport.
    if matches!(auth, super::RemoteAuth::DidNostr { .. }) && !url.starts_with("https://") {
        warn!(
            "write-back: refusing DidNostr auth over non-HTTPS URL: {}",
            url
        );
        return Err(WriteBackError::PushFailed(
            "DidNostr auth requires HTTPS remote URL".to_string(),
        ));
    }
    let (push_callbacks, push_rejection) = build_push_callbacks(auth)?;
    let mut push_opts = PushOptions::new();
    push_opts.remote_callbacks(push_callbacks);

    let mut origin = repo
        .find_remote("origin")
        .map_err(|e| WriteBackError::PushFailed(format!("find remote: {e}")))?;

    let push_refspec = format!("refs/heads/{}:refs/heads/{}", branch, branch);

    // Capture the local HEAD sha before push for conflict diagnostics.
    let local_head_sha = repo
        .head()
        .ok()
        .and_then(|h| h.target())
        .map(|oid| oid.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let remote_ref_name = format!("refs/heads/{}", branch);

    origin
        .push(&[&push_refspec], Some(&mut push_opts))
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("non-fast-forward")
                || msg.contains("rejected")
                || msg.contains("fetch first")
            {
                WriteBackError::Conflict {
                    local: local_head_sha.clone(),
                    remote: remote_ref_name.clone(),
                }
            } else {
                WriteBackError::PushFailed(format!("push: {e}"))
            }
        })?;

    // Check if the remote rejected the push (H8 fix).
    if let Ok(guard) = push_rejection.lock() {
        if let Some(ref msg) = *guard {
            if msg.contains("non-fast-forward") || msg.contains("fetch first") {
                return Err(WriteBackError::Conflict {
                    local: local_head_sha,
                    remote: remote_ref_name,
                });
            }
            return Err(WriteBackError::PushFailed(format!(
                "remote rejected push: {}",
                msg
            )));
        }
    }

    info!(
        "write-back: phase 4 complete — pushed {} to origin",
        commit_sha
    );

    Ok(commit_sha)
}

fn build_push_callbacks(
    auth: &RemoteAuth,
) -> Result<(RemoteCallbacks<'_>, Arc<std::sync::Mutex<Option<String>>>), WriteBackError> {
    let mut callbacks = RemoteCallbacks::new();

    match auth {
        RemoteAuth::None => {}
        RemoteAuth::Pat { token_env_var } => {
            let token = std::env::var(token_env_var).map_err(|_| {
                WriteBackError::PushFailed(format!("env var {} not set", token_env_var))
            })?;
            let token_owned = token.clone();
            callbacks.credentials(move |_url, _username_from_url, _allowed_types| {
                Cred::userpass_plaintext("x-access-token", &token_owned)
            });
        }
        RemoteAuth::DidNostr {
            server_identity,
            keypair_env_var,
        } => {
            // NIP-98 auth is header-based. For git smart-HTTP transport, the
            // credentials callback fires for HTTP Basic/Digest challenges.
            // Solid pod git endpoints that accept NIP-98 typically fall back
            // to bearer token auth via the Authorization header.
            //
            // The keypair is loaded from SERVER_NOSTR_PRIVKEY (server identity)
            // or the per-remote override env var. The 32-byte secret key is
            // hex-encoded in the env var.
            let key_var = if *server_identity {
                "SERVER_NOSTR_PRIVKEY".to_string()
            } else {
                keypair_env_var
                    .clone()
                    .unwrap_or_else(|| "SERVER_NOSTR_PRIVKEY".to_string())
            };

            match std::env::var(&key_var) {
                Ok(hex_key) if hex_key.len() == 64 => {
                    // For HTTP transport we use the hex privkey as a bearer token
                    // that the Solid pod's NIP-98 extractor will verify.
                    // git2's credential callback maps to HTTP Basic auth.
                    callbacks.credentials(move |_url, _username_from_url, _allowed_types| {
                        Cred::userpass_plaintext("nostr", &hex_key)
                    });
                }
                _ => {
                    warn!(
                        "write-back: {} not set or invalid; attempting unauthenticated push",
                        key_var
                    );
                }
            }
        }
    }

    let push_rejection: Arc<std::sync::Mutex<Option<String>>> =
        Arc::new(std::sync::Mutex::new(None));
    let rejection_flag = push_rejection.clone();

    callbacks.push_update_reference(move |refname, status| {
        if let Some(msg) = status {
            warn!("write-back: push rejected for {}: {}", refname, msg);
            if let Ok(mut guard) = rejection_flag.lock() {
                *guard = Some(format!("{}: {}", refname, msg));
            }
        }
        Ok(())
    });

    Ok((callbacks, push_rejection))
}

// ---------------------------------------------------------------------------
// Result
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteBackResult {
    pub commit_sha: String,
    pub remote_id: String,
    pub case_id: String,
    pub pushed_at: chrono::DateTime<chrono::Utc>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writeback_disabled_by_default() {
        std::env::remove_var(WRITEBACK_ENABLED_ENV);
        assert!(!writeback_enabled());
    }

    #[test]
    fn writeback_enabled_variants() {
        for val in &["1", "true", "TRUE", "yes", "on"] {
            std::env::set_var(WRITEBACK_ENABLED_ENV, val);
            assert!(writeback_enabled(), "expected enabled for '{}'", val);
        }
        std::env::remove_var(WRITEBACK_ENABLED_ENV);
    }

    #[test]
    fn enrichment_type_serialises() {
        let json = serde_json::to_string(&EnrichmentType::OntologyPromotion).unwrap();
        assert_eq!(json, "\"ontology_promotion\"");
    }

    #[test]
    fn enrichment_payload_round_trips() {
        let payload = EnrichmentPayload {
            enrichment_type: EnrichmentType::EmbeddingUpdate,
            target_path: "concepts/smart-contract.embeddings.json".to_string(),
            content: r#"{"vector": [0.1, 0.2]}"#.to_string(),
            commit_subject: "feat(embedding): update smart-contract vector".to_string(),
            commit_body: String::new(),
        };
        let json = serde_json::to_string(&payload).unwrap();
        let rt: EnrichmentPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.target_path, payload.target_path);
    }

    #[test]
    fn decision_report_round_trips() {
        let report = DecisionReport {
            case_id: "case-001".into(),
            decision: "approve".into(),
            proposed_by: "did:nostr:aaa".into(),
            approved_by: "did:nostr:bbb".into(),
            reasoning: "test reasoning".into(),
            server_did: "did:nostr:ccc".into(),
            entity_urn: "urn:visionclaw:concept:test:node".into(),
        };
        let json = serde_json::to_string(&report).unwrap();
        let rt: DecisionReport = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.case_id, "case-001");
    }

    #[test]
    fn writeback_result_round_trips() {
        let result = WriteBackResult {
            commit_sha: "abc123".into(),
            remote_id: "remote-1".into(),
            case_id: "case-1".into(),
            pushed_at: Utc::now(),
        };
        let json = serde_json::to_string(&result).unwrap();
        let rt: WriteBackResult = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.commit_sha, "abc123");
    }
}
