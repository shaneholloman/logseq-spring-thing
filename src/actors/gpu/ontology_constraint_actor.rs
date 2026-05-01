//! Ontology Constraint Actor - GPU-accelerated ontology constraint evaluation
//!
//! This actor handles ontology-derived physics constraints on the GPU, translating
//! OWL axioms and ontology rules into physics forces that guide graph layout.
//!
//! ## Architecture
//!
//! Follows the established GPU actor pattern:
//! - SharedGPUContext for unified GPU access
//! - Graceful CPU fallback on GPU errors
//! - Memory pooling for constraint buffers
//! - Integration with ontology validation system

use actix::prelude::*;
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;

use super::shared::{GPUState, SharedGPUContext};
use crate::actors::messages::*;
use crate::models::constraints::{Constraint, ConstraintData, ConstraintSet};
use crate::physics::ontology_constraints::{
    OWLAxiom, OntologyConstraintTranslator, OntologyReasoningReport,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OntologyConstraintStats {
    pub total_axioms_processed: u32,
    pub active_ontology_constraints: u32,
    pub constraint_evaluation_count: u32,
    pub last_update_time_ms: f32,
    pub gpu_failure_count: u32,
    pub cpu_fallback_count: u32,
    pub constraint_cache_hits: u32,
    pub constraint_cache_misses: u32,
}

impl Default for OntologyConstraintStats {
    fn default() -> Self {
        Self {
            total_axioms_processed: 0,
            active_ontology_constraints: 0,
            constraint_evaluation_count: 0,
            last_update_time_ms: 0.0,
            gpu_failure_count: 0,
            cpu_fallback_count: 0,
            constraint_cache_hits: 0,
            constraint_cache_misses: 0,
        }
    }
}

pub struct OntologyConstraintActor {

    shared_context: Option<Arc<SharedGPUContext>>,


    translator: OntologyConstraintTranslator,


    ontology_constraints: Vec<Constraint>,


    constraint_buffer: Vec<ConstraintData>,


    gpu_state: GPUState,


    stats: OntologyConstraintStats,


    
    last_update: Instant,


    gpu_initialized: bool,

    /// Address of ForceComputeActor for sending constraint buffer updates
    force_compute_addr: Option<actix::Addr<super::force_compute_actor::ForceComputeActor>>,
}

impl OntologyConstraintActor {
    
    pub fn new() -> Self {
        info!("Creating new OntologyConstraintActor");

        Self {
            shared_context: None,
            translator: OntologyConstraintTranslator::new(),
            ontology_constraints: Vec::new(),
            constraint_buffer: Vec::new(),
            gpu_state: GPUState::default(),
            stats: OntologyConstraintStats::default(),
            last_update: Instant::now(),
            gpu_initialized: false,
            force_compute_addr: None,
        }
    }

    
    fn initialize_gpu(&mut self) -> Result<(), String> {
        if self.shared_context.is_none() {
            return Err("GPU context not available".to_string());
        }

        info!("OntologyConstraintActor: GPU initialization - context available");
        self.gpu_initialized = true;
        Ok(())
    }

    
    
    fn apply_ontology_constraints(
        &mut self,
        reasoning_report: &OntologyReasoningReport,
        graph_data: &crate::models::graph::GraphData,
    ) -> Result<(), String> {
        let start_time = Instant::now();

        info!(
            "OntologyConstraintActor: Applying ontology constraints - {} axioms, {} inferences",
            reasoning_report.axioms.len(),
            reasoning_report.inferences.len()
        );

        
        let constraint_set = self
            .translator
            .apply_ontology_constraints(graph_data, reasoning_report)
            .map_err(|e| format!("Failed to translate ontology constraints: {}", e))?;

        
        self.ontology_constraints = constraint_set.constraints.clone();
        self.stats.total_axioms_processed += reasoning_report.axioms.len() as u32;
        self.stats.active_ontology_constraints = self
            .ontology_constraints
            .iter()
            .filter(|c| c.active)
            .count() as u32;


        self.constraint_buffer = constraint_set.to_gpu_data();


        if self.gpu_initialized && self.shared_context.is_some() {
            // Try specialized ontology kernels first (5 dedicated CUDA kernels)
            match self.apply_specialized_constraints(graph_data) {
                Ok(true) => {
                    info!(
                        "OntologyConstraintActor: Specialized ontology kernels executed for {} constraints",
                        self.ontology_constraints.iter().filter(|c| c.active).count()
                    );
                }
                Ok(false) => {
                    // Ontology PTX module not loaded — fall back to generic constraint upload
                    info!("OntologyConstraintActor: Ontology PTX not loaded, using generic constraint path");
                    match self.upload_constraints_to_gpu() {
                        Ok(_) => {
                            info!(
                                "OntologyConstraintActor: Successfully uploaded {} constraints to GPU (generic path)",
                                self.constraint_buffer.len()
                            );
                        }
                        Err(e) => {
                            warn!(
                                "OntologyConstraintActor: GPU upload failed, using CPU fallback: {}",
                                e
                            );
                            self.stats.gpu_failure_count += 1;
                            self.stats.cpu_fallback_count += 1;
                        }
                    }
                }
                Err(e) => {
                    warn!(
                        "OntologyConstraintActor: Specialized kernel execution failed: {}. Falling back to generic path.",
                        e
                    );
                    self.stats.gpu_failure_count += 1;
                    // Fall back to generic constraint upload
                    match self.upload_constraints_to_gpu() {
                        Ok(_) => {
                            info!(
                                "OntologyConstraintActor: Fallback generic upload succeeded ({} constraints)",
                                self.constraint_buffer.len()
                            );
                        }
                        Err(e2) => {
                            warn!(
                                "OntologyConstraintActor: Both specialized and generic paths failed: {}",
                                e2
                            );
                            self.stats.cpu_fallback_count += 1;
                        }
                    }
                }
            }
        } else {
            debug!(
                "OntologyConstraintActor: GPU not available, constraints stored for CPU processing"
            );
            self.stats.cpu_fallback_count += 1;
        }

        self.last_update = Instant::now();
        self.stats.last_update_time_ms = start_time.elapsed().as_secs_f32() * 1000.0;
        self.stats.constraint_evaluation_count += 1;

        info!(
            "OntologyConstraintActor: Constraint application completed in {:.2}ms",
            self.stats.last_update_time_ms
        );

        // Notify ForceComputeActor about the new constraint buffer
        self.notify_force_compute_actor();

        Ok(())
    }

    /// Send the updated constraint buffer to ForceComputeActor
    fn notify_force_compute_actor(&self) {
        if let Some(ref addr) = self.force_compute_addr {
            info!(
                "OntologyConstraintActor: Sending {} constraints to ForceComputeActor",
                self.constraint_buffer.len()
            );
            addr.do_send(UpdateOntologyConstraintBuffer {
                constraint_buffer: self.constraint_buffer.clone(),
            });
        } else {
            debug!("OntologyConstraintActor: ForceComputeActor address not set, skipping notification");
        }
    }


    fn update_constraints(&mut self, axioms: &[OWLAxiom]) -> Result<(), String> {
        info!(
            "OntologyConstraintActor: Updating constraints with {} new axioms",
            axioms.len()
        );

        
        
        warn!("OntologyConstraintActor: Dynamic constraint updates require graph context");
        warn!("Consider using ApplyOntologyConstraints message with full context");

        self.stats.total_axioms_processed += axioms.len() as u32;

        Ok(())
    }

    /// Attempt to launch the 5 specialized ontology constraint kernels directly.
    /// Returns `Ok(true)` if specialized kernels were executed, `Ok(false)` if the
    /// ontology PTX module is not loaded (caller should fall back to generic path),
    /// or `Err` on GPU failure.
    fn apply_specialized_constraints(
        &self,
        graph_data: &crate::models::graph::GraphData,
    ) -> Result<bool, String> {
        let shared_context = self
            .shared_context
            .as_ref()
            .ok_or("GPU context not available")?;

        let mut unified_compute = shared_context
            .unified_compute
            .lock()
            .map_err(|e| format!("Failed to acquire GPU compute lock: {}", e))?;

        if !unified_compute.has_ontology_module() {
            return Ok(false);
        }

        use crate::utils::unified_gpu_compute::{
            GpuOntologyNode, GpuOntologyConstraint,
            CONSTRAINT_DISJOINT_CLASSES, CONSTRAINT_SUBCLASS_OF,
            CONSTRAINT_SAMEAS, CONSTRAINT_FUNCTIONAL,
        };

        // Build GpuOntologyNode array from graph data and current GPU positions.
        // We use the array index as the simulation node index.
        let num_nodes = graph_data.nodes.len();
        let mut gpu_nodes: Vec<GpuOntologyNode> = Vec::with_capacity(num_nodes);

        for (idx, node) in graph_data.nodes.iter().enumerate() {
            // Determine ontology type flags from node metadata (HashMap<String, String>)
            let owl_type = node.metadata.get("owl_type").map(|s| s.as_str()).unwrap_or("");
            let mut ontology_type: u32 = 0;
            if owl_type == "class" {
                ontology_type |= 0x01; // ONTOLOGY_CLASS
            }
            if owl_type == "individual" {
                ontology_type |= 0x02; // ONTOLOGY_INDIVIDUAL
            }
            if owl_type == "property" {
                ontology_type |= 0x04; // ONTOLOGY_PROPERTY
            }
            // Default to individual if no owl_type specified
            if ontology_type == 0 {
                ontology_type = 0x02;
            }

            let property_count = node
                .metadata
                .get("property_count")
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(0);

            let parent_class = node
                .metadata
                .get("parent_class_id")
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(0);

            let mass = node
                .metadata
                .get("mass")
                .and_then(|v| v.parse::<f32>().ok())
                .or(node.mass)
                .unwrap_or(1.0);

            let radius = node
                .metadata
                .get("radius")
                .and_then(|v| v.parse::<f32>().ok())
                .unwrap_or(10.0);

            gpu_nodes.push(GpuOntologyNode {
                graph_id: 0, // default graph
                node_id: idx as u32,
                ontology_type,
                constraint_flags: 0,
                position: [
                    node.x.unwrap_or(0.0),
                    node.y.unwrap_or(0.0),
                    node.z.unwrap_or(0.0),
                ],
                velocity: [0.0, 0.0, 0.0],
                mass,
                radius,
                parent_class,
                property_count,
                padding: [0; 6],
            });
        }

        // Build GpuOntologyConstraint array from ontology_constraints.
        // Map ConstraintKind to CUDA constraint type constants.
        // The ontology translator maps OWL axioms as follows:
        //   DisjointClasses -> ConstraintKind::Separation  -> CUDA type 1
        //   SubClassOf      -> ConstraintKind::Clustering  -> CUDA type 2
        //   SameAs          -> ConstraintKind::Clustering (with colocation params) -> CUDA type 3
        //   FunctionalProp  -> ConstraintKind::Boundary   -> CUDA type 5
        //   Semantic        -> sub-type encoded in params[0]
        let mut gpu_constraints: Vec<GpuOntologyConstraint> = Vec::new();

        for constraint in &self.ontology_constraints {
            if !constraint.active {
                continue;
            }

            let cuda_type = match constraint.kind {
                crate::models::constraints::ConstraintKind::Separation => CONSTRAINT_DISJOINT_CLASSES,
                crate::models::constraints::ConstraintKind::Clustering => {
                    // Clustering is used for both SubClassOf and SameAs.
                    // Distinguish by number of node_indices: SameAs has exactly 2 nodes
                    // with a very high weight (colocation), SubClassOf has 1 node + centroid params.
                    if constraint.node_indices.len() == 2 && constraint.weight >= 0.9 {
                        CONSTRAINT_SAMEAS
                    } else {
                        CONSTRAINT_SUBCLASS_OF
                    }
                }
                crate::models::constraints::ConstraintKind::Boundary => CONSTRAINT_FUNCTIONAL,
                crate::models::constraints::ConstraintKind::Semantic => {
                    // Semantic constraints encode sub-type in params[0]:
                    // 1.0=disjoint, 2.0=subclass, 3.0=sameas, 4.0=inverse, 5.0=functional
                    let sub_type = constraint.params.first().copied().unwrap_or(0.0) as u32;
                    if sub_type >= 1 && sub_type <= 5 {
                        sub_type
                    } else {
                        CONSTRAINT_SUBCLASS_OF // default to hierarchy
                    }
                }
                _ => continue, // Skip non-ontology constraint types
            };

            let source_id = constraint.node_indices.first().copied().unwrap_or(0);
            let target_id = constraint.node_indices.get(1).copied().unwrap_or(0);
            let graph_id = 0u32; // default graph

            // Pre-compute indices (the host-side equivalent of precompute_constraint_indices)
            let source_idx = if (source_id as usize) < num_nodes {
                source_id as i32
            } else {
                -1
            };
            let target_idx = if (target_id as usize) < num_nodes {
                target_id as i32
            } else {
                -1
            };

            let distance = constraint.params.get(1).copied().unwrap_or(50.0);

            gpu_constraints.push(GpuOntologyConstraint {
                constraint_type: cuda_type,
                source_id,
                target_id,
                graph_id,
                strength: constraint.weight,
                distance,
                source_idx,
                target_idx,
                padding: [0.0; 8],
            });
        }

        if gpu_constraints.is_empty() {
            info!("OntologyConstraintActor: No active ontology constraints to process via specialized kernels");
            return Ok(true);
        }

        info!(
            "OntologyConstraintActor: Launching specialized ontology kernels - {} nodes, {} constraints",
            gpu_nodes.len(),
            gpu_constraints.len()
        );

        let delta_time = 0.016; // ~60 FPS default timestep

        unified_compute
            .execute_ontology_constraints(&gpu_nodes, &gpu_constraints, delta_time)
            .map_err(|e| format!("Specialized ontology kernel execution failed: {}", e))?;

        Ok(true)
    }


    fn upload_constraints_to_gpu(&self) -> Result<(), String> {
        let shared_context = self
            .shared_context
            .as_ref()
            .ok_or("GPU context not available")?;

        
        let mut unified_compute = shared_context
            .unified_compute
            .lock()
            .map_err(|e| format!("Failed to acquire GPU compute lock: {}", e))?;

        
        if self.constraint_buffer.is_empty() {
            debug!("OntologyConstraintActor: No constraints to upload, clearing GPU constraints");
            unified_compute
                .clear_constraints()
                .map_err(|e| format!("Failed to clear GPU constraints: {}", e))?;
        } else {
            unified_compute
                .upload_constraints(&self.constraint_buffer)
                .map_err(|e| format!("Failed to upload constraints to GPU: {}", e))?;
        }

        Ok(())
    }

    
    fn get_ontology_stats(&self) -> OntologyConstraintStats {
        self.stats.clone()
    }

    
    fn cleanup(&mut self) -> Result<(), String> {
        info!("OntologyConstraintActor: Cleaning up resources");

        
        self.ontology_constraints.clear();
        self.constraint_buffer.clear();

        
        if let Some(ref shared_context) = self.shared_context {
            if let Ok(mut unified_compute) = shared_context.unified_compute.lock() {
                if let Err(e) = unified_compute.clear_constraints() {
                    warn!("OntologyConstraintActor: Failed to clear GPU constraints during cleanup: {}", e);
                }
            }
        }

        
        self.translator.clear_cache();

        info!("OntologyConstraintActor: Cleanup completed");
        Ok(())
    }
}

impl Actor for OntologyConstraintActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        info!("Ontology Constraint Actor started");
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        info!("Ontology Constraint Actor stopped");
        let _ = self.cleanup();
    }
}

