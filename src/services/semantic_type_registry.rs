//! Semantic Type Registry
//!
//! Dynamic registry for ontology relationship types that decouples ontology from code.
//! Eliminates hard-coded edge_type_to_int mappings and enables runtime type registration.
//!
//! ## Schema-Code Decoupling
//!
//! This registry enables adding new relationship types (e.g., ngm:requires, ngm:enables)
//! without requiring CUDA recompilation. The workflow is:
//!
//! 1. Register new relationship type with `registry.register("ngm:new-type", config)`
//! 2. Build GPU buffer with `registry.build_dynamic_gpu_buffer()`
//! 3. Upload to GPU with `set_dynamic_relationship_buffer(buffer.as_ptr(), count, true)`
//! 4. GPU kernel uses lookup table instead of switch statement
//!
//! Hot-reload is supported: call `update_dynamic_relationship_config` to update
//! individual types without full buffer re-upload.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, RwLock};

/// Force configuration for a relationship type
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct RelationshipForceConfig {
    /// Spring strength (0.0 - 1.0, can be negative for repulsion)
    pub strength: f32,
    /// Rest length for spring calculations
    pub rest_length: f32,
    /// Whether the force is directional (source → target only)
    pub is_directional: bool,
    /// Force type identifier for GPU kernel dispatch:
    /// - 0: Standard spring force
    /// - 1: Orbit clustering (has-part)
    /// - 2: Cross-domain long-range spring
    /// - 3: Repulsion force
    pub force_type: u32,
}

/// GPU-compatible dynamic force configuration
/// Matches the DynamicForceConfig struct in semantic_forces.cu
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct DynamicForceConfigGPU {
    /// Spring strength (can be negative for repulsion)
    pub strength: f32,
    /// Rest length for spring calculations
    pub rest_length: f32,
    /// 1 = directional (source → target), 0 = bidirectional
    pub is_directional: i32,
    /// Force behavior type (0=spring, 1=orbit, 2=cross-domain, 3=repulsion)
    pub force_type: u32,
}

impl Default for DynamicForceConfigGPU {
    fn default() -> Self {
        Self {
            strength: 0.5,
            rest_length: 100.0,
            is_directional: 0,
            force_type: 0,
        }
    }
}

impl From<&RelationshipForceConfig> for DynamicForceConfigGPU {
    fn from(config: &RelationshipForceConfig) -> Self {
        Self {
            strength: config.strength,
            rest_length: config.rest_length,
            is_directional: if config.is_directional { 1 } else { 0 },
            force_type: config.force_type,
        }
    }
}

impl Default for RelationshipForceConfig {
    fn default() -> Self {
        Self {
            strength: 0.5,
            rest_length: 100.0,
            is_directional: false,
            force_type: 0,
        }
    }
}

/// Thread-safe registry for semantic relationship types
///
/// ## Hot-Reload Versioning (ADR-070 D2.1)
///
/// `buffer_version` is incremented on every mutation (register, update).
/// The relationship-buffer constant memory carries this version so the force
/// kernel can detect mid-launch staleness.  Physics tick acquires
/// `update_lock` before kernel launch; a concurrent registry mutation holds
/// the same lock while committing, implementing the **delay-launch** pattern
/// that closes F-30 (mid-tick edge insertion race).
pub struct SemanticTypeRegistry {
    uri_to_id: RwLock<HashMap<String, u32>>,
    id_to_config: RwLock<Vec<RelationshipForceConfig>>,
    id_to_uri: RwLock<Vec<String>>,
    next_id: AtomicU32,
    /// Monotonically increasing version, bumped on every mutation.
    /// Starts at 1 after initial registration; 0 means "never updated".
    buffer_version: AtomicU64,
    /// Delay-launch mutex: physics tick acquires before kernel launch,
    /// mutation methods acquire while committing buffer changes.
    update_lock: Mutex<()>,
}

