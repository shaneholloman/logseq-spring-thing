//! Layout mode enum for graph visualization
//!
//! Domain-level layout mode selection. The full LayoutModeConfig with
//! algorithm parameters lives in the main crate's `src/layout/types.rs`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LayoutMode {
    /// ForceAtlas2 with LinLog (default)
    ForceDirected,
    /// Sugiyama DAG layers
    Hierarchical,
    /// Centrality rings
    Radial,
    /// Graph Laplacian eigenvectors
    Spectral,
    /// Z-axis = timestamp
    Temporal,
    /// ForceAtlas2 + Louvain metanodes
    Clustered,
}

impl Default for LayoutMode {
    fn default() -> Self {
        LayoutMode::ForceDirected
    }
}

impl std::fmt::Display for LayoutMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LayoutMode::ForceDirected => write!(f, "forceDirected"),
            LayoutMode::Hierarchical => write!(f, "hierarchical"),
            LayoutMode::Radial => write!(f, "radial"),
            LayoutMode::Spectral => write!(f, "spectral"),
            LayoutMode::Temporal => write!(f, "temporal"),
            LayoutMode::Clustered => write!(f, "clustered"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default() {
        assert_eq!(LayoutMode::default(), LayoutMode::ForceDirected);
    }

    #[test]
    fn test_serde_round_trip() {
        let mode = LayoutMode::Hierarchical;
        let json = serde_json::to_string(&mode).unwrap();
        assert_eq!(json, "\"hierarchical\"");
        let back: LayoutMode = serde_json::from_str(&json).unwrap();
        assert_eq!(back, mode);
    }

    #[test]
    fn test_display() {
        assert_eq!(LayoutMode::ForceDirected.to_string(), "forceDirected");
        assert_eq!(LayoutMode::Clustered.to_string(), "clustered");
    }
}
