use serde::{Deserialize, Serialize};
use strum::{EnumIter, EnumString, Display, AsRefStr};

/// 21-variant node kind taxonomy per ADR-064.
///
/// Uses a dedicated `kind_id: u8` field rather than packing into the
/// existing 32-bit flag word (only 2 free bits remain).
///
/// Grouped by domain:
/// - Code (5): Function, Module, Class, Interface, Variable
/// - Infrastructure (8): Service, Container, Database, Queue, Cache, Gateway, LoadBalancer, CDN
/// - Domain (3): Entity, ValueObject, Aggregate
/// - Knowledge (5): Page, Block, Concept, OntologyClass, OntologyIndividual
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize,
    EnumIter, EnumString, Display, AsRefStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[repr(u8)]
pub enum NodeKind {
    // ── Code (5) ──
    Function = 0,
    Module = 1,
    Class = 2,
    Interface = 3,
    Variable = 4,

    // ── Infrastructure (8) ──
    Service = 10,
    Container = 11,
    Database = 12,
    Queue = 13,
    Cache = 14,
    Gateway = 15,
    LoadBalancer = 16,
    Cdn = 17,

    // ── Domain (3) ──
    Entity = 20,
    ValueObject = 21,
    Aggregate = 22,

    // ── Knowledge (5) ──
    Page = 30,
    Block = 31,
    Concept = 32,
    OntologyClass = 33,
    OntologyIndividual = 34,
}

impl NodeKind {
    pub fn kind_id(self) -> u8 {
        self as u8
    }

    pub fn group(self) -> NodeGroup {
        match self {
            Self::Function | Self::Module | Self::Class | Self::Interface | Self::Variable => {
                NodeGroup::Code
            }
            Self::Service | Self::Container | Self::Database | Self::Queue | Self::Cache
            | Self::Gateway | Self::LoadBalancer | Self::Cdn => NodeGroup::Infrastructure,
            Self::Entity | Self::ValueObject | Self::Aggregate => NodeGroup::Domain,
            Self::Page | Self::Block | Self::Concept | Self::OntologyClass
            | Self::OntologyIndividual => NodeGroup::Knowledge,
        }
    }

    /// Map from existing VisionClaw node_type strings to the new taxonomy.
    /// Returns None for truly unknown types (caller decides fallback).
    pub fn from_legacy_type(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "page" => Some(Self::Page),
            "linked_page" => Some(Self::Page),
            "block" => Some(Self::Block),
            "knowledge_node" => Some(Self::Concept),
            "owl_class" | "owlclass" | "ontology_node" | "ontologyclass" => {
                Some(Self::OntologyClass)
            }
            "owl_individual" | "ontologyindividual" => Some(Self::OntologyIndividual),
            "owl_property" | "ontologyproperty" => Some(Self::OntologyClass),
            "agent" | "bot" => Some(Self::Service),
            "function" => Some(Self::Function),
            "module" => Some(Self::Module),
            "class" => Some(Self::Class),
            "interface" => Some(Self::Interface),
            "variable" => Some(Self::Variable),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeGroup {
    Code,
    Infrastructure,
    Domain,
    Knowledge,
}

#[cfg(test)]
mod tests {
    use super::*;
    use strum::IntoEnumIterator;

    #[test]
    fn all_kind_ids_unique() {
        let ids: Vec<u8> = NodeKind::iter().map(|k| k.kind_id()).collect();
        let mut deduped = ids.clone();
        deduped.sort();
        deduped.dedup();
        assert_eq!(ids.len(), deduped.len(), "duplicate kind_id detected");
    }

    #[test]
    fn total_variants_is_21() {
        assert_eq!(NodeKind::iter().count(), 21);
    }

    #[test]
    fn serde_roundtrip() {
        for kind in NodeKind::iter() {
            let json = serde_json::to_string(&kind).unwrap();
            let back: NodeKind = serde_json::from_str(&json).unwrap();
            assert_eq!(kind, back);
        }
    }

    #[test]
    fn legacy_mapping_page() {
        assert_eq!(NodeKind::from_legacy_type("page"), Some(NodeKind::Page));
        assert_eq!(NodeKind::from_legacy_type("linked_page"), Some(NodeKind::Page));
    }

    #[test]
    fn legacy_mapping_ontology() {
        assert_eq!(NodeKind::from_legacy_type("owl_class"), Some(NodeKind::OntologyClass));
        assert_eq!(NodeKind::from_legacy_type("OwlClass"), Some(NodeKind::OntologyClass));
    }

    #[test]
    fn legacy_mapping_agent_to_service() {
        assert_eq!(NodeKind::from_legacy_type("agent"), Some(NodeKind::Service));
    }

    #[test]
    fn group_classification() {
        assert_eq!(NodeKind::Function.group(), NodeGroup::Code);
        assert_eq!(NodeKind::Database.group(), NodeGroup::Infrastructure);
        assert_eq!(NodeKind::Entity.group(), NodeGroup::Domain);
        assert_eq!(NodeKind::Block.group(), NodeGroup::Knowledge);
    }
}