// === Message Handlers ===

impl Handler<ApplyOntologyConstraints> for OntologyConstraintActor {
    type Result = Result<(), String>;

    fn handle(&mut self, msg: ApplyOntologyConstraints, _ctx: &mut Self::Context) -> Self::Result {
        info!(
            "OntologyConstraintActor: Received ApplyOntologyConstraints message for graph_id {}",
            msg.graph_id
        );

        // ADR-070 D1.4: Hard cap on constraint count to prevent OOM kernel launch.
        // Configurable via physics.max_ontology_constraints setting (default 50,000).
        const MAX_ONTOLOGY_CONSTRAINTS: usize = 50_000;
        let constraint_count = msg.constraint_set.constraints.len();

        if constraint_count > MAX_ONTOLOGY_CONSTRAINTS {
            warn!(
                "OntologyConstraintActor: Rejecting {} constraints (cap: {}). Surplus axioms truncated.",
                constraint_count, MAX_ONTOLOGY_CONSTRAINTS
            );
            // Truncate to cap — take the first MAX constraints, mark event
            let mut truncated_set = msg.constraint_set.clone();
            truncated_set.constraints.truncate(MAX_ONTOLOGY_CONSTRAINTS);
            let truncated_count = constraint_count - MAX_ONTOLOGY_CONSTRAINTS;
            warn!(
                "physics.constraints_truncated: {} constraints dropped",
                truncated_count
            );
            // Continue with truncated set
            let constraint_count = truncated_set.constraints.len();
            match msg.merge_mode {
                ConstraintMergeMode::Replace => {
                    self.ontology_constraints = truncated_set.constraints.clone();
                    info!(
                        "OntologyConstraintActor: Replaced with {} constraints (truncated from {})",
                        self.ontology_constraints.len(), constraint_count + truncated_count
                    );
                }
                _ => {
                    // For merge modes, also check total after merge
                    self.ontology_constraints.extend(truncated_set.constraints.clone());
                    if self.ontology_constraints.len() > MAX_ONTOLOGY_CONSTRAINTS {
                        self.ontology_constraints.truncate(MAX_ONTOLOGY_CONSTRAINTS);
                        warn!("OntologyConstraintActor: Post-merge truncation to {}", MAX_ONTOLOGY_CONSTRAINTS);
                    }
                }
            }
            self.constraint_buffer = truncated_set.to_gpu_data();
            self.stats.active_ontology_constraints = self
                .ontology_constraints.iter().filter(|c| c.active).count() as u32;
            return Ok(());
        }

        match msg.merge_mode {
            ConstraintMergeMode::Replace => {

                self.ontology_constraints = msg.constraint_set.constraints.clone();
                info!(
                    "OntologyConstraintActor: Replaced all constraints with {} new constraints",
                    self.ontology_constraints.len()
                );
            }
            ConstraintMergeMode::Merge => {
                
                let existing_count = self.ontology_constraints.len();
                self.ontology_constraints
                    .extend(msg.constraint_set.constraints.clone());
                info!("OntologyConstraintActor: Merged {} new constraints with {} existing (total: {})",
                      constraint_count, existing_count, self.ontology_constraints.len());
            }
            ConstraintMergeMode::AddIfNoConflict => {
                
                let initial_count = self.ontology_constraints.len();
                for constraint in msg.constraint_set.constraints.clone() {
                    
                    let has_conflict = self.ontology_constraints.iter().any(|existing| {
                        existing.node_indices == constraint.node_indices
                            && existing.kind == constraint.kind
                    });

                    if !has_conflict {
                        self.ontology_constraints.push(constraint);
                    }
                }
                let added_count = self.ontology_constraints.len() - initial_count;
                info!(
                    "OntologyConstraintActor: Added {} non-conflicting constraints (skipped {})",
                    added_count,
                    constraint_count - added_count
                );
            }
        }

        // ADR-070 D1.4: Post-merge cap enforcement
        if self.ontology_constraints.len() > MAX_ONTOLOGY_CONSTRAINTS {
            let surplus = self.ontology_constraints.len() - MAX_ONTOLOGY_CONSTRAINTS;
            self.ontology_constraints.truncate(MAX_ONTOLOGY_CONSTRAINTS);
            warn!(
                "OntologyConstraintActor: Post-merge cap enforced, dropped {} surplus constraints",
                surplus
            );
        }

        self.constraint_buffer = msg.constraint_set.to_gpu_data();

        self.stats.active_ontology_constraints = self
            .ontology_constraints
            .iter()
            .filter(|c| c.active)
            .count() as u32;


        if self.gpu_initialized && self.shared_context.is_some() {
            match self.upload_constraints_to_gpu() {
                Ok(_) => {
                    info!("OntologyConstraintActor: Uploaded {} constraints via ApplyOntologyConstraints",
                          self.constraint_buffer.len());
                }
                Err(e) => {
                    warn!("OntologyConstraintActor: GPU upload failed: {}", e);
                    self.stats.gpu_failure_count += 1;
                }
            }
        }

        Ok(())
    }
}