// Lock helper methods that recover from poisoned locks
impl SemanticTypeRegistry {
    fn read_uri_map(&self) -> std::sync::RwLockReadGuard<'_, HashMap<String, u32>> {
        self.uri_to_id
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn write_uri_map(&self) -> std::sync::RwLockWriteGuard<'_, HashMap<String, u32>> {
        self.uri_to_id
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn read_configs(&self) -> std::sync::RwLockReadGuard<'_, Vec<RelationshipForceConfig>> {
        self.id_to_config
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn write_configs(&self) -> std::sync::RwLockWriteGuard<'_, Vec<RelationshipForceConfig>> {
        self.id_to_config
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn read_uris(&self) -> std::sync::RwLockReadGuard<'_, Vec<String>> {
        self.id_to_uri
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn write_uris(&self) -> std::sync::RwLockWriteGuard<'_, Vec<String>> {
        self.id_to_uri
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl SemanticTypeRegistry {
    /// Create a new registry with default relationship types
    pub fn new() -> Self {
        let registry = Self {
            uri_to_id: RwLock::new(HashMap::new()),
            id_to_config: RwLock::new(Vec::new()),
            id_to_uri: RwLock::new(Vec::new()),
            next_id: AtomicU32::new(0),
            buffer_version: AtomicU64::new(0),
            update_lock: Mutex::new(()),
        };

        // Register default relationship types
        // Generic/unknown type
        registry.register_internal(
            "generic",
            RelationshipForceConfig {
                strength: 0.3,
                rest_length: 100.0,
                is_directional: false,
                force_type: 0,
            },
        );

        // Basic relationship types
        registry.register_internal(
            "dependency",
            RelationshipForceConfig {
                strength: 0.6,
                rest_length: 80.0,
                is_directional: true,
                force_type: 0,
            },
        );

        registry.register_internal(
            "hierarchy",
            RelationshipForceConfig {
                strength: 0.8,
                rest_length: 60.0,
                is_directional: true,
                force_type: 0,
            },
        );

        registry.register_internal(
            "association",
            RelationshipForceConfig {
                strength: 0.4,
                rest_length: 120.0,
                is_directional: false,
                force_type: 0,
            },
        );

        registry.register_internal(
            "sequence",
            RelationshipForceConfig {
                strength: 0.5,
                rest_length: 90.0,
                is_directional: true,
                force_type: 0,
            },
        );

        // OWL relationship types
        registry.register_internal(
            "subClassOf",
            RelationshipForceConfig {
                strength: 0.8,
                rest_length: 60.0,
                is_directional: true,
                force_type: 0,
            },
        );

        registry.register_internal(
            "rdfs:subClassOf",
            RelationshipForceConfig {
                strength: 0.8,
                rest_length: 60.0,
                is_directional: true,
                force_type: 0,
            },
        );

        registry.register_internal(
            "instanceOf",
            RelationshipForceConfig {
                strength: 0.7,
                rest_length: 70.0,
                is_directional: true,
                force_type: 0,
            },
        );

        registry.register_internal(
            "rdf:type",
            RelationshipForceConfig {
                strength: 0.7,
                rest_length: 70.0,
                is_directional: true,
                force_type: 0,
            },
        );

        // NGM ontology relationship types
        registry.register_internal(
            "ngm:requires",
            RelationshipForceConfig {
                strength: 0.7,
                rest_length: 80.0,
                is_directional: true,
                force_type: 0,
            },
        );

        registry.register_internal(
            "requires",
            RelationshipForceConfig {
                strength: 0.7,
                rest_length: 80.0,
                is_directional: true,
                force_type: 0,
            },
        );

        registry.register_internal(
            "ngm:enables",
            RelationshipForceConfig {
                strength: 0.4,
                rest_length: 120.0,
                is_directional: false,
                force_type: 0,
            },
        );

        registry.register_internal(
            "enables",
            RelationshipForceConfig {
                strength: 0.4,
                rest_length: 120.0,
                is_directional: false,
                force_type: 0,
            },
        );

        registry.register_internal(
            "ngm:has-part",
            RelationshipForceConfig {
                strength: 0.9,
                rest_length: 40.0,
                is_directional: true,
                force_type: 1, // Orbit clustering
            },
        );

        registry.register_internal(
            "has-part",
            RelationshipForceConfig {
                strength: 0.9,
                rest_length: 40.0,
                is_directional: true,
                force_type: 1,
            },
        );

        registry.register_internal(
            "ngm:bridges-to",
            RelationshipForceConfig {
                strength: 0.3,
                rest_length: 200.0,
                is_directional: false,
                force_type: 2, // Cross-domain long-range
            },
        );

        registry.register_internal(
            "bridges-to",
            RelationshipForceConfig {
                strength: 0.3,
                rest_length: 200.0,
                is_directional: false,
                force_type: 2,
            },
        );

        // Additional common relationship types
        registry.register_internal(
            "owl:equivalentClass",
            RelationshipForceConfig {
                strength: 0.9,
                rest_length: 30.0,
                is_directional: false,
                force_type: 0,
            },
        );

        registry.register_internal(
            "owl:disjointWith",
            RelationshipForceConfig {
                strength: -0.3, // Repulsive
                rest_length: 150.0,
                is_directional: false,
                force_type: 3, // Repulsion
            },
        );

        registry.register_internal(
            "skos:broader",
            RelationshipForceConfig {
                strength: 0.6,
                rest_length: 70.0,
                is_directional: true,
                force_type: 0,
            },
        );

        registry.register_internal(
            "skos:narrower",
            RelationshipForceConfig {
                strength: 0.6,
                rest_length: 70.0,
                is_directional: true,
                force_type: 0,
            },
        );

        registry.register_internal(
            "skos:related",
            RelationshipForceConfig {
                strength: 0.4,
                rest_length: 100.0,
                is_directional: false,
                force_type: 0,
            },
        );

        // ── ADR-064 / graph-cognition-core: 35 EdgeKind variants (vc: prefix) ──
        // Values sourced from crates/graph-cognition-physics-presets/presets/edge-kinds.toml
        registry.register_all_edge_kinds();

        registry
    }

    /// Register all 35 EdgeKind variants from the graph-cognition taxonomy.
    ///
    /// Each entry uses a `vc:` URI prefix (VisionClaw namespace) so there is no
    /// collision with the existing `ngm:`, `rdfs:`, `owl:`, `skos:` registrations.
    /// Force parameters match `edge-kinds.toml` in graph-cognition-physics-presets.
    fn register_all_edge_kinds(&self) {
        // ── Structural (5) ──
        self.register_internal(
            "vc:contains",
            RelationshipForceConfig {
                strength: 0.8,
                rest_length: 0.6,
                is_directional: true,
                force_type: 0,
            },
        );
        self.register_internal(
            "vc:inherits_from",
            RelationshipForceConfig {
                strength: 0.8,
                rest_length: 0.6,
                is_directional: true,
                force_type: 0,
            },
        );
        self.register_internal(
            "vc:implements",
            RelationshipForceConfig {
                strength: 0.7,
                rest_length: 0.7,
                is_directional: true,
                force_type: 0,
            },
        );
        self.register_internal(
            "vc:composed_of",
            RelationshipForceConfig {
                strength: 0.85,
                rest_length: 0.5,
                is_directional: true,
                force_type: 1, // orbit clustering
            },
        );
        self.register_internal(
            "vc:nests",
            RelationshipForceConfig {
                strength: 0.9,
                rest_length: 0.4,
                is_directional: true,
                force_type: 0,
            },
        );

        // ── Behavioral (4) ──
        self.register_internal(
            "vc:calls",
            RelationshipForceConfig {
                strength: 0.6,
                rest_length: 0.8,
                is_directional: true,
                force_type: 0,
            },
        );
        self.register_internal(
            "vc:overrides",
            RelationshipForceConfig {
                strength: 0.7,
                rest_length: 0.6,
                is_directional: true,
                force_type: 0,
            },
        );
        self.register_internal(
            "vc:triggers",
            RelationshipForceConfig {
                strength: 0.5,
                rest_length: 0.9,
                is_directional: true,
                force_type: 0,
            },
        );
        self.register_internal(
            "vc:subscribes",
            RelationshipForceConfig {
                strength: 0.4,
                rest_length: 1.2,
                is_directional: true,
                force_type: 0,
            },
        );

        // ── Data Flow (4) ──
        self.register_internal(
            "vc:reads_from",
            RelationshipForceConfig {
                strength: 0.5,
                rest_length: 0.9,
                is_directional: true,
                force_type: 0,
            },
        );
        self.register_internal(
            "vc:writes_to",
            RelationshipForceConfig {
                strength: 0.5,
                rest_length: 0.9,
                is_directional: true,
                force_type: 0,
            },
        );
        self.register_internal(
            "vc:transforms_to",
            RelationshipForceConfig {
                strength: 0.6,
                rest_length: 0.8,
                is_directional: true,
                force_type: 0,
            },
        );
        self.register_internal(
            "vc:pipes",
            RelationshipForceConfig {
                strength: 0.5,
                rest_length: 0.9,
                is_directional: true,
                force_type: 0,
            },
        );

        // ── Dependencies (4) ──
        self.register_internal(
            "vc:depends_on",
            RelationshipForceConfig {
                strength: 0.7,
                rest_length: 0.8,
                is_directional: true,
                force_type: 0,
            },
        );
        self.register_internal(
            "vc:imports",
            RelationshipForceConfig {
                strength: 0.7,
                rest_length: 0.6,
                is_directional: true,
                force_type: 0,
            },
        );
        self.register_internal(
            "vc:requires",
            RelationshipForceConfig {
                strength: 0.7,
                rest_length: 0.8,
                is_directional: true,
                force_type: 0,
            },
        );
        self.register_internal(
            "vc:enables",
            RelationshipForceConfig {
                strength: 0.4,
                rest_length: 1.2,
                is_directional: false,
                force_type: 0,
            },
        );

        // ── Semantic (5) ──
        self.register_internal(
            "vc:sub_class_of",
            RelationshipForceConfig {
                strength: 0.8,
                rest_length: 0.6,
                is_directional: true,
                force_type: 0,
            },
        );
        self.register_internal(
            "vc:instance_of",
            RelationshipForceConfig {
                strength: 0.7,
                rest_length: 0.7,
                is_directional: true,
                force_type: 0,
            },
        );
        self.register_internal(
            "vc:equivalent_to",
            RelationshipForceConfig {
                strength: 0.9,
                rest_length: 0.3,
                is_directional: false,
                force_type: 0,
            },
        );
        self.register_internal(
            "vc:disjoint_with",
            RelationshipForceConfig {
                strength: -0.3,
                rest_length: 1.5,
                is_directional: false,
                force_type: 3, // repulsion
            },
        );
        self.register_internal(
            "vc:same_as",
            RelationshipForceConfig {
                strength: 0.9,
                rest_length: 0.3,
                is_directional: false,
                force_type: 0,
            },
        );

        // ── Infrastructure (4) ──
        self.register_internal(
            "vc:deploys_to",
            RelationshipForceConfig {
                strength: 0.4,
                rest_length: 1.2,
                is_directional: true,
                force_type: 0,
            },
        );
        self.register_internal(
            "vc:routes_to",
            RelationshipForceConfig {
                strength: 0.4,
                rest_length: 1.2,
                is_directional: true,
                force_type: 0,
            },
        );
        self.register_internal(
            "vc:replicates_to",
            RelationshipForceConfig {
                strength: 0.3,
                rest_length: 1.5,
                is_directional: true,
                force_type: 0,
            },
        );
        self.register_internal(
            "vc:monitors",
            RelationshipForceConfig {
                strength: 0.3,
                rest_length: 1.5,
                is_directional: true,
                force_type: 0,
            },
        );

        // ── Domain (4) ──
        self.register_internal(
            "vc:has_part",
            RelationshipForceConfig {
                strength: 0.9,
                rest_length: 0.4,
                is_directional: true,
                force_type: 1, // orbit clustering
            },
        );
        self.register_internal(
            "vc:bridges_to",
            RelationshipForceConfig {
                strength: 0.3,
                rest_length: 2.0,
                is_directional: false,
                force_type: 2, // cross-domain long-range
            },
        );
        self.register_internal(
            "vc:fulfills",
            RelationshipForceConfig {
                strength: 0.5,
                rest_length: 1.0,
                is_directional: true,
                force_type: 0,
            },
        );
        self.register_internal(
            "vc:constrains",
            RelationshipForceConfig {
                strength: 0.6,
                rest_length: 0.8,
                is_directional: true,
                force_type: 0,
            },
        );

        // ── Knowledge (5) ──
        self.register_internal(
            "vc:wiki_link",
            RelationshipForceConfig {
                strength: 0.5,
                rest_length: 1.0,
                is_directional: false,
                force_type: 0,
            },
        );
        self.register_internal(
            "vc:block_ref",
            RelationshipForceConfig {
                strength: 0.4,
                rest_length: 1.2,
                is_directional: false,
                force_type: 0,
            },
        );
        self.register_internal(
            "vc:block_parent",
            RelationshipForceConfig {
                strength: 0.85,
                rest_length: 0.3,
                is_directional: true,
                force_type: 0,
            },
        );
        self.register_internal(
            "vc:tagged_with",
            RelationshipForceConfig {
                strength: 0.3,
                rest_length: 1.5,
                is_directional: true,
                force_type: 0,
            },
        );
        self.register_internal(
            "vc:cited_by",
            RelationshipForceConfig {
                strength: 0.4,
                rest_length: 1.2,
                is_directional: true,
                force_type: 0,
            },
        );
    }

    /// Internal registration (bypasses lock acquisition for initialization)
    fn register_internal(&self, uri: &str, config: RelationshipForceConfig) -> u32 {
        let _guard = self.update_lock.lock().unwrap_or_else(|p| p.into_inner());
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);

        let mut uri_map = self.write_uri_map();
        let mut configs = self.write_configs();
        let mut uris = self.write_uris();

        uri_map.insert(uri.to_string(), id);
        configs.push(config);
        uris.push(uri.to_string());

        self.buffer_version.fetch_add(1, Ordering::Release);

        id
    }

