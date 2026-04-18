//! Graph type definitions for multi-agent systems and semantic forces

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum GraphType {
    #[default]
    Standard,

    MultiAgent,

    ForceDirected,

    Hierarchical,

    Network,

    Ontology,
}

/// Semantic node types for type-aware physics and pathfinding
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub enum NodeType {
    /// Generic untyped node
    #[default]
    Generic,
    /// Person or individual entity
    Person,
    /// Organization or group
    Organization,
    /// Project or initiative
    Project,
    /// Task or action item
    Task,
    /// Concept or abstract idea
    Concept,
    /// OWL class definition
    Class,
    /// OWL individual instance
    Individual,
    /// Custom user-defined type
    Custom(String),
}

/// Semantic edge types for relationship-aware algorithms
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub enum EdgeType {
    /// Generic untyped edge
    #[default]
    Generic,
    /// Dependency relationship
    Dependency,
    /// Hierarchical parent-child relationship
    Hierarchy,
    /// General association
    Association,
    /// Sequential ordering
    Sequence,
    /// OWL subClassOf relationship
    SubClassOf,
    /// OWL instanceOf relationship
    InstanceOf,
    /// Custom user-defined type
    Custom(String),
}


impl std::fmt::Display for GraphType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GraphType::Standard => write!(f, "standard"),
            GraphType::MultiAgent => write!(f, "multi-agent"),
            GraphType::ForceDirected => write!(f, "force-directed"),
            GraphType::Hierarchical => write!(f, "hierarchical"),
            GraphType::Network => write!(f, "network"),
            GraphType::Ontology => write!(f, "ontology"),
        }
    }
}

impl std::str::FromStr for GraphType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "standard" => Ok(GraphType::Standard),
            "multi-agent" | "multiagent" => Ok(GraphType::MultiAgent),
            "force-directed" | "forcedirected" => Ok(GraphType::ForceDirected),
            "hierarchical" => Ok(GraphType::Hierarchical),
            "network" => Ok(GraphType::Network),
            "ontology" => Ok(GraphType::Ontology),
            _ => Err(format!("Unknown graph type: {}", s)),
        }
    }
}


impl std::fmt::Display for NodeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NodeType::Generic => write!(f, "generic"),
            NodeType::Person => write!(f, "person"),
            NodeType::Organization => write!(f, "organization"),
            NodeType::Project => write!(f, "project"),
            NodeType::Task => write!(f, "task"),
            NodeType::Concept => write!(f, "concept"),
            NodeType::Class => write!(f, "class"),
            NodeType::Individual => write!(f, "individual"),
            NodeType::Custom(s) => write!(f, "{}", s),
        }
    }
}

impl std::str::FromStr for NodeType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "generic" => Ok(NodeType::Generic),
            "person" => Ok(NodeType::Person),
            "organization" | "org" => Ok(NodeType::Organization),
            "project" => Ok(NodeType::Project),
            "task" => Ok(NodeType::Task),
            "concept" => Ok(NodeType::Concept),
            "class" => Ok(NodeType::Class),
            "individual" => Ok(NodeType::Individual),
            _ => Ok(NodeType::Custom(s.to_string())),
        }
    }
}


impl std::fmt::Display for EdgeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EdgeType::Generic => write!(f, "generic"),
            EdgeType::Dependency => write!(f, "dependency"),
            EdgeType::Hierarchy => write!(f, "hierarchy"),
            EdgeType::Association => write!(f, "association"),
            EdgeType::Sequence => write!(f, "sequence"),
            EdgeType::SubClassOf => write!(f, "subClassOf"),
            EdgeType::InstanceOf => write!(f, "instanceOf"),
            EdgeType::Custom(s) => write!(f, "{}", s),
        }
    }
}

// ── Canonical node population classification ──────────────────────────
// ADR-036: Single classification function used by GPU actors, graph state,
// REST handlers, and binary protocol. Case-insensitive matching with all
// known type strings from Neo4j, OWL enrichment, and metadata fallbacks.

/// Population category for graph separation, binary protocol type flags,
/// and client-side rendering. Maps the many ad-hoc node_type strings to
/// three populations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodePopulation {
    Knowledge,
    Ontology,
    Agent,
}

