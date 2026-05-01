use serde::{Deserialize, Serialize};
use strum::{EnumIter, EnumString, Display, AsRefStr};

use crate::edge_category::EdgeCategory;

/// 35-variant edge kind taxonomy per ADR-064.
///
/// Organized into 8 categories matching UA's relationship model,
/// extended for VisionClaw's knowledge graph and ontology needs.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize,
    EnumIter, EnumString, Display, AsRefStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[repr(u8)]
pub enum EdgeKind {
    // ── Structural (5) ──
    Contains = 0,
    InheritsFrom = 1,
    Implements = 2,
    ComposedOf = 3,
    Nests = 4,

    // ── Behavioral (4) ──
    Calls = 10,
    Overrides = 11,
    Triggers = 12,
    Subscribes = 13,

    // ── Data Flow (4) ──
    ReadsFrom = 20,
    WritesTo = 21,
    TransformsTo = 22,
    Pipes = 23,

    // ── Dependencies (4) ──
    DependsOn = 30,
    Imports = 31,
    Requires = 32,
    Enables = 33,

    // ── Semantic (5) ──
    SubClassOf = 40,
    InstanceOf = 41,
    EquivalentTo = 42,
    DisjointWith = 43,
    SameAs = 44,

    // ── Infrastructure (4) ──
    DeploysTo = 50,
    RoutesTo = 51,
    ReplicatesTo = 52,
    Monitors = 53,

    // ── Domain (4) ──
    HasPart = 60,
    BridgesTo = 61,
    Fulfills = 62,
    Constrains = 63,

    // ── Knowledge (5) ──
    WikiLink = 70,
    BlockRef = 71,
    BlockParent = 72,
    TaggedWith = 73,
    CitedBy = 74,
}

impl EdgeKind {
    pub fn kind_id(self) -> u8 {
        self as u8
    }

    pub fn category(self) -> EdgeCategory {
        match self {
            Self::Contains | Self::InheritsFrom | Self::Implements | Self::ComposedOf
            | Self::Nests => EdgeCategory::Structural,

            Self::Calls | Self::Overrides | Self::Triggers | Self::Subscribes => {
                EdgeCategory::Behavioral
            }

            Self::ReadsFrom | Self::WritesTo | Self::TransformsTo | Self::Pipes => {
                EdgeCategory::DataFlow
            }

            Self::DependsOn | Self::Imports | Self::Requires | Self::Enables => {
                EdgeCategory::Dependencies
            }

            Self::SubClassOf | Self::InstanceOf | Self::EquivalentTo | Self::DisjointWith
            | Self::SameAs => EdgeCategory::Semantic,

            Self::DeploysTo | Self::RoutesTo | Self::ReplicatesTo | Self::Monitors => {
                EdgeCategory::Infrastructure
            }

            Self::HasPart | Self::BridgesTo | Self::Fulfills | Self::Constrains => {
                EdgeCategory::Domain
            }

            Self::WikiLink | Self::BlockRef | Self::BlockParent | Self::TaggedWith
            | Self::CitedBy => EdgeCategory::Knowledge,
        }
    }

    /// Whether this edge kind naturally has direction (source→target).
    pub fn is_directed(self) -> bool {
        match self {
            Self::EquivalentTo | Self::DisjointWith | Self::SameAs | Self::BridgesTo
            | Self::WikiLink => false,
            _ => true,
        }
    }

    /// Map from existing VisionClaw/SemanticTypeRegistry URIs to the new taxonomy.
    pub fn from_registry_uri(uri: &str) -> Option<Self> {
        match uri {
            "ngm:requires" | "requires" => Some(Self::Requires),
            "ngm:enables" | "enables" => Some(Self::Enables),
            "ngm:has-part" | "has-part" => Some(Self::HasPart),
            "ngm:bridges-to" | "bridges-to" => Some(Self::BridgesTo),
            "subClassOf" | "rdfs:subClassOf" => Some(Self::SubClassOf),
            "instanceOf" | "rdf:type" => Some(Self::InstanceOf),
            "owl:equivalentClass" => Some(Self::EquivalentTo),
            "owl:disjointWith" => Some(Self::DisjointWith),
            "skos:broader" => Some(Self::SubClassOf),
            "skos:narrower" => Some(Self::Contains),
            "skos:related" => Some(Self::BridgesTo),
            "dependency" => Some(Self::DependsOn),
            "hierarchy" => Some(Self::Contains),
            "association" => Some(Self::BridgesTo),
            "sequence" => Some(Self::Pipes),
            "WIKILINK" | "wikilink" => Some(Self::WikiLink),
            "BLOCK_REF" | "block_ref" => Some(Self::BlockRef),
            "BLOCK_PARENT" | "block_parent" => Some(Self::BlockParent),
            _ => None,
        }
    }