    /// Register a new relationship type with force configuration
    /// Returns the assigned ID for the type
    pub fn register(&self, uri: &str, config: RelationshipForceConfig) -> u32 {
        // Check if already registered
        {
            let uri_map = self.read_uri_map();
            if let Some(&existing_id) = uri_map.get(uri) {
                // Update existing config under the delay-launch lock
                let _guard = self.update_lock.lock().unwrap_or_else(|p| p.into_inner());
                let mut configs = self.write_configs();
                if (existing_id as usize) < configs.len() {
                    configs[existing_id as usize] = config;
                }
                self.buffer_version.fetch_add(1, Ordering::Release);
                return existing_id;
            }
        }

        // register_internal acquires update_lock and bumps version itself
        self.register_internal(uri, config)
    }

    /// Get the ID for a relationship type URI
    pub fn get_id(&self, uri: &str) -> Option<u32> {
        let uri_map = self.read_uri_map();
        uri_map.get(uri).copied()
    }

    /// Get the ID for a relationship type, registering with defaults if not found
    pub fn get_or_register_id(&self, uri: &str) -> u32 {
        if let Some(id) = self.get_id(uri) {
            return id;
        }

        // Register with default config
        self.register(uri, RelationshipForceConfig::default())
    }

    /// Get the force configuration for a relationship type ID
    pub fn get_config(&self, id: u32) -> Option<RelationshipForceConfig> {
        let configs = self.read_configs();
        configs.get(id as usize).copied()
    }