#[derive(Message)]
#[rtype(result = "Result<(), String>")]
pub struct UpdateOntologyConstraints {
    pub axioms: Vec<OWLAxiom>,
}

impl Handler<UpdateOntologyConstraints> for OntologyConstraintActor {
    type Result = Result<(), String>;

    fn handle(&mut self, msg: UpdateOntologyConstraints, _ctx: &mut Self::Context) -> Self::Result {
        self.update_constraints(&msg.axioms)
    }
}

#[derive(Message)]
#[rtype(result = "Result<OntologyConstraintStats, String>")]
pub struct GetOntologyStats;

impl Handler<GetOntologyStats> for OntologyConstraintActor {
    type Result = Result<OntologyConstraintStats, String>;

    fn handle(&mut self, _msg: GetOntologyStats, _ctx: &mut Self::Context) -> Self::Result {
        Ok(self.get_ontology_stats())
    }
}

impl Handler<GetOntologyConstraintStats> for OntologyConstraintActor {
    type Result = Result<crate::actors::messages::OntologyConstraintStats, String>;

    fn handle(
        &mut self,
        _msg: GetOntologyConstraintStats,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        info!("OntologyConstraintActor: Received GetOntologyConstraintStats message");

        
        let stats = crate::actors::messages::OntologyConstraintStats {
            total_axioms_processed: self.stats.total_axioms_processed,
            active_ontology_constraints: self.stats.active_ontology_constraints,
            constraint_evaluation_count: self.stats.constraint_evaluation_count,
            last_update_time_ms: self.stats.last_update_time_ms,
            gpu_failure_count: self.stats.gpu_failure_count,
            cpu_fallback_count: self.stats.cpu_fallback_count,
        };

        Ok(stats)
    }
}