/// Classify a node type string into its population.
///
/// Accepts all known type strings in any case:
/// - Knowledge: "page", "linked_page", "block", "knowledge_node"
/// - Ontology:  "owl_class", "OwlClass", "ontology_node", "owl_individual", "owl_property"
/// - Agent:     "agent", "bot"
/// - Default:   Knowledge (most nodes from Logseq/markdown lack explicit type)
pub fn classify_node_population(node_type: Option<&str>) -> NodePopulation {
    match node_type {
        Some(t) => match t.to_lowercase().as_str() {
            "agent" | "bot" => NodePopulation::Agent,
            "owl_class" | "owlclass" | "ontology_node" | "owl_individual"
            | "owl_property" | "ontologyclass" | "ontologyindividual"
            | "ontologyproperty" => NodePopulation::Ontology,
            "page" | "linked_page" | "block" | "knowledge_node" => NodePopulation::Knowledge,
            unknown => {
                // Silent fall-through was hiding classification mismatches.
                // Log the unknown type once per bucket so stray values surface
                // in ingestion telemetry instead of being quietly absorbed
                // into the Knowledge default.
                log::debug!(
                    "classify_node_population: unknown node_type '{}' — defaulting to Knowledge",
                    unknown
                );
                NodePopulation::Knowledge
            }
        },
        None => NodePopulation::Knowledge,
    }
}

/// Subclassify ontology nodes for binary protocol type flags.
/// Returns which specific ontology type this is, for the wire format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OntologySubtype {
    Class,
    Individual,
    Property,
    Unspecified,
}

pub fn classify_ontology_subtype(node_type: Option<&str>) -> OntologySubtype {
    match node_type {
        Some(t) => match t.to_lowercase().as_str() {
            "owl_class" | "owlclass" | "ontology_node" | "ontologyclass" => OntologySubtype::Class,
            "owl_individual" | "ontologyindividual" => OntologySubtype::Individual,
            "owl_property" | "ontologyproperty" => OntologySubtype::Property,
            _ => OntologySubtype::Unspecified,
        },
        None => OntologySubtype::Unspecified,
    }
}

/// Extract the effective type string from a Node's node_type field or metadata fallbacks.
/// This is the canonical way to get a type string for classification.
/// Works with Node.metadata (HashMap<String, String>).
pub fn effective_node_type<'a>(
    node_type: Option<&'a str>,
    metadata: Option<&'a std::collections::HashMap<String, String>>,
) -> Option<&'a str> {
    node_type
        .or_else(|| metadata.and_then(|m| m.get("type").map(|v| v.as_str())))
        .or_else(|| metadata.and_then(|m| m.get("node_type").map(|v| v.as_str())))
}

