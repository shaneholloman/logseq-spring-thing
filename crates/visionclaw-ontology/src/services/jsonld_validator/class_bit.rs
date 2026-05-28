//! Class-bit ↔ `@type` ↔ `@id`-scheme consistency check.
//!
//! Per fixture `108-mismatched-class-bit.md`, the binary protocol
//! encodes the entity class in the upper bits of a node ID. The
//! canonical schema treats the IRI scheme as the source of truth for
//! which class an entity belongs to. The validator rejects when the
//! declared `@type` implies a class-bit that contradicts the IRI
//! scheme — otherwise the node would be silently mis-routed at the
//! named-graph adapter and visibility would break.

use super::errors::ErrorCategory;
use super::iri::{classify, IriScheme};
use super::shacl_lite::collect_types;
use serde_json::Value;

/// Class-bit values per the binary protocol. The hex values match the
/// in-tree NODE_ID flag bits in the graph state actor.
pub mod bits {
    pub const PAGE: u32 = 0x4000_0000;
    pub const ONTOLOGY: u32 = 0x0400_0000;
    pub const AGENT: u32 = 0x8000_0000;
    pub const LINKED_PAGE: u32 = 0x0800_0000;
    pub const AXIOM: u32 = 0x0200_0000;
    pub const PROPERTY: u32 = 0x0100_0000;
    pub const BRIDGE: u32 = 0x1000_0000;
    pub const NOSTR_EVENT: u32 = 0x2000_0000;
}

/// Map a declared `@type` to the class-bit it implies.
///
/// Returns `None` for type names that do not constrain class bits
/// (e.g. annotation types, or `owl:Axiom` which can apply to any
/// asserted axiom regardless of IRI scheme).
pub fn class_bit_for_type(type_name: &str) -> Option<u32> {
    match type_name {
        "Page" | "schema:WebPage" => Some(bits::PAGE),
        "LinkedPage" | "vc:LinkedPage" => Some(bits::LINKED_PAGE),
        "OntologyClass" | "owl:Class" | "Class" => Some(bits::ONTOLOGY),
        "OntologyProperty"
        | "DataProperty"
        | "AnnotationProperty"
        | "owl:ObjectProperty"
        | "owl:DatatypeProperty"
        | "owl:AnnotationProperty" => Some(bits::PROPERTY),
        "Axiom" => Some(bits::AXIOM),
        "AgentTelemetry" | "vc:AgentTelemetry" => Some(bits::AGENT),
        "BridgeRecord" | "Bridge" | "vc:BridgeRecord" => Some(bits::BRIDGE),
        "NostrSignedPage" | "vc:NostrSignedPage" => Some(bits::NOSTR_EVENT),
        _ => None,
    }
}

/// Map an IRI scheme to the class-bit it implies.
pub fn class_bit_for_iri_scheme(scheme: IriScheme) -> Option<u32> {
    match scheme {
        IriScheme::Page => Some(bits::PAGE),
        IriScheme::LinkedPage => Some(bits::LINKED_PAGE),
        IriScheme::OwlClass => Some(bits::ONTOLOGY),
        IriScheme::OwlProperty => Some(bits::PROPERTY),
        IriScheme::Axiom => Some(bits::AXIOM),
        IriScheme::Agent => Some(bits::AGENT),
        IriScheme::Bridge => Some(bits::BRIDGE),
        IriScheme::NostrEvent => Some(bits::NOSTR_EVENT),
        IriScheme::Graph | IriScheme::DidNostr | IriScheme::OtherValid => None,
    }
}

/// Cross-check `@type` against the `@id`'s IRI scheme.
///
/// If both sides have a definite class-bit and they disagree, a
/// `ClassBitMismatch` error is returned. If either side is undecided
/// (e.g. only annotation types, or `urn:isbn:` scheme), the check is
/// skipped — we don't have enough information to call a mismatch.
pub fn validate_class_bit_consistency(entry: &Value) -> Vec<ErrorCategory> {
    let Value::Object(map) = entry else {
        return vec![];
    };
    let Some(id_value) = map.get("@id").or_else(|| map.get("id")) else {
        return vec![];
    };
    let Some(id_str) = id_value.as_str() else {
        return vec![];
    };
    let Some(scheme) = classify(id_str) else {
        return vec![]; // malformed IRI — frame.rs handles that
    };
    let Some(iri_bit) = class_bit_for_iri_scheme(scheme) else {
        return vec![];
    };

    let types = collect_types(entry);
    // Find a type that imposes a definite (and distinct) class-bit.
    let declared_bits: Vec<(String, u32)> = types
        .iter()
        .filter_map(|t| class_bit_for_type(t).map(|b| (t.clone(), b)))
        .collect();

    // If no @type carries a definite class-bit, we have nothing to
    // compare against. If at least one type matches the IRI scheme,
    // we treat the entry as consistent (a multi-typed entry like
    // `["OntologyClass", "owl:Class"]` is fine).
    if declared_bits.is_empty() {
        return vec![];
    }
    let any_match = declared_bits.iter().any(|(_, b)| *b == iri_bit);
    if any_match {
        return vec![];
    }
    // Mismatch: pick the first declared type for the diagnostic.
    let (declared_type, declared_bit) = &declared_bits[0];
    let implied_kind = describe_scheme(scheme);
    vec![ErrorCategory::ClassBitMismatch {
        declared: format!("{} (class bit 0x{:08x})", declared_type, declared_bit),
        implied_by_iri: format!("{} (class bit 0x{:08x})", implied_kind, iri_bit),
    }]
}

fn describe_scheme(scheme: IriScheme) -> &'static str {
    match scheme {
        IriScheme::Page => "Page",
        IriScheme::LinkedPage => "LinkedPage",
        IriScheme::OwlClass => "OntologyClass",
        IriScheme::OwlProperty => "OntologyProperty",
        IriScheme::Axiom => "Axiom",
        IriScheme::Agent => "AgentTelemetry",
        IriScheme::Bridge => "BridgeRecord",
        IriScheme::NostrEvent => "NostrSignedPage",
        IriScheme::Graph => "NamedGraph",
        IriScheme::DidNostr => "did:nostr",
        IriScheme::OtherValid => "Other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn agent_iri_with_ontology_class_type_rejected() {
        let entry = json!({
            "@id": "urn:visionclaw:agent:run-x:step-0",
            "@type": ["OntologyClass", "owl:Class"]
        });
        let issues = validate_class_bit_consistency(&entry);
        assert!(matches!(
            issues.first(),
            Some(ErrorCategory::ClassBitMismatch { .. })
        ));
    }

    #[test]
    fn matching_iri_and_type_accepted() {
        let entry = json!({
            "@id": "urn:visionclaw:page:abc",
            "@type": "Page"
        });
        assert!(validate_class_bit_consistency(&entry).is_empty());
    }

    #[test]
    fn ontology_class_with_owl_class_dual_type_accepted() {
        let entry = json!({
            "@id": "urn:visionclaw:owl:class:cybernetics",
            "@type": ["OntologyClass", "owl:Class"]
        });
        assert!(validate_class_bit_consistency(&entry).is_empty());
    }
}
