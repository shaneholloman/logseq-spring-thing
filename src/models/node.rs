use crate::config::dev_config;
use crate::utils::socket_flow_messages::BinaryNodeData;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};

// Static counter for generating unique numeric IDs
static NEXT_NODE_ID: AtomicU32 = AtomicU32::new(1);

/// Sovereign-model visibility for a knowledge-graph node (ADR-050).
///
/// `Public` nodes render with full label and metadata. `Private` nodes are
/// owner-sovereign: the server only emits them over the wire with bit 29 of the
/// node id set (see `crate::utils::binary_protocol::PRIVATE_OPAQUE_FLAG`) and
/// with label/metadata stripped, so non-owner clients see an opaque placeholder.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    Public,
    Private,
}

impl Default for Visibility {
    fn default() -> Self { Self::Public }
}

impl Visibility {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Private => "private",
        }
    }
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "public" => Some(Self::Public),
            "private" => Some(Self::Private),
            _ => None,
        }
    }
    pub fn is_private(&self) -> bool { matches!(self, Self::Private) }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Node {

    pub id: u32,
    pub metadata_id: String,
    pub label: String,
    pub data: BinaryNodeData,

    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub z: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vx: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vy: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vz: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mass: Option<f32>,

    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owl_class_iri: Option<String>,

    
    #[serde(default)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, String>,
    #[serde(skip)]
    pub file_size: u64,

    
    #[serde(rename = "type")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_data: Option<HashMap<String, String>>,

    // ----------------------------------------------------------------------
    // ADR-050 Sovereign-model fields
    // ----------------------------------------------------------------------
    /// Public vs private (owner-sovereign) visibility. Defaults to `Public`
    /// for legacy rows and JSON documents that omit the field.
    #[serde(default)]
    pub visibility: Visibility,

    /// Nostr public key of the owner in 64-char hex form. `None` for
    /// public/global graph content with no sovereign owner.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_pubkey: Option<String>,

    /// Per-session HMAC-derived opaque id used when `visibility == Private`
    /// and the consuming client is not the owner. 24 hex chars (12 bytes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opaque_id: Option<String>,

    /// Solid Pod URL hosting the authoritative payload for this node.
    /// `None` when the payload lives exclusively in the central graph.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pod_url: Option<String>,

    // ----------------------------------------------------------------------
    // VisionClaw v2 ontology fields
    // ----------------------------------------------------------------------
    /// Canonical IRI from the ontology namespace, e.g.
    /// `http://narrativegoldmine.com/ontology#SomeConcept`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_iri: Option<String>,

    /// VisionClaw URN alias, e.g. `urn:visionclaw:concept:artificial-intelligence:neural-networks`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visionclaw_uri: Option<String>,

    /// OWL RDF type, e.g. `owl:Class`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rdf_type: Option<String>,

    /// `owl:sameAs` link to an equivalent resource
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub same_as: Option<String>,

    /// Full-word domain slug, e.g. `artificial-intelligence`, `blockchain`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,

    /// Content hash for deduplication, e.g. `sha256-12-...`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,

    /// Quality score in range 0.00 -- 1.00
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality_score: Option<f32>,

    /// Authority score in range 0.00 -- 1.00
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authority_score: Option<f32>,

    /// Human-readable preferred label (skos:prefLabel equivalent)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferred_term: Option<String>,

    /// Which knowledge graph this node belongs to, e.g.
    /// `"mainKnowledgeGraph"` or `"workingGraph"`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph_source: Option<String>,
}

impl Node {
    pub fn new(metadata_id: String) -> Self {
        Self::new_with_id(metadata_id, None)
    }

