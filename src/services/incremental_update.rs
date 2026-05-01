// src/services/incremental_update.rs
//! Incremental Update Pipeline
//!
//! Enables efficient re-sync without full graph recomputation by tracking
//! per-file SHA-256 content hashes and producing minimal change sets.
//!
//! The existing GitHub sync path (`github_sync_service.rs`) uses Git SHA1
//! blob hashes stored in Neo4j `FileMetadata` nodes, with `FORCE_FULL_SYNC=1`
//! as the escape hatch. This module operates on content hashes (SHA-256)
//! computed locally, decoupling change detection from the GitHub API and
//! enabling any content source to participate in incremental sync.
//!
//! Content hashing follows the project convention established in `src/uri/`
//! (the `sha256-12-` prefix), but uses the full 64-char hex digest for
//! collision resistance across large file sets.

use log::{debug, info};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fmt;
use std::time::Instant;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// The set of changes detected between the stored state and current files.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChangeSet {
    /// File paths that are new (not in the stored hash map).
    pub added: Vec<String>,
    /// File paths whose content hash differs from the stored value.
    pub modified: Vec<String>,
    /// File paths present in the stored map but absent from the current set.
    pub removed: Vec<String>,
    /// Count of files whose content hash matches the stored value.
    pub unchanged: usize,
}

impl ChangeSet {
    /// True when no mutations are needed.
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.modified.is_empty() && self.removed.is_empty()
    }

    /// Total number of files that require processing.
    pub fn actionable_count(&self) -> usize {
        self.added.len() + self.modified.len() + self.removed.len()
    }
}

impl fmt::Display for ChangeSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ChangeSet {{ added: {}, modified: {}, removed: {}, unchanged: {} }}",
            self.added.len(),
            self.modified.len(),
            self.removed.len(),
            self.unchanged
        )
    }
}

/// Result of applying a `ChangeSet` to the graph.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct IncrementalResult {
    pub nodes_added: usize,
    pub nodes_removed: usize,
    pub edges_updated: usize,
    pub duration_ms: u64,
}

impl fmt::Display for IncrementalResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "IncrementalResult {{ +{} nodes, -{} nodes, ~{} edges, {}ms }}",
            self.nodes_added, self.nodes_removed, self.edges_updated, self.duration_ms
        )
    }
}

// ---------------------------------------------------------------------------
// Service
// ---------------------------------------------------------------------------

/// Tracks per-file content hashes and produces minimal change sets for
/// incremental graph synchronisation.
#[derive(Debug, Clone)]
pub struct IncrementalUpdateService {
    /// Maps `file_path` to its SHA-256 hex digest (64 chars).
    content_hashes: HashMap<String, String>,
    /// Timestamp of the last successful commit.
    last_sync_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl Default for IncrementalUpdateService {
    fn default() -> Self {
        Self::new()
    }
}

impl IncrementalUpdateService {
    /// Create an empty service. The first `detect_changes` call will treat
    /// every file as `added`.
    pub fn new() -> Self {
        Self {
            content_hashes: HashMap::new(),
            last_sync_at: None,
        }
    }

    /// Create a service pre-loaded with known hashes (e.g. restored from
    /// persistent storage).
    pub fn with_hashes(hashes: HashMap<String, String>) -> Self {
        Self {
            content_hashes: hashes,
            last_sync_at: None,
        }
    }

    /// Number of files currently tracked.
    pub fn tracked_count(&self) -> usize {
        self.content_hashes.len()
    }

    /// Timestamp of the last successful `commit`.
    pub fn last_sync_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.last_sync_at
    }

    // -----------------------------------------------------------------------
    // Core pipeline
    // -----------------------------------------------------------------------

    /// Compare `current_files` against stored hashes and return a `ChangeSet`.
    ///
    /// Each entry in `current_files` is `(file_path, file_content)`. The
    /// content is hashed with SHA-256 on the fly; callers do not need to
    /// pre-compute digests.
    pub fn detect_changes(&self, current_files: &[(String, String)]) -> ChangeSet {
        let mut changeset = ChangeSet::default();

        // Build a set of current paths for removal detection.
        let mut current_paths: HashMap<&str, &str> = HashMap::with_capacity(current_files.len());

        for (path, content) in current_files {
            let hash = Self::hash_content(content);
            current_paths.insert(path.as_str(), content.as_str());

            match self.content_hashes.get(path.as_str()) {
                None => {
                    changeset.added.push(path.clone());
                }
                Some(stored_hash) if *stored_hash != hash => {
                    changeset.modified.push(path.clone());
                }
                Some(_) => {
                    changeset.unchanged += 1;
                }
            }
        }

        // Files in stored map but absent from current set are removals.
        for stored_path in self.content_hashes.keys() {
            if !current_paths.contains_key(stored_path.as_str()) {
                changeset.removed.push(stored_path.clone());
            }
        }

        // Deterministic ordering for testability.
        changeset.added.sort();
        changeset.modified.sort();
        changeset.removed.sort();

        debug!("IncrementalUpdateService::detect_changes: {}", changeset);

        changeset
    }

