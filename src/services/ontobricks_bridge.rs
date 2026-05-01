//! OntoBricks MCP Bridge — Epic G
//!
//! Bridges external ontology sources (via MCP / Model Context Protocol) with the
//! existing GPU-accelerated ontology constraint pipeline. Fetched axioms are mapped
//! to the five native CUDA constraint types defined in `ontology_constraints.cu` and
//! consumed by [`OntologyConstraintActor`].
//!
//! ## Constraint Type Mapping
//!
//! | AxiomType             | GPU constant | CUDA kernel                         |
//! |-----------------------|-------------|-------------------------------------|
//! | SubClassOf            | 2           | `apply_subclass_hierarchy_kernel`   |
//! | DisjointWith          | 1           | `apply_disjoint_classes_kernel`     |
//! | EquivalentClass       | 3           | `apply_sameas_colocate_kernel`      |
//! | SameAs                | 3           | `apply_sameas_colocate_kernel`      |
//! | InverseOf             | 4           | `apply_inverse_symmetry_kernel`     |
//! | FunctionalProperty    | 5           | `apply_functional_cardinality_kernel`|
//! | ObjectPropertyDomain  | —           | skipped (no GPU mapping)            |
//! | ObjectPropertyRange   | —           | skipped (no GPU mapping)            |
//!
//! ## Usage
//!
//! ```rust,ignore
//! let mut bridge = OntobricksBridge::new("http://localhost:9700/mcp/ontobricks");
//! let result = bridge.refresh_if_stale()?;
//! let gpu_constraints = OntobricksBridge::map_to_gpu_constraints(&bridge.cached_axioms);
//! ```

use chrono::{DateTime, Utc};
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::Instant;
use thiserror::Error;

// Re-export GPU constraint constants from the canonical location.
use crate::utils::unified_gpu_compute::ontology::{
    CONSTRAINT_DISJOINT_CLASSES, CONSTRAINT_FUNCTIONAL, CONSTRAINT_INVERSE_OF, CONSTRAINT_SAMEAS,
    CONSTRAINT_SUBCLASS_OF,
};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors produced by the OntoBricks bridge.
#[derive(Debug, Error)]
pub enum BridgeError {
    #[error("MCP endpoint unavailable: {0}")]
    EndpointUnavailable(String),

    #[error("invalid response from MCP endpoint: {0}")]
    InvalidResponse(String),

    #[error("axiom mapping failed: {0}")]
    MappingFailed(String),

    #[error("cache has expired and no endpoint configured for refresh")]
    CacheExpired,
}

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

/// OWL axiom types that may arrive from an external ontology source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AxiomType {
    SubClassOf,
    DisjointWith,
    EquivalentClass,
    SameAs,
    InverseOf,
    FunctionalProperty,
    ObjectPropertyDomain,
    ObjectPropertyRange,
}

impl fmt::Display for AxiomType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SubClassOf => write!(f, "SubClassOf"),
            Self::DisjointWith => write!(f, "DisjointWith"),
            Self::EquivalentClass => write!(f, "EquivalentClass"),
            Self::SameAs => write!(f, "SameAs"),
            Self::InverseOf => write!(f, "InverseOf"),
            Self::FunctionalProperty => write!(f, "FunctionalProperty"),
            Self::ObjectPropertyDomain => write!(f, "ObjectPropertyDomain"),
            Self::ObjectPropertyRange => write!(f, "ObjectPropertyRange"),
        }
    }
}

/// A single axiom fetched from an external ontology via MCP.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OntologyAxiom {
    pub axiom_type: AxiomType,
    /// OWL class or property IRI (subject).
    pub subject_iri: String,
    /// Target class or property IRI (object).
    pub object_iri: String,
    /// Source ontology identifier, e.g. `"ontobricks:building-topology"`.
    pub source_ontology: String,
    /// Confidence score from the MCP response, in `[0.0, 1.0]`.
    pub confidence: f32,
}

/// Result summary returned after a fetch + map cycle.
#[derive(Debug, Clone)]
pub struct BridgeResult {
    pub axioms_fetched: usize,
    pub axioms_mapped: usize,
    pub axioms_skipped: usize,
    pub duration_ms: u64,
}