impl std::str::FromStr for EdgeType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "generic" => Ok(EdgeType::Generic),
            "dependency" | "depends" => Ok(EdgeType::Dependency),
            "hierarchy" | "parent-child" => Ok(EdgeType::Hierarchy),
            "association" | "assoc" => Ok(EdgeType::Association),
            "sequence" | "seq" => Ok(EdgeType::Sequence),
            "subclassof" | "subclass" => Ok(EdgeType::SubClassOf),
            "instanceof" | "instance" => Ok(EdgeType::InstanceOf),
            _ => Ok(EdgeType::Custom(s.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // ── classify_node_population ─────────────────────────────────────

    // GIVEN: Knowledge-type strings
    // WHEN:  Passed to classify_node_population
    // THEN:  Returns NodePopulation::Knowledge

    #[test]
    fn classify_population_page() {
        assert_eq!(classify_node_population(Some("page")), NodePopulation::Knowledge);
    }

    #[test]
    fn classify_population_linked_page() {
        assert_eq!(classify_node_population(Some("linked_page")), NodePopulation::Knowledge);
    }

    #[test]
    fn classify_population_block() {
        assert_eq!(classify_node_population(Some("block")), NodePopulation::Knowledge);
    }

    #[test]
    fn classify_population_knowledge_node() {
        assert_eq!(classify_node_population(Some("knowledge_node")), NodePopulation::Knowledge);
    }

    // GIVEN: Ontology-type strings (various casings)
    // WHEN:  Passed to classify_node_population
    // THEN:  Returns NodePopulation::Ontology

    #[test]
    fn classify_population_owl_class_lower() {
        assert_eq!(classify_node_population(Some("owl_class")), NodePopulation::Ontology);
    }

    #[test]
    fn classify_population_owl_class_camel() {
        assert_eq!(classify_node_population(Some("OwlClass")), NodePopulation::Ontology);
    }

    #[test]
    fn classify_population_owlclass_lower() {
        assert_eq!(classify_node_population(Some("owlclass")), NodePopulation::Ontology);
    }

    #[test]
    fn classify_population_owlclass_upper() {
        assert_eq!(classify_node_population(Some("OWLCLASS")), NodePopulation::Ontology);
    }

    #[test]
    fn classify_population_ontology_node() {
        assert_eq!(classify_node_population(Some("ontology_node")), NodePopulation::Ontology);
    }

    #[test]
    fn classify_population_owl_individual() {
        assert_eq!(classify_node_population(Some("owl_individual")), NodePopulation::Ontology);
    }

    #[test]
    fn classify_population_owl_property() {
        assert_eq!(classify_node_population(Some("owl_property")), NodePopulation::Ontology);
    }

    #[test]
    fn classify_population_ontology_class_camel() {
        assert_eq!(classify_node_population(Some("OntologyClass")), NodePopulation::Ontology);
    }

    #[test]
    fn classify_population_ontologyclass_lower() {
        assert_eq!(classify_node_population(Some("ontologyclass")), NodePopulation::Ontology);
    }

    // GIVEN: Agent-type strings (various casings)
    // WHEN:  Passed to classify_node_population
    // THEN:  Returns NodePopulation::Agent

    #[test]
    fn classify_population_agent_lower() {
        assert_eq!(classify_node_population(Some("agent")), NodePopulation::Agent);
    }

    #[test]
    fn classify_population_bot_lower() {
        assert_eq!(classify_node_population(Some("bot")), NodePopulation::Agent);
    }

    #[test]
    fn classify_population_agent_title_case() {
        assert_eq!(classify_node_population(Some("Agent")), NodePopulation::Agent);
    }

    #[test]
    fn classify_population_bot_upper() {
        assert_eq!(classify_node_population(Some("BOT")), NodePopulation::Agent);
    }

    // GIVEN: Default/fallback inputs
    // WHEN:  Passed to classify_node_population
    // THEN:  Returns NodePopulation::Knowledge

    #[test]
    fn classify_population_none() {
        assert_eq!(classify_node_population(None), NodePopulation::Knowledge);
    }

    #[test]
    fn classify_population_empty_string() {
        assert_eq!(classify_node_population(Some("")), NodePopulation::Knowledge);
    }

    #[test]
    fn classify_population_unknown_type() {
        assert_eq!(classify_node_population(Some("unknown_type")), NodePopulation::Knowledge);
    }

    #[test]
    fn classify_population_custom_thing() {
        assert_eq!(classify_node_population(Some("custom_thing")), NodePopulation::Knowledge);
    }

    // GIVEN: Case-insensitive inputs for all populations
    // WHEN:  Passed in UPPER CASE
    // THEN:  Still classified correctly

    #[test]
    fn classify_population_page_upper() {
        assert_eq!(classify_node_population(Some("PAGE")), NodePopulation::Knowledge);
    }

    #[test]
    fn classify_population_owl_class_upper() {
        assert_eq!(classify_node_population(Some("OWL_CLASS")), NodePopulation::Ontology);
    }

    #[test]
    fn classify_population_agent_upper() {
        assert_eq!(classify_node_population(Some("AGENT")), NodePopulation::Agent);
    }

    // ── classify_ontology_subtype ────────────────────────────────────

    // GIVEN: Class-type ontology strings
    // WHEN:  Passed to classify_ontology_subtype
    // THEN:  Returns OntologySubtype::Class

    #[test]
    fn classify_subtype_owl_class() {
        assert_eq!(classify_ontology_subtype(Some("owl_class")), OntologySubtype::Class);
    }

    #[test]
    fn classify_subtype_owlclass() {
        assert_eq!(classify_ontology_subtype(Some("owlclass")), OntologySubtype::Class);
    }

    #[test]
    fn classify_subtype_ontology_node() {
        assert_eq!(classify_ontology_subtype(Some("ontology_node")), OntologySubtype::Class);
    }

    #[test]
    fn classify_subtype_ontologyclass() {
        assert_eq!(classify_ontology_subtype(Some("ontologyclass")), OntologySubtype::Class);
    }

    // GIVEN: Individual-type ontology strings
    // WHEN:  Passed to classify_ontology_subtype
    // THEN:  Returns OntologySubtype::Individual

    #[test]
    fn classify_subtype_owl_individual() {
        assert_eq!(classify_ontology_subtype(Some("owl_individual")), OntologySubtype::Individual);
    }

    #[test]
    fn classify_subtype_ontologyindividual() {
        assert_eq!(classify_ontology_subtype(Some("ontologyindividual")), OntologySubtype::Individual);
    }

    // GIVEN: Property-type ontology strings
    // WHEN:  Passed to classify_ontology_subtype
    // THEN:  Returns OntologySubtype::Property

    #[test]
    fn classify_subtype_owl_property() {
        assert_eq!(classify_ontology_subtype(Some("owl_property")), OntologySubtype::Property);
    }

    #[test]
    fn classify_subtype_ontologyproperty() {
        assert_eq!(classify_ontology_subtype(Some("ontologyproperty")), OntologySubtype::Property);
    }

    // GIVEN: Non-ontology or absent type strings
    // WHEN:  Passed to classify_ontology_subtype
    // THEN:  Returns OntologySubtype::Unspecified

    #[test]
    fn classify_subtype_none() {
        assert_eq!(classify_ontology_subtype(None), OntologySubtype::Unspecified);
    }

    #[test]
    fn classify_subtype_page() {
        assert_eq!(classify_ontology_subtype(Some("page")), OntologySubtype::Unspecified);
    }

    #[test]
    fn classify_subtype_agent() {
        assert_eq!(classify_ontology_subtype(Some("agent")), OntologySubtype::Unspecified);
    }

    // ── effective_node_type ──────────────────────────────────────────

    // GIVEN: node_type is Some
    // WHEN:  effective_node_type is called
    // THEN:  Returns node_type directly, ignoring metadata

    #[test]
    fn effective_type_returns_node_type_when_present() {
        let meta = HashMap::from([
            ("type".to_string(), "ignored".to_string()),
        ]);
        assert_eq!(
            effective_node_type(Some("owl_class"), Some(&meta)),
            Some("owl_class"),
        );
    }

    #[test]
    fn effective_type_returns_node_type_without_metadata() {
        assert_eq!(
            effective_node_type(Some("agent"), None),
            Some("agent"),
        );
    }

    // GIVEN: node_type is None, metadata has "type" key
    // WHEN:  effective_node_type is called
    // THEN:  Returns metadata["type"]

    #[test]
    fn effective_type_falls_back_to_metadata_type() {
        let meta = HashMap::from([
            ("type".to_string(), "owl_class".to_string()),
        ]);
        assert_eq!(
            effective_node_type(None, Some(&meta)),
            Some("owl_class"),
        );
    }

    // GIVEN: node_type is None, metadata has "node_type" key (no "type")
    // WHEN:  effective_node_type is called
    // THEN:  Returns metadata["node_type"]

    #[test]
    fn effective_type_falls_back_to_metadata_node_type() {
        let meta = HashMap::from([
            ("node_type".to_string(), "page".to_string()),
        ]);
        assert_eq!(
            effective_node_type(None, Some(&meta)),
            Some("page"),
        );
    }

    // GIVEN: node_type is None, metadata has both "type" and "node_type"
    // WHEN:  effective_node_type is called
    // THEN:  Returns "type" (first fallback wins)

    #[test]
    fn effective_type_prefers_type_over_node_type() {
        let meta = HashMap::from([
            ("type".to_string(), "owl_class".to_string()),
            ("node_type".to_string(), "page".to_string()),
        ]);
        assert_eq!(
            effective_node_type(None, Some(&meta)),
            Some("owl_class"),
        );
    }

    // GIVEN: node_type is None, no metadata
    // WHEN:  effective_node_type is called
    // THEN:  Returns None

    #[test]
    fn effective_type_returns_none_without_metadata() {
        assert_eq!(effective_node_type(None, None), None);
    }

    // GIVEN: node_type is None, empty metadata
    // WHEN:  effective_node_type is called
    // THEN:  Returns None

    #[test]
    fn effective_type_returns_none_with_empty_metadata() {
        let meta: HashMap<String, String> = HashMap::new();
        assert_eq!(effective_node_type(None, Some(&meta)), None);
    }
}