    /// Generate the minimal set of graph mutations implied by a `ChangeSet`.
    ///
    /// This is a planning step: it computes counts of node/edge operations
    /// that the caller (typically the sync service or graph state actor) will
    /// execute. The actual database writes are the caller's responsibility
    /// so that this module stays free of persistence dependencies.
    ///
    /// Heuristic:
    /// - Each added file produces 1 node add + 2 edge updates (average fan-out).
    /// - Each modified file produces 0 node add/remove + 3 edge updates (re-link).
    /// - Each removed file produces 1 node remove + 2 edge updates (cleanup).
    pub fn apply_changeset(&self, changeset: &ChangeSet) -> IncrementalResult {
        let start = Instant::now();

        let nodes_added = changeset.added.len();
        let nodes_removed = changeset.removed.len();
        let edges_updated =
            changeset.added.len() * 2 + changeset.modified.len() * 3 + changeset.removed.len() * 2;

        let duration_ms = start.elapsed().as_millis() as u64;

        let result = IncrementalResult {
            nodes_added,
            nodes_removed,
            edges_updated,
            duration_ms,
        };

        info!("IncrementalUpdateService::apply_changeset: {}", result);
        result
    }

    /// Commit a `ChangeSet` by updating the stored content hashes.
    ///
    /// Call this *after* the graph mutations have been persisted successfully.
    /// Re-hashes content for added/modified files from the original input.
    /// Removes entries for deleted files.
    pub fn commit(&mut self, changeset: &ChangeSet, current_files: &[(String, String)]) {
        // Build a lookup from the current file set.
        let file_map: HashMap<&str, &str> = current_files
            .iter()
            .map(|(p, c)| (p.as_str(), c.as_str()))
            .collect();

        // Upsert added + modified.
        for path in changeset.added.iter().chain(changeset.modified.iter()) {
            if let Some(content) = file_map.get(path.as_str()) {
                let hash = Self::hash_content(content);
                self.content_hashes.insert(path.clone(), hash);
            }
        }

        // Purge removed.
        for path in &changeset.removed {
            self.content_hashes.remove(path);
        }

        self.last_sync_at = Some(chrono::Utc::now());

        info!(
            "IncrementalUpdateService::commit: tracked={}, last_sync_at={:?}",
            self.content_hashes.len(),
            self.last_sync_at
        );
    }

    /// Clear all stored hashes, forcing the next `detect_changes` to treat
    /// every file as new. Equivalent to `FORCE_FULL_SYNC=1` in the GitHub
    /// sync path.
    pub fn full_reset(&mut self) {
        self.content_hashes.clear();
        self.last_sync_at = None;
        info!("IncrementalUpdateService::full_reset: all hashes cleared");
    }

    // -----------------------------------------------------------------------
    // Hashing
    // -----------------------------------------------------------------------

