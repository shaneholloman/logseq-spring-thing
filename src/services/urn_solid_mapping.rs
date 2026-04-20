//! URN-Solid vocabulary mapping loader (ADR-054 §1).
//!
//! Parses the curated markdown table at `docs/reference/urn-solid-mapping.md`
//! into an `Arc<RwLock<HashMap<String, UrnSolidMapping>>>` keyed by our vocabulary
//! IRI (e.g. `bc:Person` → `urn:solid:Person`).
//!
//! Features:
//!   * One-shot parse on startup via [`UrnSolidMapper::from_file`].
//!   * Optional hot-reload: spawn [`UrnSolidMapper::spawn_watcher`] to
//!     re-parse the file whenever `notify` reports a `Modify` event. Keeps
//!     the running server in sync with manual refreshes of the mapping
//!     table without a restart.
//!   * Pure-data [`UrnSolidMapper::from_markdown`] helper for tests.
//!
//! The parser is deliberately permissive: it reads every line that starts
//! with `|`, splits on `|` into cells, drops rows whose second cell doesn't
//! match the `urn:solid:` prefix (covers the header and the `|---|---|---|---|`
//! separator), and strips backticks/whitespace from every cell.
//!
//! Gated downstream by the `URN_SOLID_ALIGNMENT` env flag (see
//! [`urn_solid_alignment_enabled`]). This module itself has no side-effects
//! other than file I/O and in-memory state; the gate lives at the call sites.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use log::{debug, info, warn};

/// Env-var feature flag for the whole ADR-054 surface.
pub const URN_SOLID_ALIGNMENT_ENV: &str = "URN_SOLID_ALIGNMENT";

/// Default in-repo location of the mapping table.
pub const DEFAULT_MAPPING_PATH: &str = "docs/reference/urn-solid-mapping.md";

/// Returns `true` when `URN_SOLID_ALIGNMENT` is set to a truthy value.
/// Defaults to `false` so every behaviour gated on this flag is safe-off.
pub fn urn_solid_alignment_enabled() -> bool {
    std::env::var(URN_SOLID_ALIGNMENT_ENV)
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}

/// Lifecycle status for a mapping row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MappingStatus {
    /// Ready for production emission.
    Stable,
    /// Listed but not yet emitted; pending community review.
    Proposed,
    /// Intentionally not mapped — reason recorded inline in the markdown.
    Deferred,
}

impl MappingStatus {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "stable" => Some(MappingStatus::Stable),
            "proposed" => Some(MappingStatus::Proposed),
            "deferred" => Some(MappingStatus::Deferred),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            MappingStatus::Stable => "stable",
            MappingStatus::Proposed => "proposed",
            MappingStatus::Deferred => "deferred",
        }
    }
}

/// A single IRI ↔ URN-Solid binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UrnSolidMapping {
    /// Our vocabulary IRI (e.g. `bc:Person`).
    pub our_iri: String,
    /// URN-Solid canonical term (e.g. `urn:solid:Person`).
    pub urn_solid: String,
    /// Canonical upstream vocabulary (e.g. `foaf:Person`).
    pub canonical_vocab: String,
    /// Lifecycle status.
    pub status: MappingStatus,
}

/// Hot-reloadable map of IRI → URN-Solid mapping.
///
/// Cheap to clone — the inner map is an `Arc<RwLock<HashMap>>`, so clones
/// share the same backing state.
#[derive(Clone)]
pub struct UrnSolidMapper {
    map: Arc<RwLock<HashMap<String, UrnSolidMapping>>>,
    path: Option<Arc<PathBuf>>,
}

impl UrnSolidMapper {
    /// Build a mapper from an in-memory markdown string. No filesystem I/O.
    pub fn from_markdown(markdown: &str) -> Self {
        let map = parse_markdown(markdown);
        Self {
            map: Arc::new(RwLock::new(map)),
            path: None,
        }
    }

