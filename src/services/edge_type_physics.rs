//! Per-Edge-Type Physics Configuration
//!
//! Maps `SemanticEdgeType` variants to GPU-compatible force parameters that drive
//! per-edge-type force differentiation in the CUDA spring kernel. The generated
//! `DynamicForceConfigGPU` buffer is indexed by the `SemanticEdgeType` discriminant
//! (u8), enabling the GPU to apply distinct rest lengths, spring strengths, and
//! force behaviors (spring / orbit / cross-domain / repulsion) per edge without
//! CPU-side per-frame branching.
//!
//! Integration path:
//!   EdgeTypePhysicsConfig::default()
//!     → .to_gpu_buffer()
//!     → upload to `DynamicRelationshipBuffer` constant memory via SemanticForcesActor
//!
//! The config covers all `SemanticEdgeType` variants including materialized N-hop
//! edges (ADR-014 Phase 3) which use extremely weak springs to provide barely
//! perceptible transitive grouping without disrupting direct-link layout.

use crate::models::edge::SemanticEdgeType;
use crate::services::semantic_type_registry::DynamicForceConfigGPU;

/// Force parameters for a single `SemanticEdgeType` variant.
#[derive(Debug, Clone, Copy)]
pub struct EdgeTypeForceParams {
    pub edge_type: SemanticEdgeType,
    /// Rest length of the spring (in world units). Larger values produce looser layout.
    pub rest_length: f32,
    /// Spring strength coefficient. Range [0.0, 1.0] for attraction; can be < 0 for repulsion.
    pub spring_strength: f32,
    /// Force behavior type matching the GPU kernel dispatch:
    ///   0 = standard spring
    ///   1 = orbit clustering (has-part semantics)
    ///   2 = cross-domain long-range spring
    ///   3 = repulsion
    pub force_type: u32,
    /// Whether the force acts only from source to target (true) or bidirectionally (false).
    pub is_directional: bool,
}

/// Aggregate physics configuration covering all `SemanticEdgeType` variants.
/// Provides a default configuration tuned for knowledge-graph visualization where
/// hierarchy and structural edges pull tight while bridges and materialized edges
/// stay loose.
#[derive(Debug, Clone)]
pub struct EdgeTypePhysicsConfig {
    pub configs: Vec<EdgeTypeForceParams>,
}

impl Default for EdgeTypePhysicsConfig {
    fn default() -> Self {
        Self {
            configs: vec![
                EdgeTypeForceParams {
                    edge_type: SemanticEdgeType::ExplicitLink,
                    rest_length: 100.0,
                    spring_strength: 0.5,
                    force_type: 0,
                    is_directional: false,
                },
                EdgeTypeForceParams {
                    edge_type: SemanticEdgeType::Hierarchical,
                    rest_length: 60.0,
                    spring_strength: 0.9,
                    force_type: 0,
                    is_directional: true,
                },
                EdgeTypeForceParams {
                    edge_type: SemanticEdgeType::Structural,
                    rest_length: 80.0,
                    spring_strength: 0.7,
                    force_type: 1, // orbit clustering
                    is_directional: false,
                },
                EdgeTypeForceParams {
                    edge_type: SemanticEdgeType::Dependency,
                    rest_length: 90.0,
                    spring_strength: 0.6,
                    force_type: 0,
                    is_directional: true,
                },
                EdgeTypeForceParams {
                    edge_type: SemanticEdgeType::Associative,
                    rest_length: 120.0,
                    spring_strength: 0.3,
                    force_type: 0,
                    is_directional: false,
                },
                EdgeTypeForceParams {
                    edge_type: SemanticEdgeType::Bridge,
                    rest_length: 200.0,
                    spring_strength: 0.15,
                    force_type: 2, // cross-domain long-range
                    is_directional: false,
                },
                EdgeTypeForceParams {
                    edge_type: SemanticEdgeType::Namespace,
                    rest_length: 150.0,
                    spring_strength: 0.2,
                    force_type: 0,
                    is_directional: false,
                },
                EdgeTypeForceParams {
                    edge_type: SemanticEdgeType::Inferred,
                    rest_length: 110.0,
                    spring_strength: 0.4,
                    force_type: 0,
                    is_directional: false,
                },
                EdgeTypeForceParams {
                    edge_type: SemanticEdgeType::Materialized2Hop,
                    rest_length: 250.0,
                    spring_strength: 0.08,
                    force_type: 0,
                    is_directional: false,
                },
                EdgeTypeForceParams {
                    edge_type: SemanticEdgeType::Materialized3Hop,
                    rest_length: 350.0,
                    spring_strength: 0.03,
                    force_type: 0,
                    is_directional: false,
                },
            ],
        }
    }
}