    /// Compute the full SHA-256 hex digest (64 lowercase hex chars) of the
    /// given content string.
    fn hash_content(content: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        let digest = hasher.finalize();
        // Full 32-byte digest → 64 hex chars (lowercase).
        digest.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build a `(path, content)` pair.
    fn file(path: &str, content: &str) -> (String, String) {
        (path.to_string(), content.to_string())
    }

    // -- Construction -------------------------------------------------------

    #[test]
    fn new_service_has_no_tracked_files() {
        let svc = IncrementalUpdateService::new();
        assert_eq!(svc.tracked_count(), 0);
        assert!(svc.last_sync_at().is_none());
    }

    #[test]
    fn with_hashes_preloads_state() {
        let mut hashes = HashMap::new();
        hashes.insert("a.md".to_string(), "abc123".to_string());
        let svc = IncrementalUpdateService::with_hashes(hashes);
        assert_eq!(svc.tracked_count(), 1);
    }

    // -- detect_changes: empty initial state (all added) --------------------

    #[test]
    fn empty_state_marks_all_files_as_added() {
        let svc = IncrementalUpdateService::new();
        let files = vec![
            file("a.md", "alpha"),
            file("b.md", "beta"),
            file("c.md", "gamma"),
        ];
        let cs = svc.detect_changes(&files);

        assert_eq!(cs.added, vec!["a.md", "b.md", "c.md"]);
        assert!(cs.modified.is_empty());
        assert!(cs.removed.is_empty());
        assert_eq!(cs.unchanged, 0);
        assert_eq!(cs.actionable_count(), 3);
        assert!(!cs.is_empty());
    }

    // -- detect_changes: no changes (all unchanged) -------------------------

    #[test]
    fn no_changes_detected_when_content_identical() {
        let mut svc = IncrementalUpdateService::new();
        let files = vec![file("a.md", "alpha"), file("b.md", "beta")];

        // Seed the stored hashes by running a full cycle.
        let cs = svc.detect_changes(&files);
        svc.commit(&cs, &files);

        // Same files, same content → all unchanged.
        let cs2 = svc.detect_changes(&files);
        assert!(cs2.added.is_empty());
        assert!(cs2.modified.is_empty());
        assert!(cs2.removed.is_empty());
        assert_eq!(cs2.unchanged, 2);
        assert!(cs2.is_empty());
        assert_eq!(cs2.actionable_count(), 0);
    }

    // -- detect_changes: mixed changes --------------------------------------

    #[test]
    fn mixed_changes_detected_correctly() {
        let mut svc = IncrementalUpdateService::new();
        let initial = vec![
            file("keep.md", "stable"),
            file("change.md", "original"),
            file("remove.md", "doomed"),
        ];

        let cs = svc.detect_changes(&initial);
        svc.commit(&cs, &initial);
        assert_eq!(svc.tracked_count(), 3);

        // Second round: keep.md unchanged, change.md modified, remove.md
        // absent, new.md added.
        let updated = vec![
            file("keep.md", "stable"),
            file("change.md", "mutated"),
            file("new.md", "fresh"),
        ];

        let cs2 = svc.detect_changes(&updated);
        assert_eq!(cs2.added, vec!["new.md"]);
        assert_eq!(cs2.modified, vec!["change.md"]);
        assert_eq!(cs2.removed, vec!["remove.md"]);
        assert_eq!(cs2.unchanged, 1);
        assert_eq!(cs2.actionable_count(), 3);
    }

    // -- detect_changes: removal detection ----------------------------------

    #[test]
    fn removal_detected_when_file_disappears() {
        let mut svc = IncrementalUpdateService::new();
        let initial = vec![file("a.md", "A"), file("b.md", "B"), file("c.md", "C")];
        let cs = svc.detect_changes(&initial);
        svc.commit(&cs, &initial);

        // Only a.md remains.
        let reduced = vec![file("a.md", "A")];
        let cs2 = svc.detect_changes(&reduced);

        assert!(cs2.added.is_empty());
        assert!(cs2.modified.is_empty());
        assert_eq!(cs2.removed, vec!["b.md", "c.md"]);
        assert_eq!(cs2.unchanged, 1);
    }

    // -- full_reset ---------------------------------------------------------

    #[test]
    fn full_reset_clears_state() {
        let mut svc = IncrementalUpdateService::new();
        let files = vec![file("a.md", "content")];
        let cs = svc.detect_changes(&files);
        svc.commit(&cs, &files);
        assert_eq!(svc.tracked_count(), 1);
        assert!(svc.last_sync_at().is_some());

        svc.full_reset();

        assert_eq!(svc.tracked_count(), 0);
        assert!(svc.last_sync_at().is_none());

        // After reset, same files should appear as added again.
        let cs2 = svc.detect_changes(&files);
        assert_eq!(cs2.added, vec!["a.md"]);
        assert!(cs2.modified.is_empty());
        assert!(cs2.removed.is_empty());
        assert_eq!(cs2.unchanged, 0);
    }

    // -- apply_changeset heuristic ------------------------------------------

    #[test]
    fn apply_changeset_computes_expected_counts() {
        let svc = IncrementalUpdateService::new();
        let cs = ChangeSet {
            added: vec!["a.md".into(), "b.md".into()],
            modified: vec!["c.md".into()],
            removed: vec!["d.md".into()],
            unchanged: 5,
        };
        let result = svc.apply_changeset(&cs);

        assert_eq!(result.nodes_added, 2);
        assert_eq!(result.nodes_removed, 1);
        // edges: 2*2 + 1*3 + 1*2 = 9
        assert_eq!(result.edges_updated, 9);
    }

    #[test]
    fn apply_changeset_empty_changeset() {
        let svc = IncrementalUpdateService::new();
        let cs = ChangeSet::default();
        let result = svc.apply_changeset(&cs);

        assert_eq!(result.nodes_added, 0);
        assert_eq!(result.nodes_removed, 0);
        assert_eq!(result.edges_updated, 0);
    }

    // -- commit updates stored hashes correctly -----------------------------

    #[test]
    fn commit_updates_hashes_and_timestamp() {
        let mut svc = IncrementalUpdateService::new();
        assert!(svc.last_sync_at().is_none());

        let files = vec![file("x.md", "data")];
        let cs = svc.detect_changes(&files);
        svc.commit(&cs, &files);

        assert_eq!(svc.tracked_count(), 1);
        assert!(svc.last_sync_at().is_some());
    }

    #[test]
    fn commit_removes_deleted_files_from_tracking() {
        let mut svc = IncrementalUpdateService::new();

        let initial = vec![file("a.md", "A"), file("b.md", "B")];
        let cs1 = svc.detect_changes(&initial);
        svc.commit(&cs1, &initial);
        assert_eq!(svc.tracked_count(), 2);

        // Remove b.md.
        let after = vec![file("a.md", "A")];
        let cs2 = svc.detect_changes(&after);
        assert_eq!(cs2.removed, vec!["b.md"]);

        svc.commit(&cs2, &after);
        assert_eq!(svc.tracked_count(), 1);

        // b.md should now appear as added if re-introduced.
        let reintroduced = vec![file("a.md", "A"), file("b.md", "B-new")];
        let cs3 = svc.detect_changes(&reintroduced);
        assert_eq!(cs3.added, vec!["b.md"]);
    }

    // -- hash_content determinism -------------------------------------------

    #[test]
    fn hash_content_is_deterministic() {
        let h1 = IncrementalUpdateService::hash_content("hello world");
        let h2 = IncrementalUpdateService::hash_content("hello world");
        assert_eq!(h1, h2);
        // SHA-256 of "hello world" is well-known.
        assert_eq!(
            h1,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
        assert_eq!(h1.len(), 64);
    }

    #[test]
    fn hash_content_differs_for_different_input() {
        let h1 = IncrementalUpdateService::hash_content("alpha");
        let h2 = IncrementalUpdateService::hash_content("beta");
        assert_ne!(h1, h2);
    }

    // -- Display impls ------------------------------------------------------

    #[test]
    fn changeset_display_format() {
        let cs = ChangeSet {
            added: vec!["a".into()],
            modified: vec!["b".into(), "c".into()],
            removed: vec![],
            unchanged: 10,
        };
        let s = format!("{}", cs);
        assert!(s.contains("added: 1"));
        assert!(s.contains("modified: 2"));
        assert!(s.contains("removed: 0"));
        assert!(s.contains("unchanged: 10"));
    }

    #[test]
    fn incremental_result_display_format() {
        let r = IncrementalResult {
            nodes_added: 5,
            nodes_removed: 2,
            edges_updated: 12,
            duration_ms: 42,
        };
        let s = format!("{}", r);
        assert!(s.contains("+5 nodes"));
        assert!(s.contains("-2 nodes"));
        assert!(s.contains("~12 edges"));
        assert!(s.contains("42ms"));
    }

    // -- with_hashes round-trip ---------------------------------------------

    #[test]
    fn preloaded_hashes_detect_unchanged_correctly() {
        // Manually compute the hash for "content-a".
        let hash_a = IncrementalUpdateService::hash_content("content-a");

        let mut hashes = HashMap::new();
        hashes.insert("file-a.md".to_string(), hash_a);

        let svc = IncrementalUpdateService::with_hashes(hashes);

        let files = vec![file("file-a.md", "content-a")];
        let cs = svc.detect_changes(&files);

        assert!(cs.added.is_empty());
        assert!(cs.modified.is_empty());
        assert!(cs.removed.is_empty());
        assert_eq!(cs.unchanged, 1);
    }

    // -- empty file list ----------------------------------------------------

    #[test]
    fn empty_current_files_marks_all_stored_as_removed() {
        let mut svc = IncrementalUpdateService::new();
        let initial = vec![file("a.md", "A"), file("b.md", "B")];
        let cs = svc.detect_changes(&initial);
        svc.commit(&cs, &initial);

        let cs2 = svc.detect_changes(&[]);
        assert!(cs2.added.is_empty());
        assert!(cs2.modified.is_empty());
        assert_eq!(cs2.removed, vec!["a.md", "b.md"]);
        assert_eq!(cs2.unchanged, 0);
    }

    // -- default trait ------------------------------------------------------

    #[test]
    fn default_creates_empty_service() {
        let svc = IncrementalUpdateService::default();
        assert_eq!(svc.tracked_count(), 0);
        assert!(svc.last_sync_at().is_none());
    }
}
