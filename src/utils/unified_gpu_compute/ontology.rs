//! Specialized ontology constraint GPU kernel dispatch.
//!
//! Launches the 5 dedicated CUDA kernels from `ontology_constraints.cu` instead
//! of routing ontology constraints through the generic `force_pass_kernel`.
//! Each kernel targets a specific OWL axiom type:
//!
//! | CUDA type constant | Kernel                              | Rust discriminant |
//! |--------------------|-------------------------------------|-------------------|
//! | 1 (DisjointClasses)| `apply_disjoint_classes_kernel`     | Separation        |
//! | 2 (SubClassOf)     | `apply_subclass_hierarchy_kernel`   | Hierarchy         |
//! | 3 (SameAs)         | `apply_sameas_colocate_kernel`      | Identity          |
//! | 4 (InverseOf)      | `apply_inverse_symmetry_kernel`     | Symmetry          |
//! | 5 (Functional)     | `apply_functional_cardinality_kernel`| Cardinality      |

use super::construction::UnifiedGPUCompute;
use anyhow::{anyhow, Result};
use cust::launch;
use cust::memory::{CopyDestination, DeviceBuffer};
use log::{debug, info};

/// GPU-side representation of an ontology node.
/// Must match the `OntologyNode` struct in `ontology_constraints.cu` exactly (64 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuOntologyNode {
    pub graph_id: u32,
    pub node_id: u32,
    pub ontology_type: u32,
    pub constraint_flags: u32,
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub mass: f32,
    pub radius: f32,
    pub parent_class: u32,
    pub property_count: u32,
    pub padding: [u32; 6],
}

// SAFETY: GpuOntologyNode is repr(C), contains no pointers/references, and any bit
// pattern represents a valid (if possibly meaningless) value — matching the CUDA struct.
unsafe impl cust::memory::DeviceCopy for GpuOntologyNode {}

/// GPU-side representation of an ontology constraint.
/// Must match the `OntologyConstraint` struct in `ontology_constraints.cu` exactly (64 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuOntologyConstraint {
    pub constraint_type: u32,
    pub source_id: u32,
    pub target_id: u32,
    pub graph_id: u32,
    pub strength: f32,
    pub distance: f32,
    pub source_idx: i32,
    pub target_idx: i32,
    pub padding: [f32; 8],
}

// SAFETY: Same rationale as GpuOntologyNode — repr(C), no pointers, POD type.
unsafe impl cust::memory::DeviceCopy for GpuOntologyConstraint {}

/// CUDA constraint type constants — must match `#define`s in `ontology_constraints.cu`.
pub const CONSTRAINT_DISJOINT_CLASSES: u32 = 1;
pub const CONSTRAINT_SUBCLASS_OF: u32 = 2;
pub const CONSTRAINT_SAMEAS: u32 = 3;
pub const CONSTRAINT_INVERSE_OF: u32 = 4;
pub const CONSTRAINT_FUNCTIONAL: u32 = 5;

/// Default strength multipliers for each constraint type.
const DEFAULT_SEPARATION_STRENGTH: f32 = 50.0;
const DEFAULT_ALIGNMENT_STRENGTH: f32 = 30.0;
const DEFAULT_COLOCATE_STRENGTH: f32 = 80.0;
const DEFAULT_SYMMETRY_STRENGTH: f32 = 40.0;
const DEFAULT_CARDINALITY_PENALTY: f32 = 20.0;

const BLOCK_SIZE: u32 = 256;

impl UnifiedGPUCompute {
    /// Returns `true` if the ontology constraints PTX module is loaded and ready.
    pub fn has_ontology_module(&self) -> bool {
        self.ontology_module.is_some()
    }

