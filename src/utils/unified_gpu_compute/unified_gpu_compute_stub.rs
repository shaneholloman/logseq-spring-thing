//! CPU-only stub for UnifiedGPUCompute.
//! Provides the type definition so that non-GPU code compiles,
//! but all methods panic if actually called without the GPU feature.

use super::types::GPUPerformanceMetrics;
use crate::models::simulation_params::SimParams;

/// Stub UnifiedGPUCompute for cpu-only builds.
/// GPU-dependent fields are omitted; only the interface needed
/// by non-GPU code (type-level references in messages) is provided.
pub struct UnifiedGPUCompute {
    pub num_nodes: usize,
    pub num_edges: usize,
    pub max_grid_cells: usize,
}

impl UnifiedGPUCompute {
    pub fn get_performance_metrics(&self) -> GPUPerformanceMetrics {
        GPUPerformanceMetrics::default()
    }

    pub fn get_params(&self) -> &SimParams {
        unimplemented!("GPU not available in cpu-only build")
    }
}