/// GPU-side constraint matching the layout consumed by
/// [`GpuOntologyConstraint`](crate::utils::unified_gpu_compute::ontology::GpuOntologyConstraint).
///
/// Node IDs must be resolved by the caller before uploading to the GPU — this
/// bridge produces them with placeholder IDs (0) because IRI-to-node resolution
/// depends on the current graph state.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct GpuConstraint {
    /// One of the five CUDA constraint constants (1..=5).
    pub constraint_type: u32,
    /// Source node ID (must be resolved from `subject_iri`).
    pub source_node_id: u32,
    /// Target node ID (must be resolved from `object_iri`).
    pub target_node_id: u32,
    /// Constraint strength derived from axiom confidence.
    pub strength: f32,
}

// ---------------------------------------------------------------------------
// Bridge
// ---------------------------------------------------------------------------

/// Default cache time-to-live in seconds (1 hour).
const DEFAULT_CACHE_TTL_SECS: u64 = 3600;

/// The MCP bridge that fetches OWL axioms from an OntoBricks-compatible endpoint
/// and maps them onto the five GPU constraint types used by the CUDA kernels.
pub struct OntobricksBridge {
    endpoint: String,
    cached_axioms: Vec<OntologyAxiom>,
    last_fetch_at: Option<DateTime<Utc>>,
    cache_ttl_secs: u64,
}

impl OntobricksBridge {
    /// Create a new bridge targeting the given MCP endpoint URL.
    pub fn new(endpoint: &str) -> Self {
        Self {
            endpoint: endpoint.to_owned(),
            cached_axioms: Vec::new(),
            last_fetch_at: None,
            cache_ttl_secs: DEFAULT_CACHE_TTL_SECS,
        }
    }

    /// Override the cache TTL (in seconds).
    pub fn with_cache_ttl(mut self, secs: u64) -> Self {
        self.cache_ttl_secs = secs;
        self
    }

    // ------------------------------------------------------------------
    // Fetch
    // ------------------------------------------------------------------

    /// Fetch axioms from the MCP endpoint for the given ontology IRI.
    ///
    /// Current implementation returns stubbed test data so the module can be
    /// integrated and tested end-to-end before the MCP transport layer is wired.
    /// The real implementation will issue an HTTP/JSON-RPC call to `self.endpoint`.
    pub fn fetch_axioms(&mut self, ontology_iri: &str) -> Result<Vec<OntologyAxiom>, BridgeError> {
        info!(
            "OntobricksBridge: fetching axioms for ontology '{}' from '{}'",
            ontology_iri, self.endpoint
        );

        // --- stub: return representative test axioms ---
        let axioms = self.stub_axioms(ontology_iri);

        self.cached_axioms = axioms.clone();
        self.last_fetch_at = Some(Utc::now());

        info!(
            "OntobricksBridge: cached {} axioms for '{}'",
            self.cached_axioms.len(),
            ontology_iri
        );

        Ok(axioms)
    }

    // ------------------------------------------------------------------
    // Mapping
    // ------------------------------------------------------------------

    /// Map a slice of [`OntologyAxiom`]s to GPU constraints.
    ///
    /// Axiom types that have no corresponding CUDA kernel (e.g. `ObjectPropertyDomain`,
    /// `ObjectPropertyRange`) are silently skipped. Node IDs are left as 0 — the
    /// caller must resolve IRIs to concrete graph node IDs before uploading.
    pub fn map_to_gpu_constraints(axioms: &[OntologyAxiom]) -> Vec<GpuConstraint> {
        let mut out = Vec::with_capacity(axioms.len());

        for axiom in axioms {
            if let Some(cuda_type) = Self::axiom_to_cuda_type(axiom.axiom_type) {
                out.push(GpuConstraint {
                    constraint_type: cuda_type,
                    source_node_id: 0, // placeholder — IRI resolution needed
                    target_node_id: 0,
                    strength: axiom.confidence,
                });
            } else {
                debug!(
                    "OntobricksBridge: skipping unsupported axiom type {} ({}→{})",
                    axiom.axiom_type, axiom.subject_iri, axiom.object_iri
                );
            }
        }

        out
    }

    // ------------------------------------------------------------------
    // Cache management
    // ------------------------------------------------------------------