/// Handler for setting the ForceComputeActor address for bidirectional communication
impl Handler<SetForceComputeAddr> for OntologyConstraintActor {
    type Result = ();

    fn handle(&mut self, msg: SetForceComputeAddr, _ctx: &mut Self::Context) -> Self::Result {
        info!("OntologyConstraintActor: Received ForceComputeActor address for constraint synchronization");
        self.force_compute_addr = Some(msg.addr);

        // If we already have constraints buffered, send them immediately
        if !self.constraint_buffer.is_empty() {
            self.notify_force_compute_actor();
        }
    }
}

impl Handler<SetSharedGPUContext> for OntologyConstraintActor {
    type Result = Result<(), String>;

    fn handle(&mut self, msg: SetSharedGPUContext, _ctx: &mut Self::Context) -> Self::Result {
        info!("OntologyConstraintActor: Received SharedGPUContext from ResourceActor");

        self.shared_context = Some(msg.context);

        match self.initialize_gpu() {
            Ok(_) => {
                info!("OntologyConstraintActor: GPU initialization successful");
                Ok(())
            }
            Err(e) => {
                warn!("OntologyConstraintActor: GPU initialization failed: {}", e);
                
                Ok(())
            }
        }
    }
}

impl Handler<GetConstraintStats> for OntologyConstraintActor {
    type Result = Result<ConstraintStats, String>;