    /// Build a mapper by reading the markdown table at `path`. Returns an
    /// error only if the file cannot be read; malformed rows are skipped with
    /// a warning.
    pub fn from_file(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("read {}: {}", path.display(), e))?;
        let map = parse_markdown(&text);
        info!(
            "[urn-solid] Loaded {} mappings from {}",
            map.len(),
            path.display()
        );
        Ok(Self {
            map: Arc::new(RwLock::new(map)),
            path: Some(Arc::new(path.to_path_buf())),
        })
    }

    /// Build a mapper from the default repo-root path. Convenience for the
    /// app wiring layer.
    pub fn from_default_path() -> Result<Self, String> {
        Self::from_file(Path::new(DEFAULT_MAPPING_PATH))
    }

    /// Empty mapper — returns `None` for every lookup. Useful when the flag
    /// is off or the file is missing and the caller wants to continue
    /// operating in a degraded mode.
    pub fn empty() -> Self {
        Self {
            map: Arc::new(RwLock::new(HashMap::new())),
            path: None,
        }
    }

    /// Look up a mapping by our IRI. Returns `None` if the IRI is unmapped.
    pub fn lookup(&self, iri: &str) -> Option<UrnSolidMapping> {
        let guard = self.map.read().ok()?;
        guard.get(iri).cloned()
    }

    /// Return every mapping with the given status.
    pub fn all_with_status(&self, status: MappingStatus) -> Vec<UrnSolidMapping> {
        let guard = match self.map.read() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        guard
            .values()
            .filter(|m| m.status == status)
            .cloned()
            .collect()
    }

    /// Return the total number of mappings (across all statuses).
    pub fn len(&self) -> usize {
        self.map.read().map(|g| g.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Force-refresh the in-memory map from the file the mapper was built
    /// with. Does nothing for mappers built from in-memory markdown.
    pub fn reload(&self) -> Result<usize, String> {
        let path = match self.path.as_ref() {
            Some(p) => p.clone(),
            None => return Ok(self.len()),
        };
        let text = std::fs::read_to_string(&*path)
            .map_err(|e| format!("reload {}: {}", path.display(), e))?;
        let new_map = parse_markdown(&text);
        let n = new_map.len();
        let mut guard = self
            .map
            .write()
            .map_err(|e| format!("mapper write lock poisoned: {}", e))?;
        *guard = new_map;
        info!("[urn-solid] Hot-reloaded {} mappings from {}", n, path.display());
        Ok(n)
    }

    /// Spawn a background watcher that reloads the mapping table whenever
    /// the file is modified. Returns a handle to the watcher — drop it to
    /// stop watching.
    ///
    /// Noop (returns `Ok(None)`) when the mapper has no backing path.
    pub fn spawn_watcher(&self) -> Result<Option<MappingWatcher>, String> {
        use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

        let path = match self.path.as_ref() {
            Some(p) => p.clone(),
            None => return Ok(None),
        };
        let this = self.clone();

        let (tx, rx) = std::sync::mpsc::channel();

        let mut watcher: RecommendedWatcher = notify::recommended_watcher(
            move |res: notify::Result<notify::Event>| {
                // Forward events to the mpsc receiver.
                if let Err(e) = tx.send(res) {
                    warn!("[urn-solid] watcher channel closed: {}", e);
                }
            },
        )
        .map_err(|e| format!("notify watcher: {}", e))?;

        watcher
            .watch(&*path, RecursiveMode::NonRecursive)
            .map_err(|e| format!("watch {}: {}", path.display(), e))?;

        let handle = std::thread::Builder::new()
            .name("urn-solid-mapping-watcher".into())
            .spawn(move || {
                while let Ok(res) = rx.recv() {
                    match res {
                        Ok(ev)
                            if matches!(
                                ev.kind,
                                EventKind::Modify(_) | EventKind::Create(_)
                            ) =>
                        {
                            if let Err(e) = this.reload() {
                                warn!("[urn-solid] reload failed: {}", e);
                            }
                        }
                        Ok(_) => { /* ignore remove/access */ }
                        Err(e) => debug!("[urn-solid] watcher event error: {}", e),
                    }
                }
            })
            .map_err(|e| format!("spawn watcher thread: {}", e))?;

        Ok(Some(MappingWatcher {
            _watcher: watcher,
            _handle: handle,
        }))
    }
}

/// RAII handle for the file watcher. Drop it to stop watching.
pub struct MappingWatcher {
    _watcher: notify::RecommendedWatcher,
    _handle: std::thread::JoinHandle<()>,
}

/// Parse the markdown table into a keyed map. Malformed rows are skipped.
fn parse_markdown(text: &str) -> HashMap<String, UrnSolidMapping> {
    let mut out = HashMap::new();

    for line in text.lines() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with('|') || trimmed.starts_with("<!--") {
            continue;
        }

        let cells: Vec<String> = trimmed
            .trim_start_matches('|')
            .trim_end_matches('|')
            .split('|')
            .map(|s| s.trim().trim_matches('`').trim().to_string())
            .collect();

        if cells.len() < 4 {
            continue;
        }

        // Skip header/separator rows — the separator row's cells are `---`,
        // `---:`, or similar; the header's second cell lacks `urn:solid:`.
        let our_iri = &cells[0];
        let urn_solid = &cells[1];
        let canonical_vocab = &cells[2];
        let status_raw = &cells[3];

        if our_iri.is_empty()
            || our_iri.chars().all(|c| c == '-' || c == ':')
            || !urn_solid.starts_with("urn:solid:")
        {
            continue;
        }

        let status = match MappingStatus::from_str(status_raw) {
            Some(s) => s,
            None => {
                debug!(
                    "[urn-solid] skipping row {} (unknown status '{}')",
                    our_iri, status_raw
                );
                continue;
            }
        };

        // Strip leading/trailing backticks that survived split.
        let our_iri = our_iri.trim_matches('`').to_string();
        let urn_solid = urn_solid.trim_matches('`').to_string();
        let canonical_vocab = canonical_vocab.trim_matches('`').to_string();

        // Later rows for the same key override earlier ones (keeps behaviour
        // predictable if the table has accidental dupes).
        out.insert(
            our_iri.clone(),
            UrnSolidMapping {
                our_iri,
                urn_solid,
                canonical_vocab,
                status,
            },
        );
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
# Heading that must be ignored

| Our class (IRI) | `urn:solid:<Name>` | Canonical vocab | Status |
|-----------------|--------------------|-----------------|--------|
| `bc:Person` | `urn:solid:Person` | `foaf:Person` | stable |
| `bc:Document` | `urn:solid:Document` | `schema:CreativeWork` | stable |
| `mv:Brain` | `urn:solid:CognitiveAgent` | `prov:Agent` | proposed |
| `junk row` | `not-urn` | x | stable |
| `bc:Placeholder` | `urn:solid:Placeholder` | `schema:Thing` | bogus-status |
"#;

    #[test]
    fn parses_valid_rows_and_skips_header_separator_and_malformed() {
        let mapper = UrnSolidMapper::from_markdown(SAMPLE);
        // 3 valid (bc:Person, bc:Document, mv:Brain); junk + bogus-status skipped.
        assert_eq!(mapper.len(), 3);
        let person = mapper.lookup("bc:Person").unwrap();
        assert_eq!(person.urn_solid, "urn:solid:Person");
        assert_eq!(person.canonical_vocab, "foaf:Person");
        assert_eq!(person.status, MappingStatus::Stable);

        let brain = mapper.lookup("mv:Brain").unwrap();
        assert_eq!(brain.status, MappingStatus::Proposed);
    }

    #[test]
    fn status_filter_returns_only_matching() {
        let mapper = UrnSolidMapper::from_markdown(SAMPLE);
        let stable: Vec<_> = mapper.all_with_status(MappingStatus::Stable);
        assert_eq!(stable.len(), 2);
        let proposed: Vec<_> = mapper.all_with_status(MappingStatus::Proposed);
        assert_eq!(proposed.len(), 1);
        assert_eq!(proposed[0].our_iri, "mv:Brain");
    }

    #[test]
    fn unknown_iri_returns_none() {
        let mapper = UrnSolidMapper::from_markdown(SAMPLE);
        assert!(mapper.lookup("bc:DoesNotExist").is_none());
    }

    #[test]
    fn alignment_flag_defaults_off() {
        std::env::remove_var(URN_SOLID_ALIGNMENT_ENV);
        assert!(!urn_solid_alignment_enabled());
        std::env::set_var(URN_SOLID_ALIGNMENT_ENV, "true");
        assert!(urn_solid_alignment_enabled());
        std::env::set_var(URN_SOLID_ALIGNMENT_ENV, "0");
        assert!(!urn_solid_alignment_enabled());
        std::env::remove_var(URN_SOLID_ALIGNMENT_ENV);
    }

    #[test]
    fn empty_mapper_is_inert() {
        let mapper = UrnSolidMapper::empty();
        assert!(mapper.is_empty());
        assert!(mapper.lookup("bc:Person").is_none());
    }

    #[test]
    fn reload_without_path_is_noop() {
        let mapper = UrnSolidMapper::from_markdown(SAMPLE);
        let before = mapper.len();
        let n = mapper.reload().unwrap();
        assert_eq!(n, before);
    }
}
