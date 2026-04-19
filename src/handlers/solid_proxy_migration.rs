//! ADR-052 Pod default-private migration.
//!
//! Scans each existing Pod in the configured JSS storage root and, when the
//! Pod's root `.acl` still carries a legacy `foaf:Agent` read grant, rewrites
//! it to the owner-only template from ADR-052. Idempotent: a Pod whose root
//! ACL has no `<#publicRead>` marker and no `agentClass foaf:Agent` clause is
//! treated as already migrated and skipped.
//!
//! Controlled by two env vars:
//! - `POD_DEFAULT_PRIVATE` — if not `true`, the migration is a no-op.
//! - `POD_STORAGE_ROOT` — filesystem path where the JSS stores Pod
//!   directories (one top-level directory per `npub`). Default:
//!   `/var/lib/jss/pods`.
//!
//! The migration writes only to the root `.acl` of each Pod. It does NOT
//! create the `./private`, `./public`, `./shared`, `./profile` containers;
//! those are created when the user next authenticates and the provisioning
//! flow detects missing sub-containers.

use log::{debug, info, warn};
use std::path::{Path, PathBuf};

use super::solid_proxy_handler::{derive_webid, render_owner_only_acl};

/// Outcome of a migration pass.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MigrationReport {
    /// Number of Pods inspected.
    pub scanned: usize,
    /// Number of Pods whose root ACL was rewritten.
    pub migrated: usize,
    /// Number of Pods already in the sovereign layout (no-op).
    pub skipped_already_private: usize,
    /// Number of Pods with no root ACL to migrate.
    pub skipped_no_acl: usize,
    /// Number of Pods where we failed to read/write the ACL.
    pub errors: usize,
}

/// Resolve the JSS storage root from env with a safe default.
pub fn pod_storage_root() -> PathBuf {
    PathBuf::from(
        std::env::var("POD_STORAGE_ROOT")
            .unwrap_or_else(|_| "/var/lib/jss/pods".to_string()),
    )
}

/// True when the ACL contents are already sovereign (no public read grant).
/// An ACL is sovereign when it contains NEITHER a `<#publicRead>` rule NOR a
/// bare `agentClass foaf:Agent` clause. The check is lexical and intentionally
/// tolerant of whitespace / prefix variations.
pub fn acl_is_sovereign(acl_contents: &str) -> bool {
    let lc = acl_contents.to_ascii_lowercase();
    let has_public_read = lc.contains("<#publicread>");
    let has_foaf_agent_class = lc.contains("agentclass foaf:agent")
        || lc.contains("agentclass <http://xmlns.com/foaf/0.1/agent>");
    !has_public_read && !has_foaf_agent_class
}

/// Migrate a single Pod root ACL in-place on disk. The Pod is identified by
/// its directory name (treated as the `npub` / pubkey for WebID derivation).
///
/// Returns `Ok(true)` if the ACL was rewritten, `Ok(false)` if it was already
/// sovereign or missing. Errors propagate only for I/O failures.
pub fn migrate_pod_acl(pod_dir: &Path) -> std::io::Result<MigrateOutcome> {
    let acl_path = pod_dir.join(".acl");
    if !acl_path.is_file() {
        return Ok(MigrateOutcome::NoAcl);
    }
    let existing = std::fs::read_to_string(&acl_path)?;
    if acl_is_sovereign(&existing) {
        return Ok(MigrateOutcome::AlreadySovereign);
    }

    let pubkey = pod_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_string();
    if pubkey.is_empty() {
        return Ok(MigrateOutcome::NoAcl);
    }
    let webid = derive_webid(&pubkey);
    let new_body = render_owner_only_acl(&webid);
    std::fs::write(&acl_path, new_body)?;
    Ok(MigrateOutcome::Migrated)
}

/// Per-Pod result of the migration loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrateOutcome {
    Migrated,
    AlreadySovereign,
    NoAcl,
}

/// Run the migration over every Pod directory under `root`. Idempotent —
/// running twice produces `migrated=0` on the second pass.
pub fn run_migration(root: &Path) -> MigrationReport {
    let mut report = MigrationReport::default();
    let entries = match std::fs::read_dir(root) {
        Ok(it) => it,
        Err(e) => {
            warn!(
                "ADR-052 migration: cannot read POD_STORAGE_ROOT {:?}: {}",
                root, e
            );
            return report;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        report.scanned += 1;
        match migrate_pod_acl(&path) {
            Ok(MigrateOutcome::Migrated) => {
                report.migrated += 1;
                debug!("ADR-052 migration: rewrote {:?}", path);
            }
            Ok(MigrateOutcome::AlreadySovereign) => {
                report.skipped_already_private += 1;
            }
            Ok(MigrateOutcome::NoAcl) => {
                report.skipped_no_acl += 1;
            }
            Err(e) => {
                report.errors += 1;
                warn!("ADR-052 migration: failed on {:?}: {}", path, e);
            }
        }
    }

    info!(
        "ADR-052 migration complete: scanned={} migrated={} already_private={} no_acl={} errors={}",
        report.scanned,
        report.migrated,
        report.skipped_already_private,
        report.skipped_no_acl,
        report.errors,
    );
    report
}

/// Startup entrypoint. Returns a migration report. Does nothing when the
/// feature flag is off or the storage root is absent.
pub fn run_startup_migration() -> MigrationReport {
    if !super::solid_proxy_handler::pod_default_private_enabled() {
        debug!("ADR-052 migration skipped: POD_DEFAULT_PRIVATE is not enabled");
        return MigrationReport::default();
    }
    let root = pod_storage_root();
    if !root.is_dir() {
        debug!(
            "ADR-052 migration skipped: POD_STORAGE_ROOT {:?} does not exist",
            root
        );
        return MigrationReport::default();
    }
    run_migration(&root)
}
