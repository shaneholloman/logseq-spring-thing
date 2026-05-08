//! Git Ingest Surface — PRD-013 Phase 1 (G1 + G2 + G3 + G4 scaffold).
//!
//! Replaces the GitHub REST API ingest with a git-over-HTTP pipeline that
//! treats every knowledge source identically: GitHub, GitLab, Solid pod, or
//! bare repo. Identity-mediated access via `did:nostr` + NIP-98 is layered on
//! top (Phase 2). Write-back through the Judgment Broker is scaffolded here
//! and completed in Phase 3.
//!
//! # Module layout
//!
//! | File                  | Component | PRD-013 ref |
//! |-----------------------|-----------|-------------|
//! | `mod.rs`              | GitIngestService (clone/fetch) | G1 |
//! | `remote_registry.rs`  | RemoteRegistry + REST handlers | G2 |
//! | `provenance.rs`       | ProvenanceTrailer encoder      | G3 |
//! | `writeback_saga.rs`   | WriteBackSaga scaffold         | G4 |

pub mod provenance;
pub mod remote_registry;
pub mod writeback_saga;

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use git2::{build::RepoBuilder, Cred, FetchOptions, RemoteCallbacks, Repository};
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use remote_registry::RemoteRegistry;

// ---------------------------------------------------------------------------
// Feature flag
// ---------------------------------------------------------------------------

/// Env-var gate for the git-ingest pipeline (default: disabled during rollout).
pub const GIT_INGEST_ENABLED_ENV: &str = "GIT_INGEST_ENABLED";

/// Root directory for local clones (default: `/app/data/git-ingest/`).
pub const GIT_INGEST_ROOT_ENV: &str = "GIT_INGEST_ROOT";

const DEFAULT_INGEST_ROOT: &str = "/app/data/git-ingest";

/// Returns `true` if git-ingest is enabled via `GIT_INGEST_ENABLED`.
pub fn git_ingest_enabled() -> bool {
    std::env::var(GIT_INGEST_ENABLED_ENV)
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}

/// Resolves the local clone storage root from the environment.
fn ingest_root() -> PathBuf {
    PathBuf::from(
        std::env::var(GIT_INGEST_ROOT_ENV).unwrap_or_else(|_| DEFAULT_INGEST_ROOT.to_string()),
    )
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Authentication strategy for a git remote.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RemoteAuth {
    /// No authentication (public repos, public pod paths).
    None,
    /// GitHub/GitLab personal access token (legacy compat).
    /// `token_env_var` names the env var holding the token — the token itself
    /// is never persisted to disk.
    Pat { token_env_var: String },
    /// `did:nostr` NIP-98 auth against a Solid pod (Phase 2).
    DidNostr {
        /// Use VisionClaw's `SERVER_NOSTR_PRIVKEY` identity.
        server_identity: bool,
        /// Optional: env var holding a per-remote keypair override.
        keypair_env_var: Option<String>,
    },
}

/// A configured knowledge source (git remote).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitRemote {
    /// Unique identifier (UUID).
    pub id: String,
    /// Git remote URL (HTTPS).
    pub url: String,
    /// Authentication strategy.
    pub auth: RemoteAuth,
    /// `did:nostr:<hex>` of the pod owner (if known).
    pub owner_did: Option<String>,
    /// Subdirectories to ingest (like `GITHUB_BASE_PATHS`).
    pub base_paths: Vec<String>,
    /// Branch to track (default: `"main"`).
    pub branch: String,
    /// Automatic sync interval in seconds. `0` = manual only.
    pub sync_interval_secs: u64,
    /// Whether write-back is enabled for this remote.
    pub writeback_enabled: bool,
    /// Last successful sync timestamp.
    pub last_sync: Option<DateTime<Utc>>,
    /// HEAD commit SHA after last successful fetch (for incremental sync).
    pub last_commit_sha: Option<String>,
}

