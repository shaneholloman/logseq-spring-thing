use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LayoutMode {
    ForceDirected,   // ForceAtlas2 with LinLog (default)
    Hierarchical,    // Sugiyama DAG layers
    Radial,          // Centrality rings
    Spectral,        // Graph Laplacian eigenvectors
    Temporal,        // Z-axis = timestamp
    Clustered,       // ForceAtlas2 + Louvain metanodes
}

impl Default for LayoutMode {
    fn default() -> Self { LayoutMode::ForceDirected }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutModeConfig {
    pub mode: LayoutMode,
    pub transition_duration_ms: u32,
    // ForceAtlas2 specific
    pub scaling_ratio: f32,
    pub gravity: f32,
    pub lin_log_mode: bool,
    pub dissuade_hubs: bool,
    pub barnes_hut_theta: f32,
    pub strong_gravity: bool,
    // Hierarchical specific
    pub layer_spacing: f32,
    pub node_spacing: f32,
    pub hierarchy_direction: String, // "top_down", "left_right", "radial"
    // Radial specific
    pub centrality_measure: String, // "pagerank", "degree", "betweenness"
    pub ring_count: u32,
    // Zone constraints
    pub zones: Vec<ConstraintZone>,
    // Graph separation
    pub graph_separation_x: f32,
}

impl Default for LayoutModeConfig {
    fn default() -> Self {
        Self {
            mode: LayoutMode::ForceDirected,
            transition_duration_ms: 500,
            scaling_ratio: 10.0,
            gravity: 1.0,
            lin_log_mode: true,
            dissuade_hubs: true,
            barnes_hut_theta: 0.5,
            strong_gravity: false,
            layer_spacing: 150.0,
            node_spacing: 80.0,
            hierarchy_direction: "top_down".to_string(),
            centrality_measure: "pagerank".to_string(),
            ring_count: 8,
            zones: vec![],
            graph_separation_x: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConstraintZone {
    pub id: String,
    pub center: [f32; 3],
    pub radius: f32,
    pub strength: f32,
    pub node_types: Vec<String>,  // e.g. ["owl_class", "ontology_node"]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutStatus {
    pub current_mode: LayoutMode,
    pub transitioning: bool,
    pub transition_progress: f32,
    pub iterations: u64,
    pub converged: bool,
    pub kinetic_energy: f64,
    pub available_modes: Vec<LayoutMode>,
}