    fn handle(&mut self, _msg: GetConstraintStats, _ctx: &mut Self::Context) -> Self::Result {

        let mut stats = ConstraintStats {
            total_constraints: self.ontology_constraints.len(),
            active_constraints: self.stats.active_ontology_constraints as usize,
            constraint_groups: std::collections::HashMap::new(),
            ontology_constraints: self.ontology_constraints.len(),
            user_constraints: 0,
        };


        stats.constraint_groups.insert(
            "ontology_derived".to_string(),
            self.ontology_constraints.len(),
        );

        Ok(stats)
    }
}

/// Handler for GetConstraintBuffer - provides GPU-ready constraint data
/// This is the key integration point for P0-2: it returns the constraint_buffer
/// that ForceComputeActor needs to upload to GPU via UnifiedGPUCompute::upload_constraints()
impl Handler<crate::actors::messages::GetConstraintBuffer> for OntologyConstraintActor {
    type Result = Result<Vec<ConstraintData>, String>;

    fn handle(&mut self, _msg: crate::actors::messages::GetConstraintBuffer, _ctx: &mut Self::Context) -> Self::Result {
        debug!("OntologyConstraintActor: Providing constraint buffer for GPU upload ({} constraints)",
               self.constraint_buffer.len());

        // Return a clone of the constraint buffer for GPU upload
        // This buffer contains ConstraintData structs ready for GPU processing
        Ok(self.constraint_buffer.clone())
    }
}