    /// Get the URI for a relationship type ID
    pub fn get_uri(&self, id: u32) -> Option<String> {
        let uris = self.read_uris();
        uris.get(id as usize).cloned()
    }

    /// Update the configuration for an existing relationship type
    pub fn update_config(&self, uri: &str, config: RelationshipForceConfig) -> bool {
        let uri_map = self.read_uri_map();
        if let Some(&id) = uri_map.get(uri) {
            drop(uri_map); // Release read lock before acquiring write lock
            let _guard = self.update_lock.lock().unwrap_or_else(|p| p.into_inner());
            let mut configs = self.write_configs();
            if (id as usize) < configs.len() {
                configs[id as usize] = config;
                self.buffer_version.fetch_add(1, Ordering::Release);
                return true;
            }
        }
        false
    }

    /// Build a GPU-compatible buffer of all force configurations
    /// Buffer is indexed by relationship type ID
    pub fn build_gpu_buffer(&self) -> Vec<RelationshipForceConfig> {
        let configs = self.read_configs();
        configs.clone()
    }

    /// Build a GPU buffer with the proper C-compatible struct layout
    /// for the dynamic relationship system in semantic_forces.cu
    pub fn build_dynamic_gpu_buffer(&self) -> Vec<DynamicForceConfigGPU> {
        let configs = self.read_configs();
        configs
            .iter()
            .map(|c| DynamicForceConfigGPU::from(c))
            .collect()
    }