    /// Refresh the axiom cache if the TTL has elapsed. Returns a [`BridgeResult`]
    /// summarising what happened.
    ///
    /// If the cache is still fresh the method returns immediately with zero counts.
    pub fn refresh_if_stale(&mut self) -> Result<BridgeResult, BridgeError> {
        if !self.is_cache_stale() {
            return Ok(BridgeResult {
                axioms_fetched: 0,
                axioms_mapped: 0,
                axioms_skipped: 0,
                duration_ms: 0,
            });
        }

        let start = Instant::now();

        // Use a well-known default IRI when refreshing. A production implementation
        // would track which ontology IRIs have been requested previously.
        let axioms = self.fetch_axioms("urn:ontobricks:default")?;
        let mapped = Self::map_to_gpu_constraints(&axioms);

        let fetched = axioms.len();
        let mapped_count = mapped.len();
        let skipped = fetched.saturating_sub(mapped_count);
        let duration_ms = start.elapsed().as_millis() as u64;

        info!(
            "OntobricksBridge: refresh complete — fetched={}, mapped={}, skipped={}, {}ms",
            fetched, mapped_count, skipped, duration_ms
        );

        Ok(BridgeResult {
            axioms_fetched: fetched,
            axioms_mapped: mapped_count,
            axioms_skipped: skipped,
            duration_ms,
        })
    }

    /// Number of axioms currently held in the cache.
    pub fn axiom_count(&self) -> usize {
        self.cached_axioms.len()
    }

    /// Drop all cached axioms and reset the fetch timestamp.
    pub fn clear_cache(&mut self) {
        self.cached_axioms.clear();
        self.last_fetch_at = None;
        info!("OntobricksBridge: cache cleared");
    }

    /// Read-only access to the cached axioms.
    pub fn cached_axioms(&self) -> &[OntologyAxiom] {
        &self.cached_axioms
    }

    /// The MCP endpoint URL.
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    // ------------------------------------------------------------------
    // Internals
    // ------------------------------------------------------------------

    /// Returns `true` when the cache has never been populated or the TTL has
    /// elapsed since the last fetch.
    fn is_cache_stale(&self) -> bool {
        match self.last_fetch_at {
            None => true,
            Some(ts) => {
                let age = Utc::now().signed_duration_since(ts);
                age.num_seconds() as u64 >= self.cache_ttl_secs
            }
        }
    }

    /// Map an [`AxiomType`] to the CUDA constraint constant, or `None` for
    /// axiom types with no GPU kernel.
    fn axiom_to_cuda_type(axiom_type: AxiomType) -> Option<u32> {
        match axiom_type {
            AxiomType::SubClassOf => Some(CONSTRAINT_SUBCLASS_OF),
            AxiomType::DisjointWith => Some(CONSTRAINT_DISJOINT_CLASSES),
            AxiomType::EquivalentClass => Some(CONSTRAINT_SAMEAS),
            AxiomType::SameAs => Some(CONSTRAINT_SAMEAS),
            AxiomType::InverseOf => Some(CONSTRAINT_INVERSE_OF),
            AxiomType::FunctionalProperty => Some(CONSTRAINT_FUNCTIONAL),
            // No GPU kernel for property domain/range axioms.
            AxiomType::ObjectPropertyDomain => None,
            AxiomType::ObjectPropertyRange => None,
        }
    }