impl Handler<UpdateConstraints> for OntologyConstraintActor {
    type Result = Result<(), String>;

    fn handle(&mut self, msg: UpdateConstraints, _ctx: &mut Self::Context) -> Self::Result {
        info!("OntologyConstraintActor: Received UpdateConstraints message");

        
        let constraints =
            match serde_json::from_value::<Vec<Constraint>>(msg.constraint_data.clone()) {
                Ok(constraints) => constraints,
                Err(e) => {
                    
                    match serde_json::from_value::<ConstraintSet>(msg.constraint_data) {
                        Ok(constraint_set) => constraint_set.constraints,
                        Err(_) => {
                            error!(
                                "OntologyConstraintActor: Failed to parse constraint_data: {}",
                                e
                            );
                            return Err(format!("Failed to parse constraints: {}", e));
                        }
                    }
                }
            };

        self.ontology_constraints = constraints;
        self.constraint_buffer = self
            .ontology_constraints
            .iter()
            .filter(|c| c.active)
            .map(|c| ConstraintData::from_constraint(c))
            .collect();

        if self.gpu_initialized && self.shared_context.is_some() {
            self.upload_constraints_to_gpu()?;
        }

        Ok(())
    }
}

impl Handler<InitializeGPU> for OntologyConstraintActor {
    type Result = Result<(), String>;