impl GitRemote {
    /// Directory under the ingest root where this remote's clone lives.
    pub fn local_path(&self) -> PathBuf {
        ingest_root().join(&self.id)
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum GitIngestError {
    #[error("git operation failed: {0}")]
    Git(#[from] git2::Error),

    #[error("remote not found: {0}")]
    RemoteNotFound(String),

    #[error("local clone path is not a valid UTF-8 string")]
    InvalidPath,

    #[error("auth env var `{0}` not set")]
    MissingAuthEnvVar(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("git-ingest is disabled (set {GIT_INGEST_ENABLED_ENV}=true)")]
    Disabled,
}

// ---------------------------------------------------------------------------
// Sync result
// ---------------------------------------------------------------------------

/// Files changed during a sync operation, grouped for the parser pipeline.
#[derive(Debug, Clone, Default)]
pub struct SyncResult {
    /// Absolute paths of files that were added or modified.
    pub changed_files: Vec<PathBuf>,
    /// Absolute paths of files that were deleted.
    pub deleted_files: Vec<PathBuf>,
    /// The new HEAD commit SHA after fetch.
    pub head_sha: String,
}

// ---------------------------------------------------------------------------
// GitIngestService
// ---------------------------------------------------------------------------

/// Core git clone/fetch service (PRD-013 G1).
///
/// Holds no persistent state — all state is in the local worktree on disk and
/// the `GitRemote` registry entries in Neo4j. This struct is cheap to clone
/// and safe to share across `Arc`.
#[derive(Debug, Clone)]
pub struct GitIngestService;

impl GitIngestService {
    pub fn new() -> Self {
        Self
    }

    /// Clone or fetch a remote and return the list of changed file paths.
    ///
    /// If the local clone directory already exists, performs an incremental
    /// `git fetch` + diff. Otherwise, performs a full `git clone`.
    pub async fn sync_remote(&self, remote: &mut GitRemote) -> Result<SyncResult, GitIngestError> {
        if !git_ingest_enabled() {
            return Err(GitIngestError::Disabled);
        }

        let local_path = remote.local_path();
        let url = remote.url.clone();
        let branch = remote.branch.clone();
        let auth = remote.auth.clone();
        let last_sha = remote.last_commit_sha.clone();

        // git2 operations are blocking — run on a blocking thread.
        let result = tokio::task::spawn_blocking(move || {
            if local_path.exists() {
                fetch_and_diff(&local_path, &url, &branch, &auth, last_sha.as_deref())
            } else {
                clone_remote(&local_path, &url, &branch, &auth)
            }
        })
        .await
        .map_err(|e| GitIngestError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))??;

        // Update remote metadata.
        remote.last_sync = Some(Utc::now());
        remote.last_commit_sha = Some(result.head_sha.clone());

        info!(
            "git-ingest: synced remote {} ({}) — {} changed, {} deleted",
            remote.id,
            remote.url,
            result.changed_files.len(),
            result.deleted_files.len(),
        );

        Ok(result)
    }
}

// ---------------------------------------------------------------------------
// git2 helpers (blocking, run on spawn_blocking)
// ---------------------------------------------------------------------------

/// Build `RemoteCallbacks` with appropriate credentials for the auth type.
fn build_callbacks(auth: &RemoteAuth) -> Result<RemoteCallbacks<'_>, GitIngestError> {
    let mut callbacks = RemoteCallbacks::new();

    match auth {
        RemoteAuth::None => {
            // No credentials needed.
        }
        RemoteAuth::Pat { token_env_var } => {
            let token = std::env::var(token_env_var)
                .map_err(|_| GitIngestError::MissingAuthEnvVar(token_env_var.clone()))?;
            // For HTTPS PAT auth, git uses the token as the password with any
            // username (GitHub accepts "x-access-token", GitLab accepts "oauth2").
            let token_owned = token.clone();
            callbacks.credentials(move |_url, _username_from_url, _allowed_types| {
                Cred::userpass_plaintext("x-access-token", &token_owned)
            });
        }
        RemoteAuth::DidNostr { .. } => {
            // Phase 2: NIP-98 auth injection via custom HTTP headers.
            // For now, fall through to no auth (public pod paths only).
            warn!(
                "git-ingest: did:nostr auth not yet implemented; attempting unauthenticated clone"
            );
        }
    }

    Ok(callbacks)
}

/// Full clone of a remote into `local_path`.
fn clone_remote(
    local_path: &Path,
    url: &str,
    branch: &str,
    auth: &RemoteAuth,
) -> Result<SyncResult, GitIngestError> {
    info!(
        "git-ingest: cloning {} (branch: {}) → {:?}",
        url, branch, local_path
    );

    // Ensure parent directory exists.
    if let Some(parent) = local_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let callbacks = build_callbacks(auth)?;
    let mut fetch_opts = FetchOptions::new();
    fetch_opts.remote_callbacks(callbacks);

    let repo = RepoBuilder::new()
        .branch(branch)
        .fetch_options(fetch_opts)
        .clone(url, local_path)?;

    let head = repo.head()?;
    let head_sha = head.target().map(|oid| oid.to_string()).unwrap_or_default();

    // On initial clone, every file in the worktree is "changed".
    let changed_files = collect_worktree_files(local_path)?;

    Ok(SyncResult {
        changed_files,
        deleted_files: Vec::new(),
        head_sha,
    })
}

/// Incremental fetch + diff against the previously recorded commit SHA.
fn fetch_and_diff(
    local_path: &Path,
    url: &str,
    branch: &str,
    auth: &RemoteAuth,
    last_sha: Option<&str>,
) -> Result<SyncResult, GitIngestError> {
    info!(
        "git-ingest: fetching {} (branch: {}) in {:?}",
        url, branch, local_path
    );

    let repo = Repository::open(local_path)?;

    // Fetch from origin.
    let callbacks = build_callbacks(auth)?;
    let mut fetch_opts = FetchOptions::new();
    fetch_opts.remote_callbacks(callbacks);

    let mut origin = repo.find_remote("origin").or_else(|_| {
        // Remote might have been renamed; re-add it.
        repo.remote("origin", url)
    })?;

    let refspec = format!("refs/heads/{}:refs/remotes/origin/{}", branch, branch);
    origin.fetch(&[&refspec], Some(&mut fetch_opts), None)?;
    drop(origin);

    // Resolve the new HEAD for the tracked branch.
    let fetch_head = repo.find_reference(&format!("refs/remotes/origin/{}", branch))?;
    let new_oid = fetch_head
        .target()
        .ok_or_else(|| git2::Error::from_str("fetch head has no target"))?;
    let new_sha = new_oid.to_string();

    // Fast-forward the local branch to the fetched commit.
    let fetch_commit = repo.find_commit(new_oid)?;
    let local_branch_ref = format!("refs/heads/{}", branch);
    if repo.find_reference(&local_branch_ref).is_ok() {
        repo.reference(&local_branch_ref, new_oid, true, "git-ingest: fast-forward")?;
    } else {
        repo.branch(branch, &fetch_commit, true)?;
    }
    repo.set_head(&local_branch_ref)?;
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))?;

    // Diff old..new to find changed files.
    let (changed_files, deleted_files) = if let Some(old_sha_str) = last_sha {
        if old_sha_str == new_sha {
            debug!("git-ingest: no new commits since {}", old_sha_str);
            return Ok(SyncResult {
                changed_files: Vec::new(),
                deleted_files: Vec::new(),
                head_sha: new_sha,
            });
        }
        diff_trees(&repo, old_sha_str, &new_sha, local_path)?
    } else {
        // No previous SHA — treat everything as changed.
        (collect_worktree_files(local_path)?, Vec::new())
    };

    Ok(SyncResult {
        changed_files,
        deleted_files,
        head_sha: new_sha,
    })
}