impl EdgeTypePhysicsConfig {
    /// Convert this config into a GPU-compatible buffer suitable for upload to
    /// the `DynamicRelationshipBuffer` constant memory in `semantic_forces.cu`.
    ///
    /// The returned vector is indexed by `SemanticEdgeType` discriminant (u8).
    /// Slots for missing variants are filled with a default configuration so
    /// the GPU kernel can safely index any u8 in [0, len).
    pub fn to_gpu_buffer(&self) -> Vec<DynamicForceConfigGPU> {
        // Determine required buffer size: at least covers all known variants
        let max_discriminant = self
            .configs
            .iter()
            .map(|c| c.edge_type as usize)
            .max()
            .unwrap_or(0);

        let buffer_len = max_discriminant + 1;
        let mut buffer = vec![DynamicForceConfigGPU::default(); buffer_len];

        for cfg in &self.configs {
            let idx = cfg.edge_type as usize;
            buffer[idx] = DynamicForceConfigGPU {
                strength: cfg.spring_strength,
                rest_length: cfg.rest_length,
                is_directional: if cfg.is_directional { 1 } else { 0 },
                force_type: cfg.force_type,
            };
        }

        buffer
    }

    /// Total number of configured edge types.
    pub fn len(&self) -> usize {
        self.configs.len()
    }

    /// Returns true if no edge types are configured.
    pub fn is_empty(&self) -> bool {
        self.configs.is_empty()
    }

