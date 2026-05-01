use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Named force presets per ADR-069 D2.
///
/// Each preset defines empirically-tuned constants for a known graph topology.
/// Presets are versioned; switching produces a `physics.preset_change` audit event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForcePreset {
    Default,
    LogseqSmall,
    LogseqLarge,
    CodeRepo,
    ResearchWiki,
}

impl ForcePreset {
    /// Auto-select a preset based on graph characteristics (ADR-069 D8).
    pub fn auto_select(graph_kind: &str, node_count: usize, has_karpathy_structure: bool) -> Self {
        match graph_kind {
            "codebase" => Self::CodeRepo,
            "knowledge" if has_karpathy_structure => Self::ResearchWiki,
            "knowledge" if node_count > 5_000 => Self::LogseqLarge,
            "knowledge" => Self::LogseqSmall,
            _ => Self::Default,
        }
    }
}

/// Full preset configuration loaded from TOML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetConfig {
    pub name: ForcePreset,
    pub version: u32,
    pub global: GlobalSimConfig,
    #[serde(default)]
    pub edge_kinds: HashMap<String, EdgeKindForceConfig>,
    #[serde(default)]
    pub node_kinds: HashMap<String, NodeKindPhysicsConfig>,
    pub stability: StabilityConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalSimConfig {
    pub gravity: f32,
    pub central_gravity: f32,
    pub damping: f32,
    pub dt: f32,
    pub spring_k: f32,
    pub rest_length: f32,
    pub repel_k: f32,
    pub max_velocity: f32,
    pub max_force: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeKindForceConfig {
    pub spring_k: f32,
    pub rest_length: f32,
    pub repulsion_strength: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeKindPhysicsConfig {
    pub mass: f32,
    pub charge: f32,
    pub max_velocity: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StabilityConfig {
    pub velocity_epsilon: f32,
    pub force_epsilon: f32,
    pub max_iterations: u32,
}

impl PresetConfig {
    /// Load a preset from a TOML string.
    pub fn from_toml(content: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(content)
    }

    /// Load a preset from a file path.
    pub fn from_file(path: &Path) -> Result<Self, PresetLoadError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| PresetLoadError::Io(e.to_string()))?;
        Self::from_toml(&content).map_err(|e| PresetLoadError::Parse(e.to_string()))
    }

    /// Load a built-in preset embedded at compile time.
    ///
    /// The five canonical presets are baked into the binary via `include_str!`
    /// so they are available without filesystem access (important for WASM
    /// and containerised deployments).
    pub fn load_builtin_preset(preset: ForcePreset) -> Self {
        let toml_str = match preset {
            ForcePreset::Default => include_str!("../presets/default.toml"),
            ForcePreset::LogseqSmall => include_str!("../presets/logseq_small.toml"),
            ForcePreset::LogseqLarge => include_str!("../presets/logseq_large.toml"),
            ForcePreset::CodeRepo => include_str!("../presets/code_repo.toml"),
            ForcePreset::ResearchWiki => include_str!("../presets/research_wiki.toml"),
        };
        // These are compile-time-embedded files validated by tests; unwrap is safe.
        toml::from_str(toml_str)
            .unwrap_or_else(|e| panic!("built-in preset {:?} failed to parse: {}", preset, e))
    }

    /// Load all built-in presets into a map keyed by `ForcePreset`.
    pub fn load_all_builtin_presets() -> HashMap<ForcePreset, Self> {
        let mut map = HashMap::new();
        for preset in [
            ForcePreset::Default,
            ForcePreset::LogseqSmall,
            ForcePreset::LogseqLarge,
            ForcePreset::CodeRepo,
            ForcePreset::ResearchWiki,
        ] {
            map.insert(preset, Self::load_builtin_preset(preset));
        }
        map
    }

    /// Build the default preset (existing VisionClaw tuning).
    pub fn default_preset() -> Self {
        Self {
            name: ForcePreset::Default,
            version: 1,
            global: GlobalSimConfig {
                gravity: 0.0001,
                central_gravity: 0.01,
                damping: 0.85,
                dt: 0.016,
                spring_k: 0.5,
                rest_length: 1.0,
                repel_k: 100.0,
                max_velocity: 5.0,
                max_force: 10.0,
            },
            edge_kinds: HashMap::new(),
            node_kinds: HashMap::new(),
            stability: StabilityConfig {
                velocity_epsilon: 0.01,
                force_epsilon: 0.001,
                max_iterations: 50_000,
            },
        }
    }

    /// Build the LogseqLarge preset with matryca-derived constants
    /// POST-CALIBRATION (ADR-069 D3). Matryca pixel-space values are
    /// scaled to VC metre-space: gravity /100, spring_length /100, etc.
    pub fn logseq_large_preset() -> Self {
        let mut edge_kinds = HashMap::new();

        // Block-parent edges: tight hierarchy
        edge_kinds.insert(
            "block_parent".to_string(),
            EdgeKindForceConfig {
                spring_k: 0.85,
                rest_length: 0.3, // matryca 30px → 0.3m
                repulsion_strength: 0.0,
            },
        );
        // WikiLink: medium attraction
        edge_kinds.insert(
            "wiki_link".to_string(),
            EdgeKindForceConfig {
                spring_k: 0.5,
                rest_length: 1.0, // matryca 100px → 1.0m
                repulsion_strength: 0.0,
            },
        );
        // Block-ref: weak cross-link
        edge_kinds.insert(
            "block_ref".to_string(),
            EdgeKindForceConfig {
                spring_k: 0.3,
                rest_length: 1.5,
                repulsion_strength: 0.0,
            },
        );

        Self {
            name: ForcePreset::LogseqLarge,
            version: 1,
            global: GlobalSimConfig {
                // matryca: gravity=-50 (px) → -0.5 (m)
                gravity: -0.5,
                // matryca: central_gravity=0.01
                central_gravity: 0.01,
                // matryca: damping=0.4
                damping: 0.4,
                dt: 0.016,
                // matryca: spring_strength=0.08
                spring_k: 0.08,
                // matryca: spring_length=100 (px) → 1.0 (m)
                rest_length: 1.0,
                repel_k: 80.0,
                max_velocity: 5.0,
                max_force: 10.0,
            },
            edge_kinds,
            node_kinds: HashMap::new(),
            stability: StabilityConfig {
                velocity_epsilon: 0.1, // tolerates higher residual (force-of-life)
                force_epsilon: 0.01,
                max_iterations: 50_000,
            },
        }
    }

    /// Build the CodeRepo preset for source-code-derived graphs.
    pub fn code_repo_preset() -> Self {
        let mut edge_kinds = HashMap::new();

        edge_kinds.insert(
            "calls".to_string(),
            EdgeKindForceConfig {
                spring_k: 0.6,
                rest_length: 0.8,
                repulsion_strength: 0.0,
            },
        );
        edge_kinds.insert(
            "imports".to_string(),
            EdgeKindForceConfig {
                spring_k: 0.7,
                rest_length: 0.6,
                repulsion_strength: 0.0,
            },
        );
        edge_kinds.insert(
            "inherits_from".to_string(),
            EdgeKindForceConfig {
                spring_k: 0.8,
                rest_length: 0.5,
                repulsion_strength: 0.0,
            },
        );

        Self {
            name: ForcePreset::CodeRepo,
            version: 1,
            global: GlobalSimConfig {
                gravity: 0.0001,
                central_gravity: 0.02,
                damping: 0.9,
                dt: 0.016,
                spring_k: 0.6,
                rest_length: 0.8,
                repel_k: 120.0,
                max_velocity: 4.0,
                max_force: 8.0,
            },
            edge_kinds,
            node_kinds: HashMap::new(),
            stability: StabilityConfig {
                velocity_epsilon: 0.01, // tighter convergence
                force_epsilon: 0.001,
                max_iterations: 50_000,
            },
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PresetLoadError {
    #[error("IO error: {0}")]
    Io(String),
    #[error("Parse error: {0}")]
    Parse(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_select_codebase() {
        assert_eq!(
            ForcePreset::auto_select("codebase", 500, false),
            ForcePreset::CodeRepo
        );
    }

    #[test]
    fn auto_select_logseq_large() {
        assert_eq!(
            ForcePreset::auto_select("knowledge", 10_000, false),
            ForcePreset::LogseqLarge
        );
    }

    #[test]
    fn auto_select_logseq_small() {
        assert_eq!(
            ForcePreset::auto_select("knowledge", 500, false),
            ForcePreset::LogseqSmall
        );
    }

    #[test]
    fn auto_select_karpathy() {
        assert_eq!(
            ForcePreset::auto_select("knowledge", 50_000, true),
            ForcePreset::ResearchWiki
        );
    }

    #[test]
    fn default_preset_valid_ranges() {
        let p = PresetConfig::default_preset();
        assert!(p.global.damping > 0.0 && p.global.damping < 1.0);
        assert!(p.global.max_velocity > 0.0);
        assert!(p.global.max_force > 0.0);
    }

    #[test]
    fn logseq_large_calibrated_values() {
        let p = PresetConfig::logseq_large_preset();
        // gravity should be ~100x smaller than matryca's -50
        assert!(p.global.gravity.abs() < 1.0);
        // rest_length should be ~100x smaller than matryca's 100
        assert!(p.global.rest_length <= 2.0);
    }

    #[test]
    fn toml_roundtrip() {
        let p = PresetConfig::default_preset();
        let toml_str = toml::to_string(&p).unwrap();
        let back: PresetConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(back.name, ForcePreset::Default);
    }

    // ── Built-in TOML file loading tests ──

    #[test]
    fn builtin_default_loads() {
        let p = PresetConfig::load_builtin_preset(ForcePreset::Default);
        assert_eq!(p.name, ForcePreset::Default);
        assert_eq!(p.version, 1);
        assert!(p.edge_kinds.is_empty(), "default preset should have no edge_kind overrides");
    }

    #[test]
    fn builtin_logseq_small_loads() {
        let p = PresetConfig::load_builtin_preset(ForcePreset::LogseqSmall);
        assert_eq!(p.name, ForcePreset::LogseqSmall);
        assert_eq!(p.version, 1);
        assert!(p.edge_kinds.contains_key("block_parent"));
        assert!(p.edge_kinds.contains_key("wiki_link"));
        assert!(p.edge_kinds.contains_key("block_ref"));
        assert!(p.edge_kinds.contains_key("tagged_with"));
    }

    #[test]
    fn builtin_logseq_large_loads() {
        let p = PresetConfig::load_builtin_preset(ForcePreset::LogseqLarge);
        assert_eq!(p.name, ForcePreset::LogseqLarge);
        assert_eq!(p.version, 1);
        // Verify matryca calibration: gravity should be ~100x smaller than pixel-space -50
        assert!(p.global.gravity.abs() < 1.0, "gravity should be metre-scale");
        assert!(p.global.rest_length <= 2.0, "rest_length should be metre-scale");
        // Verify edge_kinds are present
        assert!(p.edge_kinds.contains_key("block_parent"));
        assert!(p.edge_kinds.contains_key("wiki_link"));
        assert!(p.edge_kinds.contains_key("block_ref"));
        assert!(p.edge_kinds.contains_key("disjoint_with"));
        // Verify matryca-derived block_parent rest_length
        let bp = &p.edge_kinds["block_parent"];
        assert!((bp.rest_length - 0.3).abs() < 0.01, "block_parent rest_length should be 0.3m (matryca 30px /100)");
    }

    #[test]
    fn builtin_code_repo_loads() {
        let p = PresetConfig::load_builtin_preset(ForcePreset::CodeRepo);
        assert_eq!(p.name, ForcePreset::CodeRepo);
        assert_eq!(p.version, 1);
        // Code repo should have structural + behavioral + dependency edge kinds
        assert!(p.edge_kinds.contains_key("contains"));
        assert!(p.edge_kinds.contains_key("inherits_from"));
        assert!(p.edge_kinds.contains_key("calls"));
        assert!(p.edge_kinds.contains_key("imports"));
        // Inheritance should be stronger than generic calls
        assert!(p.edge_kinds["inherits_from"].spring_k > p.edge_kinds["calls"].spring_k);
    }

    #[test]
    fn builtin_research_wiki_loads() {
        let p = PresetConfig::load_builtin_preset(ForcePreset::ResearchWiki);
        assert_eq!(p.name, ForcePreset::ResearchWiki);
        assert_eq!(p.version, 1);
        assert!(p.edge_kinds.contains_key("wiki_link"));
        assert!(p.edge_kinds.contains_key("cited_by"));
        assert!(p.edge_kinds.contains_key("bridges_to"));
        // bridges_to should have a long rest length (cross-domain separation)
        assert!(p.edge_kinds["bridges_to"].rest_length > 2.0);
        // disjoint_with should be repulsive
        let dw = &p.edge_kinds["disjoint_with"];
        assert!(dw.spring_k < 0.0, "disjoint_with should have negative spring_k");
        assert!(dw.repulsion_strength > 0.0);
    }

    #[test]
    fn all_builtin_presets_load() {
        let all = PresetConfig::load_all_builtin_presets();
        assert_eq!(all.len(), 5);
        assert!(all.contains_key(&ForcePreset::Default));
        assert!(all.contains_key(&ForcePreset::LogseqSmall));
        assert!(all.contains_key(&ForcePreset::LogseqLarge));
        assert!(all.contains_key(&ForcePreset::CodeRepo));
        assert!(all.contains_key(&ForcePreset::ResearchWiki));
    }

    #[test]
    fn all_builtin_presets_have_valid_physics_ranges() {
        for preset in [
            ForcePreset::Default,
            ForcePreset::LogseqSmall,
            ForcePreset::LogseqLarge,
            ForcePreset::CodeRepo,
            ForcePreset::ResearchWiki,
        ] {
            let p = PresetConfig::load_builtin_preset(preset);
            // damping must be in (0, 1)
            assert!(
                p.global.damping > 0.0 && p.global.damping < 1.0,
                "{:?}: damping {} out of range (0,1)",
                preset,
                p.global.damping
            );
            // dt must be positive
            assert!(p.global.dt > 0.0, "{:?}: dt must be positive", preset);
            // max_velocity and max_force must be positive
            assert!(p.global.max_velocity > 0.0, "{:?}: max_velocity must be positive", preset);
            assert!(p.global.max_force > 0.0, "{:?}: max_force must be positive", preset);
            // stability thresholds must be positive
            assert!(p.stability.velocity_epsilon > 0.0, "{:?}: velocity_epsilon must be positive", preset);
            assert!(p.stability.force_epsilon > 0.0, "{:?}: force_epsilon must be positive", preset);
            assert!(p.stability.max_iterations > 0, "{:?}: max_iterations must be > 0", preset);
        }
    }

    #[test]
    fn builtin_matches_programmatic_default() {
        let from_toml = PresetConfig::load_builtin_preset(ForcePreset::Default);
        let from_code = PresetConfig::default_preset();
        assert!((from_toml.global.gravity - from_code.global.gravity).abs() < 1e-6);
        assert!((from_toml.global.central_gravity - from_code.global.central_gravity).abs() < 1e-6);
        assert!((from_toml.global.damping - from_code.global.damping).abs() < 1e-6);
        assert!((from_toml.global.spring_k - from_code.global.spring_k).abs() < 1e-6);
        assert!((from_toml.global.rest_length - from_code.global.rest_length).abs() < 1e-6);
        assert!((from_toml.global.repel_k - from_code.global.repel_k).abs() < 1e-6);
    }

    #[test]
    fn builtin_matches_programmatic_logseq_large() {
        let from_toml = PresetConfig::load_builtin_preset(ForcePreset::LogseqLarge);
        let from_code = PresetConfig::logseq_large_preset();
        assert!((from_toml.global.gravity - from_code.global.gravity).abs() < 1e-6);
        assert!((from_toml.global.damping - from_code.global.damping).abs() < 1e-6);
        assert!((from_toml.global.spring_k - from_code.global.spring_k).abs() < 1e-6);
        // Verify edge_kinds match for the three matryca-derived edges
        for key in ["block_parent", "wiki_link", "block_ref"] {
            let toml_ek = &from_toml.edge_kinds[key];
            let code_ek = &from_code.edge_kinds[key];
            assert!(
                (toml_ek.spring_k - code_ek.spring_k).abs() < 1e-6,
                "edge_kind {}: spring_k mismatch",
                key
            );
            assert!(
                (toml_ek.rest_length - code_ek.rest_length).abs() < 1e-6,
                "edge_kind {}: rest_length mismatch",
                key
            );
        }
    }

    #[test]
    fn builtin_matches_programmatic_code_repo() {
        let from_toml = PresetConfig::load_builtin_preset(ForcePreset::CodeRepo);
        let from_code = PresetConfig::code_repo_preset();
        assert!((from_toml.global.gravity - from_code.global.gravity).abs() < 1e-6);
        assert!((from_toml.global.central_gravity - from_code.global.central_gravity).abs() < 1e-6);
        assert!((from_toml.global.damping - from_code.global.damping).abs() < 1e-6);
        assert!((from_toml.global.spring_k - from_code.global.spring_k).abs() < 1e-6);
        assert!((from_toml.global.repel_k - from_code.global.repel_k).abs() < 1e-6);
        // Verify the three code-repo edge kinds match
        for key in ["calls", "imports", "inherits_from"] {
            let toml_ek = &from_toml.edge_kinds[key];
            let code_ek = &from_code.edge_kinds[key];
            assert!(
                (toml_ek.spring_k - code_ek.spring_k).abs() < 1e-6,
                "edge_kind {}: spring_k mismatch",
                key
            );
            assert!(
                (toml_ek.rest_length - code_ek.rest_length).abs() < 1e-6,
                "edge_kind {}: rest_length mismatch",
                key
            );
        }
    }
}
