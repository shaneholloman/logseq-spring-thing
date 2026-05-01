use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::edge_kind::EdgeKind;
use crate::node_kind::NodeKind;

/// A node in the typed graph schema.
///
/// Carries the new `kind` field alongside the legacy `node_type` string
/// for backward compatibility during migration. The URN is the
/// authoritative identity per ADR-064.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypedNode {
    pub urn: String,
    pub kind: NodeKind,
    pub kind_id: u8,
    pub label: String,
    #[serde(default)]
    pub properties: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub legacy_node_type: Option<String>,
}

impl TypedNode {
    pub fn new(urn: String, kind: NodeKind, label: String) -> Self {
        Self {
            kind_id: kind.kind_id(),
            urn,
            kind,
            label,
            properties: HashMap::new(),
            legacy_node_type: None,
        }
    }
}

/// An edge in the typed graph schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypedEdge {
    pub source_urn: String,
    pub target_urn: String,
    pub kind: EdgeKind,
    pub kind_id: u8,
    #[serde(default)]
    pub weight: f32,
    #[serde(default)]
    pub properties: HashMap<String, serde_json::Value>,
}

impl TypedEdge {
    pub fn new(source_urn: String, target_urn: String, kind: EdgeKind) -> Self {
        Self {
            source_urn,
            target_urn,
            kind_id: kind.kind_id(),
            kind,
            weight: 1.0,
            properties: HashMap::new(),
        }
    }
}

/// Container for a typed graph with nodes and edges.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TypedGraph {
    pub nodes: Vec<TypedNode>,
    pub edges: Vec<TypedEdge>,
}

impl TypedGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_node(&mut self, node: TypedNode) {
        self.nodes.push(node);
    }

    pub fn add_edge(&mut self, edge: TypedEdge) {
        self.edges.push(edge);
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_node_kind_id_matches() {
        let node = TypedNode::new(
            "urn:visionclaw:concept:abc:page:test".into(),
            NodeKind::Page,
            "Test Page".into(),
        );
        assert_eq!(node.kind_id, NodeKind::Page.kind_id());
    }

    #[test]
    fn typed_edge_kind_id_matches() {
        let edge = TypedEdge::new(
            "urn:a".into(),
            "urn:b".into(),
            EdgeKind::WikiLink,
        );
        assert_eq!(edge.kind_id, EdgeKind::WikiLink.kind_id());
    }

    #[test]
    fn graph_add_and_count() {
        let mut g = TypedGraph::new();
        g.add_node(TypedNode::new("urn:a".into(), NodeKind::Page, "A".into()));
        g.add_node(TypedNode::new("urn:b".into(), NodeKind::Block, "B".into()));
        g.add_edge(TypedEdge::new("urn:a".into(), "urn:b".into(), EdgeKind::BlockParent));
        assert_eq!(g.node_count(), 2);
        assert_eq!(g.edge_count(), 1);
    }
}
