use graph_cognition_core::EdgeKind;
use strum::IntoEnumIterator;

/// Extension to register all 35 EdgeKinds into the SemanticTypeRegistry.
///
/// This bridges the new typed schema (ADR-064) with the existing
/// DynamicRelationshipBuffer + SemanticTypeRegistry dispatch path.
///
/// The returned Vec of (uri, spring_k, rest_length, is_directional, force_type)
/// tuples can be fed into SemanticTypeRegistry::register() by the main crate.
pub fn all_edge_kind_registry_entries() -> Vec<EdgeKindRegistryEntry> {
    EdgeKind::iter()
        .map(|kind| {
            let defaults = kind.default_force_params();
            let force_type = match kind.category() {
                graph_cognition_core::EdgeCategory::Domain
                    if kind == EdgeKind::HasPart =>
                {
                    1 // orbit clustering
                }
                graph_cognition_core::EdgeCategory::Domain
                    if kind == EdgeKind::BridgesTo =>
                {
                    2 // cross-domain long-range
                }
                graph_cognition_core::EdgeCategory::Semantic
                    if kind == EdgeKind::DisjointWith =>
                {
                    3 // repulsion
                }
                _ => 0, // standard spring
            };

            EdgeKindRegistryEntry {
                uri: format!("vc:{}", kind.as_ref()),
                kind,
                spring_k: defaults.spring_k,
                rest_length: defaults.rest_length,
                is_directional: kind.is_directed(),
                force_type,
            }
        })
        .collect()
}

#[derive(Debug, Clone)]
pub struct EdgeKindRegistryEntry {
    pub uri: String,
    pub kind: EdgeKind,
    pub spring_k: f32,
    pub rest_length: f32,
    pub is_directional: bool,
    pub force_type: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_35_entries() {
        let entries = all_edge_kind_registry_entries();
        assert_eq!(entries.len(), 35);
    }

    #[test]
    fn all_uris_prefixed() {
        for entry in all_edge_kind_registry_entries() {
            assert!(entry.uri.starts_with("vc:"), "bad uri: {}", entry.uri);
        }
    }

    #[test]
    fn has_part_is_orbit() {
        let entries = all_edge_kind_registry_entries();
        let hp = entries.iter().find(|e| e.kind == EdgeKind::HasPart).unwrap();
        assert_eq!(hp.force_type, 1);
    }

    #[test]
    fn disjoint_is_repulsion() {
        let entries = all_edge_kind_registry_entries();
        let dj = entries.iter().find(|e| e.kind == EdgeKind::DisjointWith).unwrap();
        assert_eq!(dj.force_type, 3);
    }
}