    /// Launch specialized ontology constraint kernels against the current position/velocity
    /// buffers. This replaces the generic constraint path for ontology-derived constraints.
    ///
    /// # Arguments
    /// * `nodes` — Ontology node data (positions, types, masses) for all relevant nodes.
    /// * `constraints` — Ontology constraints with pre-computed source/target indices.
    /// * `delta_time` — Physics timestep.
    ///
    /// # Errors
    /// Returns `Err` if the ontology PTX module is not loaded or a kernel launch fails.
    pub fn execute_ontology_constraints(
        &mut self,
        nodes: &[GpuOntologyNode],
        constraints: &[GpuOntologyConstraint],
        delta_time: f32,
    ) -> Result<()> {
        let ontology_module = self
            .ontology_module
            .as_ref()
            .ok_or_else(|| anyhow!("Ontology PTX module not loaded"))?;

        if nodes.is_empty() || constraints.is_empty() {
            debug!("No ontology nodes or constraints to process, skipping kernel launch");
            return Ok(());
        }

        let num_nodes = nodes.len() as i32;
        let num_constraints = constraints.len() as i32;

        // Upload node and constraint data to GPU
        let d_nodes = DeviceBuffer::from_slice(nodes)?;
        let d_constraints = DeviceBuffer::from_slice(constraints)?;

        let constraint_grid = (num_constraints as u32 + BLOCK_SIZE - 1) / BLOCK_SIZE;
        let node_grid = (num_nodes as u32 + BLOCK_SIZE - 1) / BLOCK_SIZE;

        // Group constraints by type to decide which kernels to launch
        let has_disjoint = constraints.iter().any(|c| c.constraint_type == CONSTRAINT_DISJOINT_CLASSES);
        let has_subclass = constraints.iter().any(|c| c.constraint_type == CONSTRAINT_SUBCLASS_OF);
        let has_sameas = constraints.iter().any(|c| c.constraint_type == CONSTRAINT_SAMEAS);
        let has_inverse = constraints.iter().any(|c| c.constraint_type == CONSTRAINT_INVERSE_OF);
        let has_functional = constraints.iter().any(|c| c.constraint_type == CONSTRAINT_FUNCTIONAL);

        // Kernel 1: DisjointClasses — repulsion between disjoint class instances
        if has_disjoint {
            let kernel = ontology_module
                .get_function("apply_disjoint_classes_kernel")
                .map_err(|e| anyhow!("Failed to get apply_disjoint_classes_kernel: {}", e))?;

            // SAFETY: d_nodes and d_constraints are valid DeviceBuffers created above.
            // num_nodes/num_constraints match their lengths. delta_time and strength are f32.
            // The kernel writes to d_nodes velocity fields via atomicAdd.
            unsafe {
                let stream = &self.stream;
                launch!(
                    kernel<<<constraint_grid, BLOCK_SIZE, 0, stream>>>(
                        d_nodes.as_device_ptr(),
                        num_nodes,
                        d_constraints.as_device_ptr(),
                        num_constraints,
                        delta_time,
                        DEFAULT_SEPARATION_STRENGTH
                    )
                )?;
            }
            debug!("Launched apply_disjoint_classes_kernel ({} constraints)", num_constraints);
        }

        // Kernel 2: SubClassOf — hierarchical spring alignment
        if has_subclass {
            let kernel = ontology_module
                .get_function("apply_subclass_hierarchy_kernel")
                .map_err(|e| anyhow!("Failed to get apply_subclass_hierarchy_kernel: {}", e))?;

            unsafe {
                let stream = &self.stream;
                launch!(
                    kernel<<<constraint_grid, BLOCK_SIZE, 0, stream>>>(
                        d_nodes.as_device_ptr(),
                        num_nodes,
                        d_constraints.as_device_ptr(),
                        num_constraints,
                        delta_time,
                        DEFAULT_ALIGNMENT_STRENGTH
                    )
                )?;
            }
            debug!("Launched apply_subclass_hierarchy_kernel ({} constraints)", num_constraints);
        }

        // Kernel 3: SameAs — strong co-location attraction
        if has_sameas {
            let kernel = ontology_module
                .get_function("apply_sameas_colocate_kernel")
                .map_err(|e| anyhow!("Failed to get apply_sameas_colocate_kernel: {}", e))?;

            unsafe {
                let stream = &self.stream;
                launch!(
                    kernel<<<constraint_grid, BLOCK_SIZE, 0, stream>>>(
                        d_nodes.as_device_ptr(),
                        num_nodes,
                        d_constraints.as_device_ptr(),
                        num_constraints,
                        delta_time,
                        DEFAULT_COLOCATE_STRENGTH
                    )
                )?;
            }
            debug!("Launched apply_sameas_colocate_kernel ({} constraints)", num_constraints);
        }

        // Kernel 4: InverseOf — symmetric positioning
        if has_inverse {
            let kernel = ontology_module
                .get_function("apply_inverse_symmetry_kernel")
                .map_err(|e| anyhow!("Failed to get apply_inverse_symmetry_kernel: {}", e))?;

            unsafe {
                let stream = &self.stream;
                launch!(
                    kernel<<<constraint_grid, BLOCK_SIZE, 0, stream>>>(
                        d_nodes.as_device_ptr(),
                        num_nodes,
                        d_constraints.as_device_ptr(),
                        num_constraints,
                        delta_time,
                        DEFAULT_SYMMETRY_STRENGTH
                    )
                )?;
            }
            debug!("Launched apply_inverse_symmetry_kernel ({} constraints)", num_constraints);
        }

        // Kernel 5: Functional — cardinality penalty (iterates over nodes, not constraints)
        if has_functional {
            let kernel = ontology_module
                .get_function("apply_functional_cardinality_kernel")
                .map_err(|e| anyhow!("Failed to get apply_functional_cardinality_kernel: {}", e))?;

            // This kernel launches one thread per NODE (not per constraint)
            unsafe {
                let stream = &self.stream;
                launch!(
                    kernel<<<node_grid, BLOCK_SIZE, 0, stream>>>(
                        d_nodes.as_device_ptr(),
                        num_nodes,
                        d_constraints.as_device_ptr(),
                        num_constraints,
                        delta_time,
                        DEFAULT_CARDINALITY_PENALTY
                    )
                )?;
            }
            debug!("Launched apply_functional_cardinality_kernel ({} nodes)", num_nodes);
        }

        // Synchronize to ensure all kernels complete before the buffers are dropped
        self.stream.synchronize()?;

        // Copy updated velocities back from the ontology node buffer into the main
        // velocity buffers so the integration kernel picks them up.
        // The ontology kernels wrote velocity updates into d_nodes via atomicAdd.
        let mut updated_nodes = vec![GpuOntologyNode::default(); nodes.len()];
        d_nodes.copy_to(&mut updated_nodes)?;

        // Apply velocity deltas to the main simulation velocity buffers.
        // We read current velocities, add the delta from ontology kernels, and write back.
        let n = self.num_nodes.min(nodes.len());
        if n > 0 {
            let mut vel_x = vec![0.0f32; self.allocated_nodes];
            let mut vel_y = vec![0.0f32; self.allocated_nodes];
            let mut vel_z = vec![0.0f32; self.allocated_nodes];
            self.vel_in_x.copy_to(&mut vel_x)?;
            self.vel_in_y.copy_to(&mut vel_y)?;
            self.vel_in_z.copy_to(&mut vel_z)?;

            for (i, node) in updated_nodes.iter().enumerate().take(n) {
                // The kernel writes absolute velocity, but the original velocity was
                // copied into the node. The delta is (updated - original).
                let dv_x = node.velocity[0] - nodes[i].velocity[0];
                let dv_y = node.velocity[1] - nodes[i].velocity[1];
                let dv_z = node.velocity[2] - nodes[i].velocity[2];

                // Map ontology node index to simulation node index.
                // node.node_id is the simulation-side node index when populated correctly.
                let sim_idx = node.node_id as usize;
                if sim_idx < self.num_nodes {
                    vel_x[sim_idx] += dv_x;
                    vel_y[sim_idx] += dv_y;
                    vel_z[sim_idx] += dv_z;
                }
            }

            self.vel_in_x.copy_from(&vel_x)?;
            self.vel_in_y.copy_from(&vel_y)?;
            self.vel_in_z.copy_from(&vel_z)?;
        }

        info!(
            "Ontology constraint kernels executed: {} nodes, {} constraints (disjoint={}, subclass={}, sameas={}, inverse={}, functional={})",
            num_nodes, num_constraints, has_disjoint, has_subclass, has_sameas, has_inverse, has_functional
        );

        Ok(())
    }
}