    /// Produce representative stub axioms for integration testing.
    fn stub_axioms(&self, ontology_iri: &str) -> Vec<OntologyAxiom> {
        vec![
            OntologyAxiom {
                axiom_type: AxiomType::SubClassOf,
                subject_iri: format!("{ontology_iri}#Wall"),
                object_iri: format!("{ontology_iri}#BuildingElement"),
                source_ontology: "ontobricks:building-topology".into(),
                confidence: 0.95,
            },
            OntologyAxiom {
                axiom_type: AxiomType::DisjointWith,
                subject_iri: format!("{ontology_iri}#Space"),
                object_iri: format!("{ontology_iri}#BuildingElement"),
                source_ontology: "ontobricks:building-topology".into(),
                confidence: 0.90,
            },
            OntologyAxiom {
                axiom_type: AxiomType::SameAs,
                subject_iri: format!("{ontology_iri}#Room"),
                object_iri: format!("{ontology_iri}#EnclosedSpace"),
                source_ontology: "ontobricks:building-topology".into(),
                confidence: 0.85,
            },
            OntologyAxiom {
                axiom_type: AxiomType::InverseOf,
                subject_iri: format!("{ontology_iri}#contains"),
                object_iri: format!("{ontology_iri}#isContainedIn"),
                source_ontology: "ontobricks:building-topology".into(),
                confidence: 1.0,
            },
            OntologyAxiom {
                axiom_type: AxiomType::FunctionalProperty,
                subject_iri: format!("{ontology_iri}#hasPrimaryFunction"),
                object_iri: String::new(),
                source_ontology: "ontobricks:building-topology".into(),
                confidence: 0.80,
            },
            OntologyAxiom {
                axiom_type: AxiomType::EquivalentClass,
                subject_iri: format!("{ontology_iri}#Storey"),
                object_iri: format!("{ontology_iri}#Floor"),
                source_ontology: "ontobricks:building-topology".into(),
                confidence: 0.92,
            },
            // Unsupported types — should be skipped during mapping.
            OntologyAxiom {
                axiom_type: AxiomType::ObjectPropertyDomain,
                subject_iri: format!("{ontology_iri}#adjacentTo"),
                object_iri: format!("{ontology_iri}#Space"),
                source_ontology: "ontobricks:building-topology".into(),
                confidence: 1.0,
            },
            OntologyAxiom {
                axiom_type: AxiomType::ObjectPropertyRange,
                subject_iri: format!("{ontology_iri}#adjacentTo"),
                object_iri: format!("{ontology_iri}#Space"),
                source_ontology: "ontobricks:building-topology".into(),
                confidence: 1.0,
            },
        ]
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_ENDPOINT: &str = "http://localhost:9700/mcp/ontobricks";
    const TEST_IRI: &str = "urn:ontobricks:test";

    fn make_bridge() -> OntobricksBridge {
        OntobricksBridge::new(TEST_ENDPOINT)
    }

    // -- construction -------------------------------------------------------

    #[test]
    fn new_bridge_has_empty_cache() {
        let bridge = make_bridge();
        assert_eq!(bridge.axiom_count(), 0);
        assert!(bridge.last_fetch_at.is_none());
        assert_eq!(bridge.endpoint(), TEST_ENDPOINT);
    }

    #[test]
    fn with_cache_ttl_overrides_default() {
        let bridge = make_bridge().with_cache_ttl(120);
        assert_eq!(bridge.cache_ttl_secs, 120);
    }

    // -- fetch --------------------------------------------------------------

    #[test]
    fn fetch_axioms_populates_cache() {
        let mut bridge = make_bridge();
        let axioms = bridge.fetch_axioms(TEST_IRI).expect("fetch should succeed");
        assert!(!axioms.is_empty());
        assert_eq!(bridge.axiom_count(), axioms.len());
        assert!(bridge.last_fetch_at.is_some());
    }

    #[test]
    fn fetch_axioms_returns_all_expected_types() {
        let mut bridge = make_bridge();
        let axioms = bridge.fetch_axioms(TEST_IRI).unwrap();
        let types: Vec<AxiomType> = axioms.iter().map(|a| a.axiom_type).collect();

        assert!(types.contains(&AxiomType::SubClassOf));
        assert!(types.contains(&AxiomType::DisjointWith));
        assert!(types.contains(&AxiomType::SameAs));
        assert!(types.contains(&AxiomType::InverseOf));
        assert!(types.contains(&AxiomType::FunctionalProperty));
        assert!(types.contains(&AxiomType::EquivalentClass));
        assert!(types.contains(&AxiomType::ObjectPropertyDomain));
        assert!(types.contains(&AxiomType::ObjectPropertyRange));
    }

    // -- axiom type mapping -------------------------------------------------

    #[test]
    fn axiom_to_cuda_type_supported_types() {
        assert_eq!(
            OntobricksBridge::axiom_to_cuda_type(AxiomType::SubClassOf),
            Some(CONSTRAINT_SUBCLASS_OF)
        );
        assert_eq!(
            OntobricksBridge::axiom_to_cuda_type(AxiomType::DisjointWith),
            Some(CONSTRAINT_DISJOINT_CLASSES)
        );
        assert_eq!(
            OntobricksBridge::axiom_to_cuda_type(AxiomType::EquivalentClass),
            Some(CONSTRAINT_SAMEAS)
        );
        assert_eq!(
            OntobricksBridge::axiom_to_cuda_type(AxiomType::SameAs),
            Some(CONSTRAINT_SAMEAS)
        );
        assert_eq!(
            OntobricksBridge::axiom_to_cuda_type(AxiomType::InverseOf),
            Some(CONSTRAINT_INVERSE_OF)
        );
        assert_eq!(
            OntobricksBridge::axiom_to_cuda_type(AxiomType::FunctionalProperty),
            Some(CONSTRAINT_FUNCTIONAL)
        );
    }

    #[test]
    fn axiom_to_cuda_type_unsupported_returns_none() {
        assert_eq!(
            OntobricksBridge::axiom_to_cuda_type(AxiomType::ObjectPropertyDomain),
            None
        );
        assert_eq!(
            OntobricksBridge::axiom_to_cuda_type(AxiomType::ObjectPropertyRange),
            None
        );
    }

    // -- GPU constraint mapping ---------------------------------------------

    #[test]
    fn map_to_gpu_constraints_skips_unsupported() {
        let mut bridge = make_bridge();
        let axioms = bridge.fetch_axioms(TEST_IRI).unwrap();

        let total = axioms.len();
        let unsupported = axioms
            .iter()
            .filter(|a| {
                matches!(
                    a.axiom_type,
                    AxiomType::ObjectPropertyDomain | AxiomType::ObjectPropertyRange
                )
            })
            .count();

        let constraints = OntobricksBridge::map_to_gpu_constraints(&axioms);
        assert_eq!(constraints.len(), total - unsupported);
    }

    #[test]
    fn map_to_gpu_constraints_preserves_confidence_as_strength() {
        let axioms = vec![OntologyAxiom {
            axiom_type: AxiomType::SubClassOf,
            subject_iri: "urn:test#A".into(),
            object_iri: "urn:test#B".into(),
            source_ontology: "test".into(),
            confidence: 0.77,
        }];

        let constraints = OntobricksBridge::map_to_gpu_constraints(&axioms);
        assert_eq!(constraints.len(), 1);
        assert!((constraints[0].strength - 0.77).abs() < f32::EPSILON);
        assert_eq!(constraints[0].constraint_type, CONSTRAINT_SUBCLASS_OF);
    }

    #[test]
    fn map_to_gpu_constraints_empty_input() {
        let constraints = OntobricksBridge::map_to_gpu_constraints(&[]);
        assert!(constraints.is_empty());
    }

    #[test]
    fn constraint_type_values_match_cuda_constants() {
        // Verify the constant values align with the CUDA kernel #defines.
        assert_eq!(CONSTRAINT_DISJOINT_CLASSES, 1);
        assert_eq!(CONSTRAINT_SUBCLASS_OF, 2);
        assert_eq!(CONSTRAINT_SAMEAS, 3);
        assert_eq!(CONSTRAINT_INVERSE_OF, 4);
        assert_eq!(CONSTRAINT_FUNCTIONAL, 5);
    }

    // -- cache TTL ----------------------------------------------------------

    #[test]
    fn fresh_cache_is_not_stale() {
        let mut bridge = make_bridge();
        bridge.fetch_axioms(TEST_IRI).unwrap();
        assert!(!bridge.is_cache_stale());
    }

    #[test]
    fn empty_cache_is_stale() {
        let bridge = make_bridge();
        assert!(bridge.is_cache_stale());
    }

    #[test]
    fn expired_cache_is_stale() {
        let mut bridge = make_bridge().with_cache_ttl(0);
        bridge.fetch_axioms(TEST_IRI).unwrap();
        // TTL=0 means any non-zero age is stale.
        assert!(bridge.is_cache_stale());
    }

    #[test]
    fn refresh_if_stale_returns_zero_when_fresh() {
        let mut bridge = make_bridge();
        bridge.fetch_axioms(TEST_IRI).unwrap();
        let result = bridge.refresh_if_stale().unwrap();
        assert_eq!(result.axioms_fetched, 0);
        assert_eq!(result.axioms_mapped, 0);
        assert_eq!(result.axioms_skipped, 0);
        assert_eq!(result.duration_ms, 0);
    }

    #[test]
    fn refresh_if_stale_fetches_when_expired() {
        let mut bridge = make_bridge().with_cache_ttl(0);
        let result = bridge.refresh_if_stale().unwrap();
        assert!(result.axioms_fetched > 0);
        assert!(result.axioms_mapped > 0);
        // The stub data includes 2 unsupported axiom types.
        assert_eq!(result.axioms_skipped, 2);
    }

    // -- clear cache --------------------------------------------------------

    #[test]
    fn clear_cache_empties_everything() {
        let mut bridge = make_bridge();
        bridge.fetch_axioms(TEST_IRI).unwrap();
        assert!(bridge.axiom_count() > 0);

        bridge.clear_cache();
        assert_eq!(bridge.axiom_count(), 0);
        assert!(bridge.last_fetch_at.is_none());
        assert!(bridge.is_cache_stale());
    }

    // -- GpuConstraint repr -------------------------------------------------

    #[test]
    fn gpu_constraint_is_16_bytes() {
        // 4 x u32/f32 fields = 16 bytes. Verify repr(C) packing.
        assert_eq!(std::mem::size_of::<GpuConstraint>(), 16);
    }

    // -- AxiomType display --------------------------------------------------

    #[test]
    fn axiom_type_display() {
        assert_eq!(format!("{}", AxiomType::SubClassOf), "SubClassOf");
        assert_eq!(format!("{}", AxiomType::DisjointWith), "DisjointWith");
        assert_eq!(
            format!("{}", AxiomType::ObjectPropertyRange),
            "ObjectPropertyRange"
        );
    }

    // -- BridgeError --------------------------------------------------------

    #[test]
    fn bridge_error_display() {
        let err = BridgeError::EndpointUnavailable("timeout".into());
        assert!(err.to_string().contains("timeout"));

        let err = BridgeError::InvalidResponse("bad json".into());
        assert!(err.to_string().contains("bad json"));

        let err = BridgeError::MappingFailed("unknown IRI".into());
        assert!(err.to_string().contains("unknown IRI"));

        let err = BridgeError::CacheExpired;
        assert!(err.to_string().contains("expired"));
    }

    // -- EquivalentClass maps to SameAs GPU type ----------------------------

    #[test]
    fn equivalent_class_maps_to_sameas_constant() {
        let axioms = vec![OntologyAxiom {
            axiom_type: AxiomType::EquivalentClass,
            subject_iri: "urn:test#Storey".into(),
            object_iri: "urn:test#Floor".into(),
            source_ontology: "test".into(),
            confidence: 0.92,
        }];
        let constraints = OntobricksBridge::map_to_gpu_constraints(&axioms);
        assert_eq!(constraints.len(), 1);
        assert_eq!(constraints[0].constraint_type, CONSTRAINT_SAMEAS);
    }

    // -- all supported types produce correct GPU constant -------------------

    #[test]
    fn all_supported_types_produce_correct_constant() {
        let cases: Vec<(AxiomType, u32)> = vec![
            (AxiomType::SubClassOf, CONSTRAINT_SUBCLASS_OF),
            (AxiomType::DisjointWith, CONSTRAINT_DISJOINT_CLASSES),
            (AxiomType::EquivalentClass, CONSTRAINT_SAMEAS),
            (AxiomType::SameAs, CONSTRAINT_SAMEAS),
            (AxiomType::InverseOf, CONSTRAINT_INVERSE_OF),
            (AxiomType::FunctionalProperty, CONSTRAINT_FUNCTIONAL),
        ];

        for (axiom_type, expected_cuda) in cases {
            let axioms = vec![OntologyAxiom {
                axiom_type,
                subject_iri: "urn:a".into(),
                object_iri: "urn:b".into(),
                source_ontology: "test".into(),
                confidence: 1.0,
            }];
            let constraints = OntobricksBridge::map_to_gpu_constraints(&axioms);
            assert_eq!(
                constraints[0].constraint_type, expected_cuda,
                "axiom type {:?} should map to CUDA constant {}",
                axiom_type, expected_cuda
            );
        }
    }
}