    pub fn new_with_id(metadata_id: String, provided_id: Option<u32>) -> Self {
        
        
        let id = match provided_id {
            Some(id) if id != 0 => {
                
                id
            }
            _ => NEXT_NODE_ID.fetch_add(1, Ordering::SeqCst),
        };

        
        
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let physics = dev_config::physics();

        
        let theta = rng.gen::<f32>() * 2.0 * std::f32::consts::PI; // azimuthal [0, 2pi)
        let phi = (rng.gen::<f32>() * 2.0 - 1.0).acos(); // polar: acos(uniform(-1,1)) for uniform sphere surface

        // cbrt gives uniform distribution within the volume (not just on the surface)
        let radius = physics.initial_radius_min
            + rng.gen::<f32>().cbrt() * physics.initial_radius_range;

        let pos_x = radius * phi.sin() * theta.cos();
        let pos_y = radius * phi.sin() * theta.sin();
        let pos_z = radius * phi.cos();

        Self {
            id,
            metadata_id: metadata_id.clone(),
            label: String::new(), 
            data: BinaryNodeData {
                node_id: id,
                
                x: pos_x,
                y: pos_y,
                z: pos_z,
                vx: 0.0, 
                vy: 0.0, 
                vz: 0.0,
            },
            
            x: Some(pos_x),
            y: Some(pos_y),
            z: Some(pos_z),
            vx: Some(0.0),
            vy: Some(0.0),
            vz: Some(0.0),
            mass: Some(1.0),
            owl_class_iri: None,
            metadata: HashMap::new(),
            file_size: 0,
            node_type: None,
            size: None,
            color: None,
            weight: None,
            group: None,
            user_data: None,
            visibility: Visibility::Public,
            owner_pubkey: None,
            opaque_id: None,
            pod_url: None,
            canonical_iri: None,
            visionclaw_uri: None,
            rdf_type: None,
            same_as: None,
            domain: None,
            content_hash: None,
            quality_score: None,
            authority_score: None,
            preferred_term: None,
            graph_source: None,
        }
    }

    pub fn set_file_size(&mut self, size: u64) {
        self.file_size = size;
        

        
        
        if size > 0 {
            self.metadata
                .insert("fileSize".to_string(), size.to_string());
        }
    }

    pub fn with_position(mut self, x: f32, y: f32, z: f32) -> Self {
        self.data.x = x;
        self.data.y = y;
        self.data.z = z;
        self.x = Some(x);
        self.y = Some(y);
        self.z = Some(z);
        self
    }

    pub fn with_velocity(mut self, vx: f32, vy: f32, vz: f32) -> Self {
        self.data.vx = vx;
        self.data.vy = vy;
        self.data.vz = vz;
        self.vx = Some(vx);
        self.vy = Some(vy);
        self.vz = Some(vz);
        self
    }

    pub fn with_mass(mut self, mass: f32) -> Self {
        self.mass = Some(mass);
        self
    }

    pub fn with_owl_class_iri(mut self, iri: String) -> Self {
        self.owl_class_iri = Some(iri);
        self
    }

    pub fn with_label(mut self, label: String) -> Self {
        self.label = label;
        self
    }

    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    pub fn with_type(mut self, node_type: String) -> Self {
        self.node_type = Some(node_type);
        self
    }

    pub fn with_size(mut self, size: f32) -> Self {
        self.size = Some(size);
        self
    }

    pub fn with_color(mut self, color: String) -> Self {
        self.color = Some(color);
        self
    }

    pub fn with_weight(mut self, weight: f32) -> Self {
        self.weight = Some(weight);
        self
    }

    pub fn with_group(mut self, group: String) -> Self {
        self.group = Some(group);
        self
    }

    
    pub fn new_with_stored_id(metadata_id: String, stored_node_id: Option<u32>) -> Self {
        
        let id = match stored_node_id {
            Some(stored_id) => stored_id,
            None => NEXT_NODE_ID.fetch_add(1, Ordering::SeqCst),
        };

        
        let id_hash = id as f32;
        let angle = id_hash * 0.618033988749895; 
        let radius = (id_hash * 0.1).min(100.0); 

        let pos_x = radius * angle.cos() * 2.0;
        let pos_y = radius * angle.sin() * 2.0;
        let pos_z = (id_hash * 0.01 - 50.0).max(-100.0).min(100.0);

        Self {
            id,
            metadata_id: metadata_id.clone(),
            label: metadata_id,
            data: BinaryNodeData {
                node_id: id,
                x: pos_x,
                y: pos_y,
                z: pos_z,
                vx: 0.0,
                vy: 0.0,
                vz: 0.0,
            },
            
            x: Some(pos_x),
            y: Some(pos_y),
            z: Some(pos_z),
            vx: Some(0.0),
            vy: Some(0.0),
            vz: Some(0.0),
            mass: Some(1.0),
            owl_class_iri: None,
            metadata: HashMap::new(),
            file_size: 0,
            node_type: None,
            size: None,
            color: None,
            weight: None,
            group: None,
            user_data: None,
            visibility: Visibility::Public,
            owner_pubkey: None,
            opaque_id: None,
            pod_url: None,
            canonical_iri: None,
            visionclaw_uri: None,
            rdf_type: None,
            same_as: None,
            domain: None,
            content_hash: None,
            quality_score: None,
            authority_score: None,
            preferred_term: None,
            graph_source: None,
        }
    }