    fn handle(&mut self, msg: InitializeGPU, _ctx: &mut Self::Context) -> Self::Result {
        info!("OntologyConstraintActor: InitializeGPU received");


        self.gpu_state.num_nodes = msg.graph.nodes.len() as u32;
        self.gpu_state.num_edges = msg.graph.edges.len() as u32;

        info!(
            "OntologyConstraintActor: Graph dimensions stored - {} nodes, {} edges",
            self.gpu_state.num_nodes, self.gpu_state.num_edges
        );

        Ok(())
    }
}

impl Handler<AdjustConstraintWeights> for OntologyConstraintActor {
    type Result = Result<serde_json::Value, String>;

    fn handle(&mut self, msg: AdjustConstraintWeights, _ctx: &mut Self::Context) -> Self::Result {
        let global_strength = msg.global_strength.clamp(0.0, 1.0);
        info!(
            "OntologyConstraintActor: Adjusting constraint weights with global_strength={:.3}",
            global_strength
        );

        let mut adjusted_count = 0u32;
        for constraint in &mut self.ontology_constraints {
            constraint.weight *= global_strength;
            adjusted_count += 1;
        }

        // Rebuild constraint buffer with adjusted weights
        self.constraint_buffer = self
            .ontology_constraints
            .iter()
            .filter(|c| c.active)
            .map(|c| ConstraintData::from_constraint(c))
            .collect();

        // Re-upload to GPU if initialized
        if self.gpu_initialized && self.shared_context.is_some() {
            if let Err(e) = self.upload_constraints_to_gpu() {
                warn!("OntologyConstraintActor: GPU re-upload after weight adjustment failed: {}", e);
                self.stats.gpu_failure_count += 1;
            }
        }

        // Notify ForceComputeActor about the updated buffer
        self.notify_force_compute_actor();

        self.stats.active_ontology_constraints = self
            .ontology_constraints
            .iter()
            .filter(|c| c.active)
            .count() as u32;

        Ok(serde_json::json!({
            "success": true,
            "appliedStrength": global_strength,
            "adjustedConstraints": adjusted_count,
            "activeConstraints": self.stats.active_ontology_constraints
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::ontology_constraints::OWLAxiomType;

    #[test]
    fn test_actor_creation() {
        let actor = OntologyConstraintActor::new();
        assert_eq!(actor.ontology_constraints.len(), 0);
        assert_eq!(actor.constraint_buffer.len(), 0);
        assert!(!actor.gpu_initialized);
    }

    #[test]
    fn test_stats_default() {
        let stats = OntologyConstraintStats::default();
        assert_eq!(stats.total_axioms_processed, 0);
        assert_eq!(stats.active_ontology_constraints, 0);
        assert_eq!(stats.gpu_failure_count, 0);
    }

    #[test]
    fn test_constraint_buffer_conversion() {
        let mut actor = OntologyConstraintActor::new();

        let constraints = vec![
            Constraint::fixed_position(0, 10.0, 20.0, 30.0),
            Constraint::separation(1, 2, 50.0),
        ];

        actor.ontology_constraints = constraints;
        actor.constraint_buffer = actor
            .ontology_constraints
            .iter()
            .map(|c| ConstraintData::from_constraint(c))
            .collect();

        assert_eq!(actor.constraint_buffer.len(), 2);
    }
}