/// Compute the set of changed and deleted file paths between two commits.
fn diff_trees(
    repo: &Repository,
    old_sha: &str,
    new_sha: &str,
    local_path: &Path,
) -> Result<(Vec<PathBuf>, Vec<PathBuf>), GitIngestError> {
    let old_oid = git2::Oid::from_str(old_sha)?;
    let new_oid = git2::Oid::from_str(new_sha)?;

    let old_commit = repo.find_commit(old_oid)?;
    let new_commit = repo.find_commit(new_oid)?;

    let old_tree = old_commit.tree()?;
    let new_tree = new_commit.tree()?;

    let diff = repo.diff_tree_to_tree(Some(&old_tree), Some(&new_tree), None)?;

    let mut changed = Vec::new();
    let mut deleted = Vec::new();

    diff.foreach(
        &mut |delta, _progress| {
            match delta.status() {
                git2::Delta::Deleted => {
                    if let Some(path) = delta.old_file().path() {
                        deleted.push(local_path.join(path));
                    }
                }
                _ => {
                    // Added, Modified, Renamed, Copied, etc. — all treated as "changed".
                    if let Some(path) = delta.new_file().path() {
                        changed.push(local_path.join(path));
                    }
                }
            }
            true
        },
        None,
        None,
        None,
    )?;

    Ok((changed, deleted))
}

/// Recursively collect all non-hidden files in a directory (for initial clone).
fn collect_worktree_files(root: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut files = Vec::new();
    collect_files_recursive(root, root, &mut files)?;
    Ok(files)
}

fn collect_files_recursive(
    root: &Path,
    dir: &Path,
    out: &mut Vec<PathBuf>,
) -> Result<(), std::io::Error> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Skip .git directory and hidden files.
        if name_str.starts_with('.') {
            continue;
        }

        if path.is_dir() {
            collect_files_recursive(root, &path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_local_path_uses_id() {
        let remote = GitRemote {
            id: "abc-123".to_string(),
            url: "https://github.com/user/repo.git".to_string(),
            auth: RemoteAuth::None,
            owner_did: None,
            base_paths: vec!["pages".to_string()],
            branch: "main".to_string(),
            sync_interval_secs: 0,
            writeback_enabled: false,
            last_sync: None,
            last_commit_sha: None,
        };
        let path = remote.local_path();
        assert!(path.ends_with("abc-123"));
    }

    #[test]
    fn remote_auth_pat_serialises() {
        let auth = RemoteAuth::Pat {
            token_env_var: "GITHUB_TOKEN".to_string(),
        };
        let json = serde_json::to_string(&auth).unwrap();
        assert!(json.contains("\"type\":\"pat\""));
        assert!(json.contains("GITHUB_TOKEN"));
    }

    #[test]
    fn remote_auth_did_nostr_serialises() {
        let auth = RemoteAuth::DidNostr {
            server_identity: true,
            keypair_env_var: None,
        };
        let json = serde_json::to_string(&auth).unwrap();
        assert!(json.contains("\"type\":\"did_nostr\""));
    }

    #[test]
    fn feature_flag_default_off() {
        // Unless the env var is set, git-ingest should be disabled.
        std::env::remove_var(GIT_INGEST_ENABLED_ENV);
        assert!(!git_ingest_enabled());
    }

    #[test]
    fn ingest_root_default() {
        std::env::remove_var(GIT_INGEST_ROOT_ENV);
        assert_eq!(ingest_root(), PathBuf::from(DEFAULT_INGEST_ROOT));
    }
}
