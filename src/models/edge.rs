use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// GPU-friendly edge type for semantic pipeline (ADR-014 Phase 2).
/// Maps relationship strings from Neo4j/OntologyParser to a compact u8
/// discriminant suitable for GPU buffers and spring-force differentiation.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SemanticEdgeType {
    /// [[wikilink]] — standard spring
    ExplicitLink = 0,
    /// is-subclass-of (rdfs:subClassOf) — strong hierarchy pull
    Hierarchical = 1,
    /// has-part, is-part-of — medium clustering
    Structural = 2,
    /// requires, depends-on, enables — medium clustering
    Dependency = 3,
    /// relates-to — gentle grouping
    Associative = 4,
    /// bridges-to, bridges-from — cross-domain (weaker)
    Bridge = 5,
    /// shared namespace prefix grouping
    Namespace = 6,
    /// whelk reasoner output
    Inferred = 7,
    /// 2-hop materialized transitive edge (very weak spring)
    Materialized2Hop = 8,
    /// 3-hop materialized transitive edge (barely perceptible)
    Materialized3Hop = 9,
}

impl SemanticEdgeType {
    /// Convert a Neo4j relation_type or OntologyParser relationship string
    /// into the corresponding SemanticEdgeType variant.
    pub fn from_relation_type(s: &str) -> Self {
        match s {
            "is_subclass_of" | "subclass_of" | "hierarchical" | "SUBCLASS_OF" => {
                Self::Hierarchical
            }
            "has_part" | "is_part_of" | "structural" => Self::Structural,
            "requires" | "depends_on" | "enables" | "dependency" => Self::Dependency,
            "relates_to" | "associative" => Self::Associative,
            "bridges_to" | "bridges_from" | "bridge" => Self::Bridge,
            "namespace" => Self::Namespace,
            "inferred" => Self::Inferred,
            "MATERIALIZED_2HOP" | "materialized_2hop" => Self::Materialized2Hop,
            "MATERIALIZED_3HOP" | "materialized_3hop" => Self::Materialized3Hop,
            _ => Self::ExplicitLink,
        }
    }

    /// Spring strength multiplier for force-directed layout.
    /// Higher values produce tighter springs (shorter rest length).
    pub fn spring_multiplier(&self) -> f32 {
        match self {
            Self::Hierarchical => 2.0,
            Self::Structural | Self::Dependency => 1.5,
            Self::ExplicitLink | Self::Associative => 1.0,
            Self::Inferred => 0.8,
            Self::Bridge => 0.5,
            Self::Namespace => 0.3,
            Self::Materialized2Hop => 0.15,
            Self::Materialized3Hop => 0.08,
        }
    }

    /// Convert from raw u8 discriminant (e.g. read back from GPU buffer).
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::ExplicitLink,
            1 => Self::Hierarchical,
            2 => Self::Structural,
            3 => Self::Dependency,
            4 => Self::Associative,
            5 => Self::Bridge,
            6 => Self::Namespace,
            7 => Self::Inferred,
            8 => Self::Materialized2Hop,
            9 => Self::Materialized3Hop,
            _ => Self::ExplicitLink,
        }
    }
}

impl Default for SemanticEdgeType {
    fn default() -> Self {
        Self::ExplicitLink
    }
}

impl std::fmt::Display for SemanticEdgeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ExplicitLink => write!(f, "explicit_link"),
            Self::Hierarchical => write!(f, "hierarchical"),
            Self::Structural => write!(f, "structural"),
            Self::Dependency => write!(f, "dependency"),
            Self::Associative => write!(f, "associative"),
            Self::Bridge => write!(f, "bridge"),
            Self::Namespace => write!(f, "namespace"),
            Self::Inferred => write!(f, "inferred"),
            Self::Materialized2Hop => write!(f, "materialized_2hop"),
            Self::Materialized3Hop => write!(f, "materialized_3hop"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Edge {
    pub id: String, 
    pub source: u32,
    pub target: u32,
    pub weight: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge_type: Option<String>,

    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owl_property_iri: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
}

impl Edge {
    pub fn new(source: u32, target: u32, weight: f32) -> Self {
        
        let id = format!("{}-{}", source, target);
        Self {
            id,
            source,
            target,
            weight,
            edge_type: None,
            owl_property_iri: None,
            metadata: None,
        }
    }

    
    pub fn with_owl_property_iri(mut self, iri: String) -> Self {
        self.owl_property_iri = Some(iri);
        self
    }

    
    pub fn with_edge_type(mut self, edge_type: String) -> Self {
        self.edge_type = Some(edge_type);
        self
    }

    
    pub fn with_metadata(mut self, metadata: HashMap<String, String>) -> Self {
        self.metadata = Some(metadata);
        self
    }


    pub fn add_metadata(mut self, key: String, value: String) -> Self {
        if let Some(ref mut map) = self.metadata {
            map.insert(key, value);
        } else {
            let mut map = HashMap::new();
            map.insert(key, value);
            self.metadata = Some(map);
        }
        self
    }

    /// Derive the SemanticEdgeType from this edge's `edge_type` string field.
    /// Falls back to `ExplicitLink` when `edge_type` is None.
    pub fn semantic_edge_type(&self) -> SemanticEdgeType {
        match &self.edge_type {
            Some(t) => SemanticEdgeType::from_relation_type(t),
            None => SemanticEdgeType::ExplicitLink,
        }
    }
}