    pub fn calculate_mass(file_size: u64) -> u8 {
        
        
        let base_mass = ((file_size + 1) as f32).log10() / 4.0;
        
        let mass = base_mass.max(0.1).min(10.0);
        (mass * 255.0 / 10.0) as u8
    }

    
    pub fn x(&self) -> f32 {
        self.data.x
    }
    pub fn y(&self) -> f32 {
        self.data.y
    }
    pub fn z(&self) -> f32 {
        self.data.z
    }
    pub fn vx(&self) -> f32 {
        self.data.vx
    }
    pub fn vy(&self) -> f32 {
        self.data.vy
    }
    pub fn vz(&self) -> f32 {
        self.data.vz
    }

    pub fn set_x(&mut self, val: f32) {
        self.data.x = val;
        self.x = Some(val);
    }
    pub fn set_y(&mut self, val: f32) {
        self.data.y = val;
        self.y = Some(val);
    }
    pub fn set_z(&mut self, val: f32) {
        self.data.z = val;
        self.z = Some(val);
    }
    pub fn set_vx(&mut self, val: f32) {
        self.data.vx = val;
        self.vx = Some(val);
    }
    pub fn set_vy(&mut self, val: f32) {
        self.data.vy = val;
        self.vy = Some(val);
    }
    pub fn set_vz(&mut self, val: f32) {
        self.data.vz = val;
        self.vz = Some(val);
    }

    pub fn set_mass(&mut self, val: f32) {
        self.mass = Some(val);
    }

    pub fn get_mass(&self) -> f32 {
        self.mass.unwrap_or(1.0)
    }

    
    pub fn id_as_string(&self) -> String {
        self.id.to_string()
    }


    pub fn from_string_id(
        id_str: &str,
        metadata_id: String,
    ) -> Result<Self, std::num::ParseIntError> {
        let id: u32 = id_str.parse()?;
        Ok(Self::new_with_stored_id(metadata_id, Some(id)))
    }

    // ---------------- ADR-050 sovereign-model helpers ----------------

    pub fn with_visibility(mut self, visibility: Visibility) -> Self {
        self.visibility = visibility;
        self
    }

    pub fn with_owner_pubkey(mut self, owner_pubkey: impl Into<String>) -> Self {
        self.owner_pubkey = Some(owner_pubkey.into());
        self
    }

    pub fn with_opaque_id(mut self, opaque_id: impl Into<String>) -> Self {
        self.opaque_id = Some(opaque_id.into());
        self
    }

    pub fn with_pod_url(mut self, pod_url: impl Into<String>) -> Self {
        self.pod_url = Some(pod_url.into());
        self
    }

    // ---------------- VisionClaw v2 helpers ----------------

    pub fn with_canonical_iri(mut self, iri: impl Into<String>) -> Self {
        self.canonical_iri = Some(iri.into());
        self
    }

    pub fn with_visionclaw_uri(mut self, uri: impl Into<String>) -> Self {
        self.visionclaw_uri = Some(uri.into());
        self
    }

    pub fn with_rdf_type(mut self, rdf_type: impl Into<String>) -> Self {
        self.rdf_type = Some(rdf_type.into());
        self
    }

