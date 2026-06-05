// src/services/jsonld_ingest/shacl_gate.rs
//! SHACL-lite ingest gate (ADR-100 D4 / PRD-018 WS-5).
//!
//! Wires the EXISTING SHACL-lite validator
//! ([`crate::services::jsonld_validator::shacl_lite`]) into the ingest
//! pipeline as a validation gate over the parsed JSON-LD shapes. We do NOT
//! write a second SHACL engine — this is a thin adapter that:
//!
//! 1. Walks each block's JSON-LD entries (handling `@graph`).
//! 2. Runs `shacl_lite::validate_entry_shape` against each entry.
//! 3. Aggregates the findings into a [`ShaclGateReport`] the pipeline returns.
//!
//! The shape rules themselves (OntologyClass requires `subClassOf`,
//! BridgeRecord targets must be concrete, …) live in `shacl_lite.rs` and are
//! the single source of truth — adding a constraint there automatically
//! tightens this gate.

use serde_json::Value;

use crate::services::jsonld_validator::shacl_lite;
use crate::services::jsonld_validator::ErrorCategory;

/// A single SHACL shape violation, located to its block + subject.
#[derive(Debug, Clone)]
pub struct ShaclViolation {
    /// 0-based block index within the source file.
    pub block_index: usize,
    /// The `@id` of the offending entry, when present.
    pub subject: Option<String>,
    /// The SHACL-lite category that fired (reused from the validator).
    pub category: ErrorCategory,
}

/// Aggregated SHACL gate result for one ingest. `is_valid()` is the gate
/// decision; `violations` is the surfaced report.
#[derive(Debug, Clone, Default)]
pub struct ShaclGateReport {
    pub violations: Vec<ShaclViolation>,
    /// Number of entries (shapes) the gate inspected.
    pub shapes_checked: usize,
}

impl ShaclGateReport {
    /// The gate passes iff no shape violations were found.
    pub fn is_valid(&self) -> bool {
        self.violations.is_empty()
    }
}

/// Run the SHACL-lite gate over one parsed JSON-LD block. Reuses the existing
/// `shacl_lite::validate_entry_shape`; appends findings to `report`.
pub fn gate_block(block: &Value, block_index: usize, report: &mut ShaclGateReport) {
    for entry in collect_entries(block) {
        report.shapes_checked += 1;
        let subject = entry_subject(entry);
        for category in shacl_lite::validate_entry_shape(entry) {
            report.violations.push(ShaclViolation {
                block_index,
                subject: subject.clone(),
                category,
            });
        }
    }
}

/// Walk a JSON-LD document and return every assertion entry, handling the
/// `@graph` array form (mirrors the validator's own `collect_entries`).
fn collect_entries(block: &Value) -> Vec<&Value> {
    let Value::Object(map) = block else {
        return vec![block];
    };
    if let Some(g) = map.get("@graph").or_else(|| map.get("graph")) {
        match g {
            Value::Array(items) => items.iter().collect(),
            other => vec![other],
        }
    } else {
        vec![block]
    }
}

fn entry_subject(entry: &Value) -> Option<String> {
    entry
        .as_object()?
        .get("@id")
        .or_else(|| entry.as_object().and_then(|m| m.get("id")))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn valid_ontology_class_passes_gate() {
        let block = json!({
            "@id": "urn:visionclaw:owl:class:cybernetics",
            "@type": "OntologyClass",
            "rdfs:subClassOf": { "@id": "urn:visionclaw:owl:class:built-environment" }
        });
        let mut report = ShaclGateReport::default();
        gate_block(&block, 0, &mut report);
        assert!(report.is_valid(), "well-formed class must pass: {report:?}");
        assert_eq!(report.shapes_checked, 1);
    }

    #[test]
    fn shape_violating_block_is_reported() {
        // OntologyClass without subClassOf (and not the root) violates the
        // shape — the gate must surface it, not silently pass.
        let block = json!({
            "@id": "urn:visionclaw:owl:class:orphan",
            "@type": ["OntologyClass", "owl:Class"],
            "rdfs:label": "Orphan"
        });
        let mut report = ShaclGateReport::default();
        gate_block(&block, 3, &mut report);
        assert!(!report.is_valid());
        let v = &report.violations[0];
        assert_eq!(v.block_index, 3);
        assert_eq!(v.subject.as_deref(), Some("urn:visionclaw:owl:class:orphan"));
        assert!(matches!(
            v.category,
            ErrorCategory::RequiredFieldMissing { ref what } if what == "subClassOf"
        ));
    }

    #[test]
    fn gate_handles_graph_array() {
        let block = json!({
            "@graph": [
                {
                    "@id": "urn:visionclaw:owl:class:a",
                    "@type": "OntologyClass",
                    "rdfs:subClassOf": { "@id": "urn:visionclaw:owl:class:built-environment" }
                },
                {
                    "@id": "urn:visionclaw:owl:class:b-orphan",
                    "@type": "OntologyClass"
                }
            ]
        });
        let mut report = ShaclGateReport::default();
        gate_block(&block, 0, &mut report);
        assert_eq!(report.shapes_checked, 2);
        assert_eq!(report.violations.len(), 1, "only the orphan violates");
        assert_eq!(report.violations[0].subject.as_deref(), Some("urn:visionclaw:owl:class:b-orphan"));
    }
}