    /// Default GPU force parameters for this edge kind.
    /// Returns (spring_k, rest_length, repulsion_strength).
    pub fn default_force_params(self) -> EdgeForceDefaults {
        match self.category() {
            EdgeCategory::Structural => EdgeForceDefaults {
                spring_k: 0.8,
                rest_length: 60.0,
                repulsion_strength: 0.0,
            },
            EdgeCategory::Behavioral => EdgeForceDefaults {
                spring_k: 0.6,
                rest_length: 80.0,
                repulsion_strength: 0.0,
            },
            EdgeCategory::DataFlow => EdgeForceDefaults {
                spring_k: 0.5,
                rest_length: 90.0,
                repulsion_strength: 0.0,
            },
            EdgeCategory::Dependencies => EdgeForceDefaults {
                spring_k: 0.7,
                rest_length: 80.0,
                repulsion_strength: 0.0,
            },
            EdgeCategory::Semantic => match self {
                Self::DisjointWith => EdgeForceDefaults {
                    spring_k: -0.3,
                    rest_length: 150.0,
                    repulsion_strength: 0.3,
                },
                Self::EquivalentTo | Self::SameAs => EdgeForceDefaults {
                    spring_k: 0.9,
                    rest_length: 30.0,
                    repulsion_strength: 0.0,
                },
                _ => EdgeForceDefaults {
                    spring_k: 0.7,
                    rest_length: 70.0,
                    repulsion_strength: 0.0,
                },
            },
            EdgeCategory::Infrastructure => EdgeForceDefaults {
                spring_k: 0.4,
                rest_length: 120.0,
                repulsion_strength: 0.0,
            },
            EdgeCategory::Domain => match self {
                Self::HasPart => EdgeForceDefaults {
                    spring_k: 0.9,
                    rest_length: 40.0,
                    repulsion_strength: 0.0,
                },
                Self::BridgesTo => EdgeForceDefaults {
                    spring_k: 0.3,
                    rest_length: 200.0,
                    repulsion_strength: 0.0,
                },
                _ => EdgeForceDefaults {
                    spring_k: 0.5,
                    rest_length: 100.0,
                    repulsion_strength: 0.0,
                },
            },
            EdgeCategory::Knowledge => match self {
                Self::BlockParent => EdgeForceDefaults {
                    spring_k: 0.85,
                    rest_length: 30.0,
                    repulsion_strength: 0.0,
                },
                Self::BlockRef => EdgeForceDefaults {
                    spring_k: 0.4,
                    rest_length: 120.0,
                    repulsion_strength: 0.0,
                },
                _ => EdgeForceDefaults {
                    spring_k: 0.5,
                    rest_length: 100.0,
                    repulsion_strength: 0.0,
                },
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct EdgeForceDefaults {
    pub spring_k: f32,
    pub rest_length: f32,
    pub repulsion_strength: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use strum::IntoEnumIterator;

    #[test]
    fn total_variants_is_35() {
        assert_eq!(EdgeKind::iter().count(), 35);
    }

    #[test]
    fn all_kind_ids_unique() {
        let ids: Vec<u8> = EdgeKind::iter().map(|k| k.kind_id()).collect();
        let mut deduped = ids.clone();
        deduped.sort();
        deduped.dedup();
        assert_eq!(ids.len(), deduped.len(), "duplicate edge kind_id");
    }

    #[test]
    fn serde_roundtrip() {
        for kind in EdgeKind::iter() {
            let json = serde_json::to_string(&kind).unwrap();
            let back: EdgeKind = serde_json::from_str(&json).unwrap();
            assert_eq!(kind, back);
        }
    }

    #[test]
    fn category_mapping_complete() {
        for kind in EdgeKind::iter() {
            let _ = kind.category();
        }
    }

    #[test]
    fn registry_uri_mapping() {
        assert_eq!(EdgeKind::from_registry_uri("ngm:requires"), Some(EdgeKind::Requires));
        assert_eq!(EdgeKind::from_registry_uri("rdfs:subClassOf"), Some(EdgeKind::SubClassOf));
        assert_eq!(EdgeKind::from_registry_uri("owl:disjointWith"), Some(EdgeKind::DisjointWith));
        assert_eq!(EdgeKind::from_registry_uri("WIKILINK"), Some(EdgeKind::WikiLink));
    }

    #[test]
    fn disjoint_with_is_repulsive() {
        let params = EdgeKind::DisjointWith.default_force_params();
        assert!(params.spring_k < 0.0);
        assert!(params.repulsion_strength > 0.0);
    }

    #[test]
    fn directionality() {
        assert!(EdgeKind::Contains.is_directed());
        assert!(!EdgeKind::DisjointWith.is_directed());
        assert!(!EdgeKind::WikiLink.is_directed());
    }
}