    pub fn with_same_as(mut self, same_as: impl Into<String>) -> Self {
        self.same_as = Some(same_as.into());
        self
    }

    pub fn with_domain(mut self, domain: impl Into<String>) -> Self {
        self.domain = Some(domain.into());
        self
    }

    pub fn with_content_hash(mut self, hash: impl Into<String>) -> Self {
        self.content_hash = Some(hash.into());
        self
    }

    pub fn with_quality_score(mut self, score: f32) -> Self {
        self.quality_score = Some(score);
        self
    }

    pub fn with_authority_score(mut self, score: f32) -> Self {
        self.authority_score = Some(score);
        self
    }

    pub fn with_preferred_term(mut self, term: impl Into<String>) -> Self {
        self.preferred_term = Some(term.into());
        self
    }

    pub fn with_graph_source(mut self, source: impl Into<String>) -> Self {
        self.graph_source = Some(source.into());
        self
    }

    /// Returns true if this node is private AND the caller is not the owner.
    /// `caller_pubkey` is the hex pubkey of the requesting user, or `None` if
    /// the caller is anonymous / unauthenticated.
    pub fn is_opaque_to(&self, caller_pubkey: Option<&str>) -> bool {
        if !self.visibility.is_private() {
            return false;
        }
        match (&self.owner_pubkey, caller_pubkey) {
            (Some(owner), Some(caller)) => owner.as_str() != caller,
            // No owner recorded, or caller is anonymous: opacity enforced.
            _ => true,
        }
    }
}

impl Default for Node {
    fn default() -> Self {
        Self::new("default".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[test]
    fn test_numeric_id_generation() {
        
        let start_value = NEXT_NODE_ID.load(Ordering::SeqCst);

        
        let node1 = Node::new("test-file-1.md".to_string());
        let node2 = Node::new("test-file-2.md".to_string());

        
        assert_ne!(node1.id, node2.id);

        
        assert_eq!(node1.metadata_id, "test-file-1.md");
        assert_eq!(node2.metadata_id, "test-file-2.md");

        
        assert_eq!(node1.id + 1, node2.id);

        
        let end_value = NEXT_NODE_ID.load(Ordering::SeqCst);
        assert_eq!(end_value, start_value + 2);
    }

    #[test]
    fn test_node_creation() {
        let node = Node::new("test".to_string())
            .with_label("Test Node".to_string())
            .with_position(1.0, 2.0, 3.0)
            .with_velocity(0.1, 0.2, 0.3)
            .with_type("test_type".to_string())
            .with_size(1.5)
            .with_color("#FF0000".to_string())
            .with_weight(2.0)
            .with_group("group1".to_string());

        
        assert!(node.id > 0, "ID should be positive, got: {}", node.id);
        assert_eq!(node.metadata_id, "test");
        assert_eq!(node.label, "Test Node");
        assert_eq!(node.data.x, 1.0);
        assert_eq!(node.data.y, 2.0);
        assert_eq!(node.data.z, 3.0);
        assert_eq!(node.data.vx, 0.1);
        assert_eq!(node.data.vy, 0.2);
        assert_eq!(node.data.vz, 0.3);
        assert_eq!(node.node_type, Some("test_type".to_string()));
        assert_eq!(node.size, Some(1.5));
        assert_eq!(node.color, Some("#FF0000".to_string()));
        assert_eq!(node.weight, Some(2.0));
        assert_eq!(node.group, Some("group1".to_string()));
    }

    #[test]
    fn test_position_velocity_getters_setters() {
        let mut node = Node::new("test".to_string());

        node.set_x(1.0);
        node.set_y(2.0);
        node.set_z(3.0);
        node.set_vx(0.1);
        node.set_vy(0.2);
        node.set_vz(0.3);

        assert_eq!(node.x(), 1.0);
        assert_eq!(node.y(), 2.0);
        assert_eq!(node.z(), 3.0);
        assert_eq!(node.vx(), 0.1);
        assert_eq!(node.vy(), 0.2);
        assert_eq!(node.vz(), 0.3);
    }

    
    
    
    
    
    
    
}