    /// Look up the force parameters for a given `SemanticEdgeType`.
    /// Returns `None` if the type is not configured (should not happen with default config).
    pub fn get(&self, edge_type: SemanticEdgeType) -> Option<&EdgeTypeForceParams> {
        self.configs.iter().find(|c| c.edge_type == edge_type)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// All SemanticEdgeType variants (including materialized) are covered by default config.
    #[test]
    fn default_config_covers_all_variants() {
        let config = EdgeTypePhysicsConfig::default();

        let all_variants = [
            SemanticEdgeType::ExplicitLink,
            SemanticEdgeType::Hierarchical,
            SemanticEdgeType::Structural,
            SemanticEdgeType::Dependency,
            SemanticEdgeType::Associative,
            SemanticEdgeType::Bridge,
            SemanticEdgeType::Namespace,
            SemanticEdgeType::Inferred,
            SemanticEdgeType::Materialized2Hop,
            SemanticEdgeType::Materialized3Hop,
        ];

        for variant in &all_variants {
            assert!(
                config.get(*variant).is_some(),
                "Missing config for SemanticEdgeType::{:?}",
                variant
            );
        }
    }

    /// GPU buffer is sized to cover all discriminants and has correct alignment.
    #[test]
    fn gpu_buffer_correct_size_and_alignment() {
        let config = EdgeTypePhysicsConfig::default();
        let buffer = config.to_gpu_buffer();

        // Must cover discriminants 0..=9 (10 variants)
        assert_eq!(buffer.len(), 10);

        // Verify struct size matches C layout expectations (4 floats * 4 bytes = 16 bytes)
        assert_eq!(
            std::mem::size_of::<DynamicForceConfigGPU>(),
            16,
            "DynamicForceConfigGPU must be 16 bytes for GPU alignment"
        );
    }

    /// Materialized edges have strictly weaker springs than any direct edge type.
    #[test]
    fn materialized_edges_weaker_than_direct() {
        let config = EdgeTypePhysicsConfig::default();

        let mat2 = config.get(SemanticEdgeType::Materialized2Hop).unwrap();
        let mat3 = config.get(SemanticEdgeType::Materialized3Hop).unwrap();

        // Find minimum spring_strength among non-materialized edges
        let min_direct_strength = config
            .configs
            .iter()
            .filter(|c| {
                c.edge_type != SemanticEdgeType::Materialized2Hop
                    && c.edge_type != SemanticEdgeType::Materialized3Hop
            })
            .map(|c| c.spring_strength)
            .fold(f32::INFINITY, f32::min);

        assert!(
            mat2.spring_strength < min_direct_strength,
            "Materialized2Hop strength ({}) must be < min direct strength ({})",
            mat2.spring_strength,
            min_direct_strength
        );
        assert!(
            mat3.spring_strength < mat2.spring_strength,
            "Materialized3Hop strength ({}) must be < Materialized2Hop ({})",
            mat3.spring_strength,
            mat2.spring_strength
        );
    }

    /// GPU buffer entries at materialized discriminants have correct values.
    #[test]
    fn gpu_buffer_materialized_values() {
        let config = EdgeTypePhysicsConfig::default();
        let buffer = config.to_gpu_buffer();

        let mat2_gpu = &buffer[SemanticEdgeType::Materialized2Hop as usize];
        assert_eq!(mat2_gpu.strength, 0.08);
        assert_eq!(mat2_gpu.rest_length, 250.0);
        assert_eq!(mat2_gpu.is_directional, 0);
        assert_eq!(mat2_gpu.force_type, 0);

        let mat3_gpu = &buffer[SemanticEdgeType::Materialized3Hop as usize];
        assert_eq!(mat3_gpu.strength, 0.03);
        assert_eq!(mat3_gpu.rest_length, 350.0);
        assert_eq!(mat3_gpu.is_directional, 0);
        assert_eq!(mat3_gpu.force_type, 0);
    }

    /// Hierarchical edges use directional force.
    #[test]
    fn hierarchical_is_directional() {
        let config = EdgeTypePhysicsConfig::default();
        let buffer = config.to_gpu_buffer();
        let hier = &buffer[SemanticEdgeType::Hierarchical as usize];
        assert_eq!(hier.is_directional, 1);
        assert_eq!(hier.strength, 0.9);
    }

    /// Bridge edges use cross-domain force type.
    #[test]
    fn bridge_uses_cross_domain_force_type() {
        let config = EdgeTypePhysicsConfig::default();
        let bridge = config.get(SemanticEdgeType::Bridge).unwrap();
        assert_eq!(bridge.force_type, 2);
    }

    /// Structural edges use orbit clustering force type.
    #[test]
    fn structural_uses_orbit_clustering() {
        let config = EdgeTypePhysicsConfig::default();
        let structural = config.get(SemanticEdgeType::Structural).unwrap();
        assert_eq!(structural.force_type, 1);
    }

    /// Rest lengths increase monotonically from hierarchy through materialized.
    #[test]
    fn rest_lengths_increase_with_weakness() {
        let config = EdgeTypePhysicsConfig::default();
        let hier = config.get(SemanticEdgeType::Hierarchical).unwrap();
        let structural = config.get(SemanticEdgeType::Structural).unwrap();
        let explicit = config.get(SemanticEdgeType::ExplicitLink).unwrap();
        let bridge = config.get(SemanticEdgeType::Bridge).unwrap();
        let mat2 = config.get(SemanticEdgeType::Materialized2Hop).unwrap();
        let mat3 = config.get(SemanticEdgeType::Materialized3Hop).unwrap();

        assert!(hier.rest_length < structural.rest_length);
        assert!(structural.rest_length < explicit.rest_length);
        assert!(explicit.rest_length < bridge.rest_length);
        assert!(bridge.rest_length < mat2.rest_length);
        assert!(mat2.rest_length < mat3.rest_length);
    }
}