    /// Get the buffer version (ADR-070 D2.1).
    ///
    /// Monotonically increasing counter bumped on every mutation
    /// (`register`, `register_internal`, `update_config`).
    /// Starts at 1 after the first registration; version 0 means
    /// "registry never mutated" which lets consumers distinguish
    /// an uninitialised buffer from a valid one.
    pub fn buffer_version(&self) -> u64 {
        self.buffer_version.load(Ordering::Acquire)
    }

    /// Acquire the delay-launch lock (ADR-070 D2.1 / F-30).
    ///
    /// Physics tick calls this **before** launching the force kernel.
    /// If a registry mutation is in progress the caller blocks until
    /// the mutation is fully committed, guaranteeing the kernel never
    /// reads a partially-written relationship buffer.
    pub fn acquire_update_lock(&self) -> MutexGuard<'_, ()> {
        self.update_lock.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Legacy version accessor — returns type count as proxy.
    /// Prefer [`buffer_version`] for hot-reload detection.
    #[deprecated(
        since = "0.70.0",
        note = "use buffer_version() for hot-reload versioning"
    )]
    pub fn version(&self) -> u32 {
        self.next_id.load(Ordering::SeqCst)
    }

    /// Get the number of registered relationship types
    pub fn len(&self) -> usize {
        let configs = self.read_configs();
        configs.len()
    }

    /// Check if the registry is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get all registered URIs
    pub fn registered_uris(&self) -> Vec<String> {
        let uris = self.read_uris();
        uris.clone()
    }

    /// Convert edge type string to integer ID (legacy compatibility)
    /// Returns the ID if the type is registered, or 0 (generic) if not found
    pub fn edge_type_to_int(&self, edge_type: &Option<String>) -> i32 {
        edge_type
            .as_deref()
            .and_then(|uri| self.get_id(uri))
            .map(|id| id as i32)
            .unwrap_or(0)
    }
}

impl Default for SemanticTypeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// Global singleton registry instance
lazy_static::lazy_static! {
    pub static ref SEMANTIC_TYPE_REGISTRY: SemanticTypeRegistry = SemanticTypeRegistry::new();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_creation() {
        let registry = SemanticTypeRegistry::new();
        // Should have default types registered
        assert!(registry.len() > 0);
    }

    #[test]
    fn test_default_types_registered() {
        let registry = SemanticTypeRegistry::new();

        // Check that default types are registered
        assert!(registry.get_id("generic").is_some());
        assert!(registry.get_id("ngm:requires").is_some());
        assert!(registry.get_id("ngm:enables").is_some());
        assert!(registry.get_id("ngm:has-part").is_some());
        assert!(registry.get_id("ngm:bridges-to").is_some());
        assert!(registry.get_id("rdfs:subClassOf").is_some());
    }

    #[test]
    fn test_vc_edge_kinds_registered() {
        let registry = SemanticTypeRegistry::new();

        // 22 legacy + 35 vc: edge kinds = 57 total
        assert_eq!(registry.len(), 57);

        // Spot-check representative vc: entries from each category
        assert!(registry.get_id("vc:contains").is_some());
        assert!(registry.get_id("vc:calls").is_some());
        assert!(registry.get_id("vc:reads_from").is_some());
        assert!(registry.get_id("vc:depends_on").is_some());
        assert!(registry.get_id("vc:sub_class_of").is_some());
        assert!(registry.get_id("vc:deploys_to").is_some());
        assert!(registry.get_id("vc:has_part").is_some());
        assert!(registry.get_id("vc:wiki_link").is_some());
        assert!(registry.get_id("vc:cited_by").is_some());

        // Verify force_type for special entries
        let hp_id = registry.get_id("vc:has_part").unwrap();
        let hp_cfg = registry.get_config(hp_id).unwrap();
        assert_eq!(hp_cfg.force_type, 1); // orbit clustering

        let bt_id = registry.get_id("vc:bridges_to").unwrap();
        let bt_cfg = registry.get_config(bt_id).unwrap();
        assert_eq!(bt_cfg.force_type, 2); // cross-domain long-range

        let dj_id = registry.get_id("vc:disjoint_with").unwrap();
        let dj_cfg = registry.get_config(dj_id).unwrap();
        assert_eq!(dj_cfg.force_type, 3); // repulsion
        assert!(dj_cfg.strength < 0.0); // negative spring_k

        let co_id = registry.get_id("vc:composed_of").unwrap();
        let co_cfg = registry.get_config(co_id).unwrap();
        assert_eq!(co_cfg.force_type, 1); // orbit clustering
    }

    #[test]
    fn test_register_new_type() {
        let registry = SemanticTypeRegistry::new();
        let initial_len = registry.len();

        let id = registry.register(
            "custom:test-type",
            RelationshipForceConfig {
                strength: 0.5,
                rest_length: 100.0,
                is_directional: true,
                force_type: 0,
            },
        );

        assert_eq!(registry.len(), initial_len + 1);
        assert_eq!(registry.get_id("custom:test-type"), Some(id));
    }

    #[test]
    fn test_get_config() {
        let registry = SemanticTypeRegistry::new();

        let id = registry.get_id("ngm:requires").unwrap();
        let config = registry.get_config(id).unwrap();

        assert_eq!(config.strength, 0.7);
        assert!(config.is_directional);
    }

    #[test]
    fn test_update_config() {
        let registry = SemanticTypeRegistry::new();

        let updated = registry.update_config(
            "ngm:requires",
            RelationshipForceConfig {
                strength: 0.9,
                rest_length: 50.0,
                is_directional: true,
                force_type: 0,
            },
        );

        assert!(updated);

        let id = registry.get_id("ngm:requires").unwrap();
        let config = registry.get_config(id).unwrap();
        assert_eq!(config.strength, 0.9);
        assert_eq!(config.rest_length, 50.0);
    }

    #[test]
    fn test_gpu_buffer() {
        let registry = SemanticTypeRegistry::new();
        let buffer = registry.build_gpu_buffer();

        assert_eq!(buffer.len(), registry.len());
    }

    #[test]
    fn test_edge_type_to_int() {
        let registry = SemanticTypeRegistry::new();

        // Registered type
        let id = registry.edge_type_to_int(&Some("ngm:requires".to_string()));
        assert!(id > 0);

        // Unregistered type returns 0 (generic)
        let unknown_id = registry.edge_type_to_int(&Some("unknown:type".to_string()));
        assert_eq!(unknown_id, 0);

        // None returns 0
        let none_id = registry.edge_type_to_int(&None);
        assert_eq!(none_id, 0);
    }

    #[test]
    fn test_get_or_register_id() {
        let registry = SemanticTypeRegistry::new();

        // Existing type
        let id1 = registry.get_or_register_id("ngm:requires");
        let id2 = registry.get_or_register_id("ngm:requires");
        assert_eq!(id1, id2);

        // New type gets registered
        let new_id = registry.get_or_register_id("new:auto-registered");
        assert!(registry.get_id("new:auto-registered").is_some());
        assert_eq!(registry.get_id("new:auto-registered"), Some(new_id));
    }

    // ── ADR-070 D2.1: hot-reload versioning tests ──

    #[test]
    fn test_buffer_version_starts_nonzero() {
        let registry = SemanticTypeRegistry::new();
        // After initial registrations, version must be > 0
        assert!(
            registry.buffer_version() > 0,
            "buffer_version must be > 0 after new()"
        );
    }

    #[test]
    fn test_buffer_version_increments_on_register() {
        let registry = SemanticTypeRegistry::new();
        let v1 = registry.buffer_version();

        registry.register("test:version-bump", RelationshipForceConfig::default());

        let v2 = registry.buffer_version();
        assert_eq!(v2, v1 + 1, "register() of new type must bump version by 1");
    }

    #[test]
    fn test_buffer_version_increments_on_re_register() {
        let registry = SemanticTypeRegistry::new();

        registry.register("test:re-reg", RelationshipForceConfig::default());
        let v1 = registry.buffer_version();

        // Re-register same URI with different config (update path)
        registry.register(
            "test:re-reg",
            RelationshipForceConfig {
                strength: 0.99,
                ..RelationshipForceConfig::default()
            },
        );

        let v2 = registry.buffer_version();
        assert_eq!(v2, v1 + 1, "re-register (update path) must bump version");
    }

    #[test]
    fn test_buffer_version_increments_on_update_config() {
        let registry = SemanticTypeRegistry::new();
        let v1 = registry.buffer_version();

        let updated = registry.update_config(
            "generic",
            RelationshipForceConfig {
                strength: 0.1,
                ..RelationshipForceConfig::default()
            },
        );
        assert!(updated);

        let v2 = registry.buffer_version();
        assert_eq!(v2, v1 + 1, "update_config must bump version");
    }

    #[test]
    fn test_acquire_update_lock_does_not_deadlock() {
        let registry = SemanticTypeRegistry::new();
        // Acquire and immediately drop — must not deadlock
        let guard = registry.acquire_update_lock();
        drop(guard);

        // Verify we can still mutate after dropping the guard
        let v1 = registry.buffer_version();
        registry.register("test:after-lock", RelationshipForceConfig::default());
        assert_eq!(registry.buffer_version(), v1 + 1);
    }

    #[test]
    fn test_version_zero_means_never_updated() {
        // A raw struct with no registrations should have version 0
        let empty = SemanticTypeRegistry {
            uri_to_id: RwLock::new(HashMap::new()),
            id_to_config: RwLock::new(Vec::new()),
            id_to_uri: RwLock::new(Vec::new()),
            next_id: AtomicU32::new(0),
            buffer_version: AtomicU64::new(0),
            update_lock: Mutex::new(()),
        };
        assert_eq!(empty.buffer_version(), 0, "version 0 = never updated");
    }
}
