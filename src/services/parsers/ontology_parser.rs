// src/services/parsers/ontology_parser.rs
//! Enhanced Ontology Parser
//!
//! Parses markdown files containing ontology metadata in two formats:
//! - **v2 (VisionClaw IRI-first)**: Flat `key:: value` properties at page level,
//!   with sections under `### Semantic Classification` and `### Relationships`.
//! - **v4 (OntologyBlock)**: Nested `- ### OntologyBlock` with indented properties.
//!
//! The parser auto-detects the format via `detect_format_version()` and dispatches
//! to the appropriate extraction pipeline. Both formats produce the same
//! `OntologyBlock` / `OntologyData` output.
//!
//! Based on PAGE-FORMAT.md (v2) and canonical-ontology-block.md (v4).

use crate::ports::ontology_repository::{AxiomType, OwlAxiom, OwlClass, OwlProperty};
use log::{debug, info};
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;

// ============================================================================
// Regex Patterns - Compiled once at startup for performance
// ============================================================================

// v4 property extraction: indented `- key:: value`
static PROPERTY_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*-\s*([a-zA-Z0-9_:-]+)::\s*(.+)$").expect("Invalid PROPERTY_PATTERN regex")
});

// v2 property extraction: flat `key:: value` at page level (no leading `-`)
static V2_PROPERTY_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^([a-z][a-z0-9-]*)::\s*(.+)$").expect("Invalid V2_PROPERTY_PATTERN regex")
});

// v2 section property: indented `- key:: value` under a `### Section` header
static V2_SECTION_PROPERTY_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s+-\s*([a-z][a-z0-9-]*)::\s*(.+)$")
        .expect("Invalid V2_SECTION_PROPERTY_PATTERN regex")
});

static WIKI_LINK_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\[\[([^\]]+)\]\]").expect("Invalid WIKI_LINK_PATTERN regex")
});

#[allow(dead_code)]
static SECTION_HEADER_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*-\s*(#{1,4})\s*(.+)$").expect("Invalid SECTION_HEADER_PATTERN regex")
});

static OWL_AXIOM_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?s)```(?:clojure|owl)\s*\n(.*?)\n\s*```").expect("Invalid OWL_AXIOM_PATTERN regex")
});

static BRIDGE_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*-\s*(bridges-(?:to|from))::\s*\[\[([^\]]+)\]\]\s*via\s+(\w+)")
        .expect("Invalid BRIDGE_PATTERN regex")
});

/// v2 bridge annotation: `[[Target]] (domain: robotics)`
static V2_BRIDGE_DOMAIN_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\[\[([^\]]+)\]\]\s*\(domain:\s*([^)]+)\)")
        .expect("Invalid V2_BRIDGE_DOMAIN_PATTERN regex")
});

// Domain configuration — v4 short-code prefixes
static DOMAIN_PREFIXES: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert("AI-", "ai");
    m.insert("BC-", "bc");
    m.insert("RB-", "rb");
    m.insert("MV-", "mv");
    m.insert("TC-", "tc");
    m.insert("DT-", "dt");
    m.insert("FASH-", "fash");
    m
});

/// v2 full-word domain → IRI namespace mapping
static DOMAIN_NAMESPACES: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert(
        "artificial-intelligence",
        "http://narrativegoldmine.com/artificial-intelligence#",
    );
    m.insert(
        "blockchain",
        "http://narrativegoldmine.com/blockchain#",
    );
    m.insert(
        "spatial-computing",
        "http://narrativegoldmine.com/spatial-computing#",
    );
    m.insert(
        "robotics",
        "http://narrativegoldmine.com/robotics#",
    );
    m.insert(
        "distributed-collaboration",
        "http://narrativegoldmine.com/distributed-collaboration#",
    );
    m.insert(
        "infrastructure",
        "http://narrativegoldmine.com/infrastructure#",
    );
    m
});

// ============================================================================
// Format Version Detection
// ============================================================================

/// Detected ontology format version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OntologyFormatVersion {
    /// VisionClaw v2: IRI-first flat properties at page level.
    V2,
    /// Legacy v4: `### OntologyBlock` with nested indented properties.
    V4,
}

/// Inspect the raw file content and determine which ontology format it uses.
///
/// - **v2**: first non-empty line starts with `iri::` AND the file contains
///   `rdf-type:: owl:Class`.
/// - **v4**: file contains `### OntologyBlock`.
/// - **Neither**: returns `None` (not an ontology file).
pub fn detect_format_version(content: &str) -> Option<OntologyFormatVersion> {
    // v2 check: first line starts with `iri::` and file has `rdf-type:: owl:Class`
    let first_line = content.lines().find(|l| !l.trim().is_empty());
    if let Some(first) = first_line {
        if first.starts_with("iri::") {
            // Confirm rdf-type is present
            for line in content.lines() {
                if line.starts_with("rdf-type::") && line.contains("owl:Class") {
                    return Some(OntologyFormatVersion::V2);
                }
            }
        }
    }

    // v4 check: contains OntologyBlock header
    for line in content.lines() {
        if line.contains("### OntologyBlock") {
            return Some(OntologyFormatVersion::V4);
        }
    }

    None
}

// ============================================================================
// Data Structures
// ============================================================================

/// Complete ontology block with all tiers of metadata
#[derive(Debug, Clone)]
pub struct OntologyBlock {
    // === File Location ===
    pub file_path: String,
    pub raw_block: String,

    // === v2 IRI-first fields ===
    /// Canonical HTTP IRI (v2 line 1), e.g. `http://narrativegoldmine.com/artificial-intelligence#AIAgentSystem`
    pub iri: Option<String>,
    /// Operational VisionClaw URN (v2 line 2), e.g. `urn:visionclaw:concept:artificial-intelligence:ai-agent-system`
    pub uri: Option<String>,
    /// Explicit RDF type assertion (v2 line 3), e.g. `owl:Class`
    pub rdf_type: Option<String>,
    /// owl:sameAs link (v2 line 4)
    pub same_as: Option<String>,
    /// SHA-256 truncated content hash, e.g. `sha256-12-5916d15f1fe9`
    pub content_hash: Option<String>,

    // === Tier 1: Required Properties ===
    // Identification
    pub ontology: bool,
    pub term_id: Option<String>,
    pub preferred_term: Option<String>,
    pub source_domain: Option<String>,
    pub status: Option<String>,
    pub public_access: Option<bool>,
    pub last_updated: Option<String>,

    // Definition
    pub definition: Option<String>,

    // Semantic Classification
    pub owl_class: Option<String>,
    pub owl_physicality: Option<String>,
    pub owl_role: Option<String>,

    // Relationships (Tier 1)
    pub is_subclass_of: Vec<String>,

    // === Tier 2: Recommended Properties ===
    // Identification (Tier 2)
    pub alt_terms: Vec<String>,
    pub version: Option<String>,
    pub quality_score: Option<f64>,
    pub cross_domain_links: Option<i32>,

    // Definition (Tier 2)
    pub maturity: Option<String>,
    pub source: Vec<String>,
    pub authority_score: Option<f64>,
    pub scope_note: Option<String>,

    // Semantic Classification (Tier 2)
    pub owl_inferred_class: Option<String>,
    pub belongs_to_domain: Vec<String>,

    // Relationships (Tier 2)
    pub has_part: Vec<String>,
    pub is_part_of: Vec<String>,
    pub requires: Vec<String>,
    pub depends_on: Vec<String>,
    pub enables: Vec<String>,
    pub relates_to: Vec<String>,
    pub implements: Vec<String>,

    // === Tier 3: Optional Properties ===
    pub implemented_in_layer: Vec<String>,

    // Cross-domain bridges
    pub bridges_to: Vec<String>,
    pub bridges_from: Vec<String>,

    // OWL axioms from code blocks
    pub owl_axioms: Vec<String>,

    // Domain-specific extension properties
    pub domain_extensions: HashMap<String, String>,

    // All other relationships not explicitly modeled
    pub other_relationships: HashMap<String, Vec<String>>,
}

impl OntologyBlock {
    pub fn new(file_path: String) -> Self {
        Self {
            file_path,
            raw_block: String::new(),
            iri: None,
            uri: None,
            rdf_type: None,
            same_as: None,
            content_hash: None,
            ontology: false,
            term_id: None,
            preferred_term: None,
            source_domain: None,
            status: None,
            public_access: None,
            last_updated: None,
            definition: None,
            owl_class: None,
            owl_physicality: None,
            owl_role: None,
            is_subclass_of: Vec::new(),
            alt_terms: Vec::new(),
            version: None,
            quality_score: None,
            cross_domain_links: None,
            maturity: None,
            source: Vec::new(),
            authority_score: None,
            scope_note: None,
            owl_inferred_class: None,
            belongs_to_domain: Vec::new(),
            has_part: Vec::new(),
            is_part_of: Vec::new(),
            requires: Vec::new(),
            depends_on: Vec::new(),
            enables: Vec::new(),
            relates_to: Vec::new(),
            implements: Vec::new(),
            implemented_in_layer: Vec::new(),
            bridges_to: Vec::new(),
            bridges_from: Vec::new(),
            owl_axioms: Vec::new(),
            domain_extensions: HashMap::new(),
            other_relationships: HashMap::new(),
        }
    }

    /// Get domain from source-domain, v2 `domain::`, term-id prefix, or owl_class namespace.
    pub fn get_domain(&self) -> Option<String> {
        // Try source-domain first
        if let Some(ref domain) = self.source_domain {
            return Some(domain.to_lowercase());
        }

        // Try term-id prefix (v4 short codes)
        if let Some(ref term_id) = self.term_id {
            for (prefix, domain) in DOMAIN_PREFIXES.iter() {
                if term_id.starts_with(prefix) {
                    return Some(domain.to_string());
                }
            }
        }

        // Try namespace in owl_class
        if let Some(ref owl_class) = self.owl_class {
            if let Some(colon_idx) = owl_class.find(':') {
                let prefix = &owl_class[..colon_idx];
                // Check v4 short codes
                if DOMAIN_PREFIXES.values().any(|&d| d == prefix) {
                    return Some(prefix.to_string());
                }
                // Check v2 full-word domains
                if DOMAIN_NAMESPACES.contains_key(prefix) {
                    return Some(prefix.to_string());
                }
            }
        }

        None
    }

    /// Get full IRI from the `iri` field (v2) or by expanding the owl:class prefix (v4).
    pub fn get_full_iri(&self) -> Option<String> {
        // v2: iri field is already a full HTTP IRI
        if let Some(ref iri) = self.iri {
            if iri.starts_with("http://") || iri.starts_with("https://") {
                return Some(iri.clone());
            }
        }

        let owl_class = self.owl_class.as_ref()?;

        // If already a full URI, return it
        if owl_class.starts_with("http://") || owl_class.starts_with("https://") {
            return Some(owl_class.clone());
        }

        // Parse namespace:localname format
        if let Some(colon_idx) = owl_class.find(':') {
            let prefix = &owl_class[..colon_idx];
            let localname = &owl_class[colon_idx + 1..];

            // Try v2 full-word domain namespaces first
            if let Some(ns) = DOMAIN_NAMESPACES.get(prefix) {
                return Some(format!("{}{}", ns, localname));
            }

            // Fall back to v4 short-code namespaces
            let namespace = match prefix {
                "ai" => "http://narrativegoldmine.com/ai#",
                "bc" => "http://narrativegoldmine.com/blockchain#",
                "rb" => "http://narrativegoldmine.com/robotics#",
                "mv" => "http://narrativegoldmine.com/metaverse#",
                "tc" => "http://narrativegoldmine.com/telecollaboration#",
                "dt" => "http://narrativegoldmine.com/disruptive-tech#",
                "owl" => "http://www.w3.org/2002/07/owl#",
                "rdfs" => "http://www.w3.org/2000/01/rdf-schema#",
                "rdf" => "http://www.w3.org/1999/02/22-rdf-syntax-ns#",
                "xsd" => "http://www.w3.org/2001/XMLSchema#",
                "dcterms" => "http://purl.org/dc/terms/",
                "skos" => "http://www.w3.org/2004/02/skos/core#",
                "fash" => "http://fashionont.org/ontology#",
                "sc" => "http://fashionont.org/supply-chain#",
                "cpi" => "http://fashionont.org/product-information#",
                "clmat" => "http://fashionont.org/clothing-material#",
                "gr" => "http://purl.org/goodrelations/v1#",
                "pto" => "http://www.productontology.org/id/",
                _ => return None,
            };

            return Some(format!("{}{}", namespace, localname));
        }

        None
    }

    /// Validate that all Tier 1 (required) properties are present
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();

        // Tier 1 required properties
        if self.term_id.is_none() {
            errors.push("Missing required property: term-id".to_string());
        }

        if self.preferred_term.is_none() {
            errors.push("Missing required property: preferred-term".to_string());
        }

        if self.source_domain.is_none() {
            errors.push("Missing required property: source-domain".to_string());
        }

        if self.status.is_none() {
            errors.push("Missing required property: status".to_string());
        }

        if self.public_access.is_none() {
            errors.push("Missing required property: public-access".to_string());
        }

        if self.last_updated.is_none() {
            errors.push("Missing required property: last-updated".to_string());
        }

        if self.definition.is_none() {
            errors.push("Missing required property: definition".to_string());
        }

        if self.owl_class.is_none() {
            errors.push("Missing required property: owl:class".to_string());
        }

        if self.owl_physicality.is_none() {
            errors.push("Missing required property: owl:physicality".to_string());
        }

        if self.owl_role.is_none() {
            errors.push("Missing required property: owl:role".to_string());
        }

        if self.is_subclass_of.is_empty() {
            errors.push("Missing required property: is-subclass-of (at least one parent class)".to_string());
        }

        // Validate term-id format (v4 only — v2 uses legacy-term-id)
        if let Some(ref term_id) = self.term_id {
            if let Some(domain) = self.get_domain() {
                if let Some((&expected_prefix, _)) = DOMAIN_PREFIXES
                    .iter()
                    .find(|(_, &d)| d == domain.as_str())
                {
                    if !term_id.starts_with(expected_prefix) {
                        errors.push(format!(
                            "term-id '{}' doesn't match domain '{}' (expected {})",
                            term_id, domain, expected_prefix
                        ));
                    }
                }
            }
        }

        // Validate namespace consistency
        if let Some(ref owl_class) = self.owl_class {
            if let Some(colon_idx) = owl_class.find(':') {
                let prefix = &owl_class[..colon_idx];
                if let Some(ref domain) = self.source_domain {
                    let domain_lower = domain.to_lowercase();
                    if prefix != domain_lower && DOMAIN_PREFIXES.values().any(|&d| d == prefix) {
                        errors.push(format!(
                            "owl:class namespace '{}' doesn't match source-domain '{}'",
                            prefix, domain
                        ));
                    }
                }
            }
        }

        errors
    }
}

/// Legacy output structure for backward compatibility
#[derive(Debug)]
pub struct OntologyData {
    pub classes: Vec<OwlClass>,
    pub properties: Vec<OwlProperty>,
    pub axioms: Vec<OwlAxiom>,
    pub class_hierarchy: Vec<(String, String)>,
}

// ============================================================================
// Parser Implementation
// ============================================================================

pub struct OntologyParser;

impl OntologyParser {
    pub fn new() -> Self {
        Self
    }

    /// Parse a markdown file and extract the complete ontology block.
    ///
    /// Auto-detects v2 vs v4 format and dispatches accordingly.
    pub fn parse_enhanced(&self, content: &str, filename: &str) -> Result<OntologyBlock, String> {
        info!("Parsing ontology file (enhanced): {}", filename);

        match detect_format_version(content) {
            Some(OntologyFormatVersion::V2) => {
                debug!("Detected v2 (IRI-first) format for {}", filename);
                self.parse_v2(content, filename)
            }
            Some(OntologyFormatVersion::V4) => {
                debug!("Detected v4 (OntologyBlock) format for {}", filename);
                self.parse_v4(content, filename)
            }
            None => Err(format!(
                "No recognised ontology format in '{}': expected v2 (iri:: on line 1 + rdf-type:: owl:Class) or v4 (### OntologyBlock)",
                filename
            )),
        }
    }

    /// Legacy parse method for backward compatibility.
    ///
    /// Returns the flat `OntologyData` projection used by the graph ingest pipeline.
    pub fn parse(&self, content: &str, filename: &str) -> Result<OntologyData, String> {
        let block = self.parse_enhanced(content, filename)?;

        // Convert to legacy format
        let classes = self.block_to_classes(&block);
        let properties = Vec::new();
        let axioms = self.block_to_axioms(&block);
        let class_hierarchy = block
            .is_subclass_of
            .iter()
            .filter_map(|parent| {
                block
                    .owl_class
                    .as_ref()
                    .map(|cls| (cls.clone(), parent.clone()))
            })
            .collect();

        Ok(OntologyData {
            classes,
            properties,
            axioms,
            class_hierarchy,
        })
    }

    // ========================================================================
    // v2 Parser (VisionClaw IRI-first format)
    // ========================================================================

    /// Parse a v2 (IRI-first) ontology page.
    ///
    /// v2 pages have flat `key:: value` properties at the top of the file,
    /// then `### Section` headers with indented `- key:: value` properties.
    fn parse_v2(&self, content: &str, filename: &str) -> Result<OntologyBlock, String> {
        let mut block = OntologyBlock::new(filename.to_string());
        block.raw_block = content.to_string();

        // --- Phase 1: Extract flat page-level properties ---
        let page_props = self.extract_v2_page_properties(content);

        // IRI-first fields
        block.iri = page_props.get("iri").cloned();
        block.uri = page_props.get("uri").cloned();
        block.rdf_type = page_props.get("rdf-type").cloned();
        block.same_as = page_props.get("same-as").cloned();
        block.content_hash = page_props.get("content-hash").cloned();

        // Identification
        block.preferred_term = page_props.get("preferred-term").cloned();
        block.source_domain = page_props.get("domain").cloned();
        block.status = page_props.get("status").cloned();
        block.version = page_props.get("version").cloned();

        // v2 uses `legacy-term-id` for the old sequential ID
        block.term_id = page_props.get("legacy-term-id").cloned();

        // Quality metrics
        if let Some(val) = page_props.get("quality-score") {
            block.quality_score = val.parse::<f64>().ok();
        }
        if let Some(val) = page_props.get("authority-score") {
            block.authority_score = val.parse::<f64>().ok();
        }

        // Maturity
        block.maturity = page_props.get("maturity").cloned();

        // Dates — v2 uses `created` / `modified` instead of `last-updated`
        block.last_updated = page_props
            .get("modified")
            .or_else(|| page_props.get("created"))
            .cloned();

        // Public access
        if let Some(val) = page_props.get("public") {
            block.public_access = Some(val.to_lowercase() == "true");
        }

        // ontology flag: v2 pages with rdf-type:: owl:Class are ontology pages
        block.ontology = block.rdf_type.as_deref() == Some("owl:Class");

        // --- Phase 2: Extract section properties ---
        let sections = self.extract_v2_sections(content);

        // Semantic Classification section
        if let Some(sem_props) = sections.get("Semantic Classification") {
            block.owl_class = sem_props.get("owl-class").cloned();
            block.owl_role = sem_props.get("owl-role").cloned();
            block.owl_inferred_class = sem_props.get("owl-inferred").cloned();

            if let Some(val) = sem_props.get("belongs-to-domain") {
                block.belongs_to_domain = self.extract_wiki_links(val);
            }
            if let Some(val) = sem_props.get("implemented-in-layer") {
                block.implemented_in_layer = self.extract_wiki_links(val);
            }
        }

        // Relationships section
        if let Some(rel_props) = sections.get("Relationships") {
            if let Some(val) = rel_props.get("is-subclass-of") {
                block.is_subclass_of = self.extract_wiki_links(val);
            }
            if let Some(val) = rel_props.get("has-part") {
                block.has_part = self.extract_wiki_links(val);
            }
            if let Some(val) = rel_props.get("is-part-of") {
                block.is_part_of = self.extract_wiki_links(val);
            }
            if let Some(val) = rel_props.get("requires") {
                block.requires = self.extract_wiki_links(val);
            }
            if let Some(val) = rel_props.get("enables") {
                block.enables = self.extract_wiki_links(val);
            }
            if let Some(val) = rel_props.get("implements") {
                block.implements = self.extract_wiki_links(val);
            }
            if let Some(val) = rel_props.get("depends-on") {
                block.depends_on = self.extract_wiki_links(val);
            }
            if let Some(val) = rel_props.get("relates-to") {
                block.relates_to = self.extract_wiki_links(val);
            }

            // bridges-to with domain annotation parsing
            if let Some(val) = rel_props.get("bridges-to") {
                block.bridges_to = self.extract_v2_bridges(val);
            }
            if let Some(val) = rel_props.get("bridges-from") {
                block.bridges_from = self.extract_v2_bridges(val);
            }

            // Collect unknown relationship types into other_relationships
            let known_rels = [
                "is-subclass-of",
                "has-part",
                "is-part-of",
                "requires",
                "depends-on",
                "enables",
                "relates-to",
                "implements",
                "bridges-to",
                "bridges-from",
            ];
            for (key, val) in rel_props.iter() {
                if !known_rels.contains(&key.as_str()) {
                    let links = self.extract_wiki_links(val);
                    if !links.is_empty() {
                        block
                            .other_relationships
                            .entry(key.clone())
                            .or_default()
                            .extend(links);
                    }
                }
            }
        }

        // Definition section — extract the first non-property text line
        if let Some(def) = self.extract_v2_definition(content) {
            block.definition = Some(def);
        }

        // OWL axioms from code blocks (same logic as v4)
        self.extract_owl_axioms(content, &mut block);

        debug!(
            "Parsed v2 {}: iri={:?}, domain={:?}, relationships={}",
            filename,
            block.iri,
            block.get_domain(),
            block.is_subclass_of.len()
                + block.has_part.len()
                + block.requires.len()
                + block.enables.len()
                + block.implements.len()
        );

        Ok(block)
    }

    /// Extract flat `key:: value` properties from the page level (before any
    /// `### Section` header).
    fn extract_v2_page_properties(&self, content: &str) -> HashMap<String, String> {
        let mut props = HashMap::new();
        for line in content.lines() {
            // Stop at the first markdown section header
            let trimmed = line.trim_start_matches("- ").trim();
            if trimmed.starts_with("### ") || trimmed.starts_with("## ") {
                break;
            }
            if let Some(caps) = V2_PROPERTY_PATTERN.captures(line) {
                if let (Some(key), Some(val)) = (caps.get(1), caps.get(2)) {
                    props.insert(key.as_str().to_string(), val.as_str().trim().to_string());
                }
            }
        }
        props
    }

    /// Extract properties grouped by `### Section` header.
    ///
    /// Returns a map of section name → (property name → raw value string).
    fn extract_v2_sections(
        &self,
        content: &str,
    ) -> HashMap<String, HashMap<String, String>> {
        let mut sections: HashMap<String, HashMap<String, String>> = HashMap::new();
        let mut current_section: Option<String> = None;

        for line in content.lines() {
            // Detect `- ### SectionName` or `### SectionName`
            let trimmed = line.trim_start_matches("- ").trim();
            if trimmed.starts_with("### ") {
                let name = trimmed.trim_start_matches("### ").trim().to_string();
                current_section = Some(name);
                continue;
            }

            // Collect indented section properties
            if let Some(ref section) = current_section {
                if let Some(caps) = V2_SECTION_PROPERTY_PATTERN.captures(line) {
                    if let (Some(key), Some(val)) = (caps.get(1), caps.get(2)) {
                        sections
                            .entry(section.clone())
                            .or_default()
                            .insert(key.as_str().to_string(), val.as_str().trim().to_string());
                    }
                }
            }
        }

        sections
    }

    /// Extract the definition text from under `### Definition`.
    fn extract_v2_definition(&self, content: &str) -> Option<String> {
        let mut in_definition = false;
        for line in content.lines() {
            let trimmed = line.trim_start_matches("- ").trim();
            if trimmed.starts_with("### Definition") {
                in_definition = true;
                continue;
            }
            if in_definition {
                // Stop at next section
                if trimmed.starts_with("### ") || trimmed.starts_with("## ") {
                    break;
                }
                // The definition line is typically `  - <text>`
                let stripped = trimmed.trim_start_matches("- ").trim();
                if !stripped.is_empty() {
                    return Some(stripped.to_string());
                }
            }
        }
        None
    }

    /// Parse v2 bridge values like `[[Target]] (domain: robotics), [[Other]] (domain: blockchain)`.
    ///
    /// Returns a list of strings in the form `"Target (domain: robotics)"`.
    fn extract_v2_bridges(&self, value: &str) -> Vec<String> {
        let mut bridges = Vec::new();

        for caps in V2_BRIDGE_DOMAIN_PATTERN.captures_iter(value) {
            if let (Some(target), Some(domain)) = (caps.get(1), caps.get(2)) {
                let target_str = target.as_str().trim();
                let domain_str = domain.as_str().trim();
                bridges.push(format!("{} (domain: {})", target_str, domain_str));
            }
        }

        // Fall back: if no domain annotations found, extract plain wiki-links
        if bridges.is_empty() {
            for link in self.extract_wiki_links(value) {
                bridges.push(link);
            }
        }

        bridges
    }

    // ========================================================================
    // v4 Parser (OntologyBlock format) — formerly the only parser
    // ========================================================================

    /// Parse a v4 (OntologyBlock) ontology page.
    fn parse_v4(&self, content: &str, filename: &str) -> Result<OntologyBlock, String> {
        // Extract ontology section
        let ontology_section = self.extract_ontology_section(content)?;

        // Create block object
        let mut block = OntologyBlock::new(filename.to_string());
        block.raw_block = ontology_section.clone();

        // === Extract Tier 1 Properties ===
        self.extract_tier1_properties(&ontology_section, &mut block);

        // === Extract Tier 2 Properties ===
        self.extract_tier2_properties(&ontology_section, &mut block);

        // === Extract Tier 3 Properties ===
        self.extract_tier3_properties(&ontology_section, &mut block);

        // === Extract Relationships ===
        self.extract_relationships(&ontology_section, &mut block);

        // === Extract Cross-Domain Bridges ===
        self.extract_bridges(&ontology_section, &mut block);

        // === Extract OWL Axioms ===
        self.extract_owl_axioms(content, &mut block);

        // === Extract Domain Extensions ===
        self.extract_domain_extensions(&ontology_section, &mut block);

        debug!(
            "Parsed v4 {}: term_id={:?}, domain={:?}, relationships={}",
            filename,
            block.term_id,
            block.get_domain(),
            block.is_subclass_of.len()
                + block.has_part.len()
                + block.requires.len()
                + block.enables.len()
        );

        Ok(block)
    }

    // ========================================================================
    // v4 Section Extraction
    // ========================================================================

    fn extract_ontology_section(&self, content: &str) -> Result<String, String> {
        let lines: Vec<&str> = content.lines().collect();
        let mut section_start = None;

        for (i, line) in lines.iter().enumerate() {
            if line.contains("### OntologyBlock") {
                section_start = Some(i);
                break;
            }
        }

        let start = section_start.ok_or_else(|| "No OntologyBlock found in file".to_string())?;
        let section: Vec<&str> = lines[start..].iter().copied().collect();

        Ok(section.join("\n"))
    }

    // ========================================================================
    // v4 Tier 1 Property Extraction
    // ========================================================================

    fn extract_tier1_properties(&self, section: &str, block: &mut OntologyBlock) {
        // Identification
        if let Some(val) = self.extract_property(section, "ontology") {
            block.ontology = val.to_lowercase() == "true";
        }
        block.term_id = self.extract_property(section, "term-id");
        block.preferred_term = self.extract_property(section, "preferred-term");
        block.source_domain = self.extract_property(section, "source-domain");
        block.status = self.extract_property(section, "status");

        if let Some(val) = self.extract_property(section, "public-access") {
            block.public_access = Some(val.to_lowercase() == "true");
        }

        block.last_updated = self.extract_property(section, "last-updated");

        // Definition
        block.definition = self.extract_property(section, "definition");

        // Semantic Classification
        block.owl_class = self.extract_property(section, "owl:class");
        block.owl_physicality = self.extract_property(section, "owl:physicality");
        block.owl_role = self.extract_property(section, "owl:role");
    }

    // ========================================================================
    // v4 Tier 2 Property Extraction
    // ========================================================================

    fn extract_tier2_properties(&self, section: &str, block: &mut OntologyBlock) {
        // Identification (Tier 2)
        block.alt_terms = self.extract_property_list(section, "alt-terms");
        block.version = self.extract_property(section, "version");

        if let Some(val) = self.extract_property(section, "quality-score") {
            block.quality_score = val.parse::<f64>().ok();
        }

        if let Some(val) = self.extract_property(section, "cross-domain-links") {
            block.cross_domain_links = val.parse::<i32>().ok();
        }

        // Definition (Tier 2)
        block.maturity = self.extract_property(section, "maturity");
        block.source = self.extract_property_list(section, "source");

        if let Some(val) = self.extract_property(section, "authority-score") {
            block.authority_score = val.parse::<f64>().ok();
        }

        block.scope_note = self.extract_property(section, "scope-note");

        // Semantic Classification (Tier 2)
        block.owl_inferred_class = self.extract_property(section, "owl:inferred-class");
        block.belongs_to_domain = self.extract_property_list(section, "belongsToDomain");
    }

    // ========================================================================
    // v4 Tier 3 Property Extraction
    // ========================================================================

    fn extract_tier3_properties(&self, section: &str, block: &mut OntologyBlock) {
        block.implemented_in_layer = self.extract_property_list(section, "implementedInLayer");
    }

    // ========================================================================
    // v4 Relationship Extraction
    // ========================================================================

    fn extract_relationships(&self, section: &str, block: &mut OntologyBlock) {
        // Known relationships
        block.is_subclass_of = self.extract_property_list(section, "is-subclass-of");
        block.has_part = self.extract_property_list(section, "has-part");
        block.is_part_of = self.extract_property_list(section, "is-part-of");
        block.requires = self.extract_property_list(section, "requires");
        block.depends_on = self.extract_property_list(section, "depends-on");
        block.enables = self.extract_property_list(section, "enables");
        block.relates_to = self.extract_property_list(section, "relates-to");
        block.implements = self.extract_property_list(section, "implements");

        // Extract other relationships from Relationships section
        let relationships_section = self.extract_relationships_section(section);

        let known_rels = vec![
            "is-subclass-of",
            "has-part",
            "is-part-of",
            "requires",
            "depends-on",
            "enables",
            "relates-to",
            "implements",
            "bridges-to",
            "bridges-from",
        ];

        for (rel_name, targets) in relationships_section {
            if !known_rels.contains(&rel_name.as_str()) {
                block.other_relationships.insert(rel_name, targets);
            }
        }
    }

    fn extract_relationships_section(&self, section: &str) -> HashMap<String, Vec<String>> {
        let mut relationships = HashMap::new();
        let mut in_relationships = false;

        for line in section.lines() {
            // Check for Relationships section header
            if line.contains("#### Relationships") {
                in_relationships = true;
                continue;
            }

            // Stop at next section
            if in_relationships && line.contains("####") && !line.contains("Relationships") {
                break;
            }

            if in_relationships {
                if let Some(caps) = PROPERTY_PATTERN.captures(line) {
                    let (prop_name, value_text) = match (caps.get(1), caps.get(2)) {
                        (Some(p), Some(v)) => (p.as_str().to_string(), v.as_str()),
                        _ => continue,
                    };

                    // Extract wiki-links
                    let wiki_links = self.extract_wiki_links(value_text);
                    if !wiki_links.is_empty() {
                        relationships
                            .entry(prop_name)
                            .or_insert_with(Vec::new)
                            .extend(wiki_links);
                    }
                }
            }
        }

        relationships
    }

    // ========================================================================
    // v4 Cross-Domain Bridges Extraction
    // ========================================================================

    fn extract_bridges(&self, section: &str, block: &mut OntologyBlock) {
        for line in section.lines() {
            if let Some(caps) = BRIDGE_PATTERN.captures(line) {
                let (direction, target, via) = match (caps.get(1), caps.get(2), caps.get(3)) {
                    (Some(d), Some(t), Some(v)) => (d.as_str(), t.as_str(), v.as_str()),
                    _ => continue,
                };

                let bridge_str = format!("{} via {}", target, via);

                if direction == "bridges-to" {
                    block.bridges_to.push(bridge_str);
                } else {
                    block.bridges_from.push(bridge_str);
                }
            }
        }
    }

    // ========================================================================
    // OWL Axioms Extraction (shared by v2 and v4)
    // ========================================================================

    fn extract_owl_axioms(&self, content: &str, block: &mut OntologyBlock) {
        for caps in OWL_AXIOM_PATTERN.captures_iter(content) {
            if let Some(axiom_match) = caps.get(1) {
                block.owl_axioms.push(axiom_match.as_str().trim().to_string());
            }
        }
    }

    // ========================================================================
    // v4 Domain Extension Properties
    // ========================================================================

    fn extract_domain_extensions(&self, section: &str, block: &mut OntologyBlock) {
        let domain = match block.get_domain() {
            Some(d) => d,
            None => return,
        };

        // Domain-specific properties based on canonical schema
        let extension_props = match domain.as_str() {
            "ai" | "artificial-intelligence" => vec!["algorithm-type", "computational-complexity"],
            "bc" | "blockchain" => vec!["consensus-mechanism", "decentralization-level"],
            "rb" | "robotics" => vec!["physicality", "autonomy-level"],
            "mv" | "spatial-computing" => vec!["immersion-level", "interaction-mode"],
            "tc" | "distributed-collaboration" => vec!["collaboration-type", "communication-mode"],
            "dt" => vec!["disruption-level", "maturity-stage"],
            _ => vec![],
        };

        for prop in extension_props {
            if let Some(val) = self.extract_property(section, prop) {
                block.domain_extensions.insert(prop.to_string(), val);
            }
        }
    }

    // ========================================================================
    // Property Extraction Utilities (v4 indented `- key:: value`)
    // ========================================================================

    fn extract_property(&self, section: &str, property_name: &str) -> Option<String> {
        for line in section.lines() {
            if let Some(caps) = PROPERTY_PATTERN.captures(line) {
                let prop = match caps.get(1) {
                    Some(m) => m.as_str(),
                    None => continue,
                };
                if prop == property_name {
                    let value = match caps.get(2) {
                        Some(m) => m.as_str().trim(),
                        None => continue,
                    };
                    // Remove wiki-link brackets if present
                    let cleaned = if value.starts_with("[[") && value.ends_with("]]") {
                        &value[2..value.len() - 2]
                    } else {
                        value
                    };
                    return Some(cleaned.to_string());
                }
            }
        }
        None
    }

    fn extract_property_list(&self, section: &str, property_name: &str) -> Vec<String> {
        let mut result = Vec::new();

        for line in section.lines() {
            if let Some(caps) = PROPERTY_PATTERN.captures(line) {
                let prop = match caps.get(1) {
                    Some(m) => m.as_str(),
                    None => continue,
                };
                if prop == property_name {
                    let value_text = match caps.get(2) {
                        Some(m) => m.as_str(),
                        None => continue,
                    };
                    let wiki_links = self.extract_wiki_links(value_text);
                    result.extend(wiki_links);
                }
            }
        }

        result
    }

    fn extract_wiki_links(&self, text: &str) -> Vec<String> {
        WIKI_LINK_PATTERN
            .captures_iter(text)
            .filter_map(|caps| caps.get(1).map(|m| m.as_str().trim().to_string()))
            .collect()
    }

    // ========================================================================
    // Legacy Conversion Methods
    // ========================================================================

    fn block_to_classes(&self, block: &OntologyBlock) -> Vec<OwlClass> {
        // For v2, owl_class comes from the Semantic Classification section.
        // For v4, it comes from the owl:class property.
        // We also accept the iri field as the IRI if owl_class is absent.
        let has_identity = block.owl_class.is_some() || block.iri.is_some();
        if !has_identity {
            return Vec::new();
        }

        let mut properties = HashMap::new();
        properties.insert("source_file".to_string(), block.file_path.clone());

        if let Some(ref term_id) = block.term_id {
            properties.insert("term_id".to_string(), term_id.clone());
        }
        if let Some(ref status) = block.status {
            properties.insert("status".to_string(), status.clone());
        }
        if let Some(ref content_hash) = block.content_hash {
            properties.insert("content_hash".to_string(), content_hash.clone());
        }
        if let Some(ref uri) = block.uri {
            properties.insert("uri".to_string(), uri.clone());
        }
        if let Some(ref rdf_type) = block.rdf_type {
            properties.insert("rdf_type".to_string(), rdf_type.clone());
        }
        if let Some(ref same_as) = block.same_as {
            properties.insert("same_as".to_string(), same_as.clone());
        }

        // IRI: prefer the explicit iri field (v2), fall back to owl_class (v4)
        let iri = block
            .iri
            .clone()
            .or_else(|| block.owl_class.clone())
            .unwrap_or_default();

        vec![OwlClass {
            iri,
            term_id: block.term_id.clone(),
            preferred_term: block.preferred_term.clone(),
            label: block.preferred_term.clone(),
            description: block.definition.clone(),
            parent_classes: block.is_subclass_of.clone(),
            source_domain: block.source_domain.clone(),
            version: block.version.clone(),
            class_type: block.owl_class.clone(),
            status: block.status.clone(),
            maturity: block.maturity.clone(),
            quality_score: block.quality_score.map(|v| v as f32),
            authority_score: block.authority_score.map(|v| v as f32),
            public_access: block.public_access,
            content_status: None,
            owl_physicality: block.owl_physicality.clone(),
            owl_role: block.owl_role.clone(),
            belongs_to_domain: block.belongs_to_domain.first().cloned(),
            bridges_to_domain: None,
            source_file: Some(block.file_path.clone()),
            file_sha1: None,
            markdown_content: Some(block.raw_block.clone()),
            last_synced: None,
            // ADR-014: Carry all relationship types through to storage
            has_part: block.has_part.clone(),
            is_part_of: block.is_part_of.clone(),
            requires: block.requires.clone(),
            depends_on: block.depends_on.clone(),
            enables: block.enables.clone(),
            relates_to: block.relates_to.clone(),
            bridges_to: block.bridges_to.clone(),
            bridges_from: block.bridges_from.clone(),
            other_relationships: block.other_relationships.clone(),
            properties,
            additional_metadata: None,
            // VisionClaw v2 identifiers
            canonical_iri: block.iri.clone(),
            visionclaw_uri: block.uri.clone(),
            content_hash: block.content_hash.clone(),
        }]
    }

    fn block_to_axioms(&self, block: &OntologyBlock) -> Vec<OwlAxiom> {
        let mut axioms = Vec::new();

        // Determine the subject: prefer owl_class, fall back to iri
        let subject = block
            .owl_class
            .clone()
            .or_else(|| block.iri.clone());

        // SubClassOf axioms
        if let Some(ref subj) = subject {
            for parent in &block.is_subclass_of {
                axioms.push(OwlAxiom {
                    id: None,
                    axiom_type: AxiomType::SubClassOf,
                    subject: subj.clone(),
                    object: parent.clone(),
                    annotations: HashMap::new(),
                });
            }
        }

        axioms
    }
}

impl Default for OntologyParser {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ====================================================================
    // Format detection tests
    // ====================================================================

    #[test]
    fn test_detect_v2_format() {
        let content = "iri:: http://narrativegoldmine.com/ai#Test\nuri:: urn:visionclaw:concept:ai:test\nrdf-type:: owl:Class\n";
        assert_eq!(
            detect_format_version(content),
            Some(OntologyFormatVersion::V2)
        );
    }

    #[test]
    fn test_detect_v4_format() {
        let content = "# Test\n\n- ### OntologyBlock\n  - term-id:: AI-0001\n";
        assert_eq!(
            detect_format_version(content),
            Some(OntologyFormatVersion::V4)
        );
    }

    #[test]
    fn test_detect_no_ontology() {
        let content = "# Just a normal page\n\nSome text here.\n";
        assert_eq!(detect_format_version(content), None);
    }

    #[test]
    fn test_detect_v2_requires_rdf_type() {
        // Has iri:: on line 1 but no rdf-type:: owl:Class
        let content = "iri:: http://example.com/test\npreferred-term:: Test\n";
        assert_eq!(detect_format_version(content), None);
    }

    // ====================================================================
    // v2 parser tests
    // ====================================================================

    #[test]
    fn test_parse_v2_complete() {
        let parser = OntologyParser::new();
        let content = r#"iri:: http://narrativegoldmine.com/artificial-intelligence#AIAgentSystem
uri:: urn:visionclaw:concept:artificial-intelligence:ai-agent-system
rdf-type:: owl:Class
same-as:: urn:visionclaw:concept:artificial-intelligence:ai-agent-system
type:: owl:Class
context:: https://visionclaw.dreamlab-ai.systems/ns/v2
domain:: artificial-intelligence
preferred-term:: AI Agent System
content-hash:: sha256-12-5916d15f1fe9
legacy-term-id:: AI-0600
status:: complete
maturity:: mature
quality-score:: 0.92
authority-score:: 0.95
version:: 2.0.0
created:: 2025-11-05T00:00:00Z
modified:: 2026-04-26T00:00:00Z
public:: true

- ### Definition
  - An autonomous software entity that perceives its environment.

- ### Semantic Classification
  - owl-class:: artificial-intelligence:AIAgentSystem
  - owl-role:: Agent
  - owl-inferred:: ai:VirtualAgent
  - belongs-to-domain:: [[AI-GroundedDomain]], [[ComputationAndIntelligenceDomain]]
  - implemented-in-layer:: [[ComputeLayer]], [[DataLayer]]

- ### Relationships
  - is-subclass-of:: [[Autonomous System]]
  - has-part:: [[Perception System]], [[Decision Engine]]
  - requires:: [[Sensor Input]], [[Environment Model]]
  - enables:: [[Autonomous Operation]], [[Adaptive Behavior]]
  - implements:: [[Reinforcement Learning]], [[Planning Algorithm]]
  - bridges-to:: [[Intelligent Virtual Entity]] (domain: metaverse), [[Autonomous Robot]] (domain: robotics)
  - depends-on:: [[API Access]]
"#;

        let result = parser.parse_enhanced(content, "AI Agent System.md");
        assert!(result.is_ok(), "Parse failed: {:?}", result.err());

        let block = result.unwrap();

        // v2-specific fields
        assert_eq!(
            block.iri,
            Some("http://narrativegoldmine.com/artificial-intelligence#AIAgentSystem".to_string())
        );
        assert_eq!(
            block.uri,
            Some("urn:visionclaw:concept:artificial-intelligence:ai-agent-system".to_string())
        );
        assert_eq!(block.rdf_type, Some("owl:Class".to_string()));
        assert_eq!(
            block.same_as,
            Some("urn:visionclaw:concept:artificial-intelligence:ai-agent-system".to_string())
        );
        assert_eq!(
            block.content_hash,
            Some("sha256-12-5916d15f1fe9".to_string())
        );

        // Identification
        assert_eq!(block.preferred_term, Some("AI Agent System".to_string()));
        assert_eq!(
            block.source_domain,
            Some("artificial-intelligence".to_string())
        );
        assert_eq!(block.status, Some("complete".to_string()));
        assert_eq!(block.term_id, Some("AI-0600".to_string()));
        assert_eq!(block.version, Some("2.0.0".to_string()));
        assert_eq!(block.public_access, Some(true));
        assert!(block.ontology);

        // Quality
        assert_eq!(block.quality_score, Some(0.92));
        assert_eq!(block.authority_score, Some(0.95));
        assert_eq!(block.maturity, Some("mature".to_string()));

        // Semantic Classification
        assert_eq!(
            block.owl_class,
            Some("artificial-intelligence:AIAgentSystem".to_string())
        );
        assert_eq!(block.owl_role, Some("Agent".to_string()));
        assert_eq!(
            block.owl_inferred_class,
            Some("ai:VirtualAgent".to_string())
        );
        assert_eq!(
            block.belongs_to_domain,
            vec!["AI-GroundedDomain", "ComputationAndIntelligenceDomain"]
        );
        assert_eq!(
            block.implemented_in_layer,
            vec!["ComputeLayer", "DataLayer"]
        );

        // Relationships
        assert_eq!(block.is_subclass_of, vec!["Autonomous System"]);
        assert_eq!(block.has_part, vec!["Perception System", "Decision Engine"]);
        assert_eq!(block.requires, vec!["Sensor Input", "Environment Model"]);
        assert_eq!(
            block.enables,
            vec!["Autonomous Operation", "Adaptive Behavior"]
        );
        assert_eq!(
            block.implements,
            vec!["Reinforcement Learning", "Planning Algorithm"]
        );
        assert_eq!(block.depends_on, vec!["API Access"]);

        // Bridges with domain annotation
        assert_eq!(block.bridges_to.len(), 2);
        assert!(block.bridges_to[0].contains("Intelligent Virtual Entity"));
        assert!(block.bridges_to[0].contains("metaverse"));
        assert!(block.bridges_to[1].contains("Autonomous Robot"));
        assert!(block.bridges_to[1].contains("robotics"));

        // Definition
        assert!(block.definition.is_some());
        assert!(block
            .definition
            .as_ref()
            .unwrap()
            .contains("autonomous software entity"));

        // Full IRI resolution
        let full_iri = block.get_full_iri();
        assert_eq!(
            full_iri,
            Some("http://narrativegoldmine.com/artificial-intelligence#AIAgentSystem".to_string())
        );
    }

    #[test]
    fn test_parse_v2_legacy_compatibility() {
        let parser = OntologyParser::new();
        let content = r#"iri:: http://narrativegoldmine.com/blockchain#SmartContract
uri:: urn:visionclaw:concept:blockchain:smart-contract
rdf-type:: owl:Class
same-as:: urn:visionclaw:concept:blockchain:smart-contract
domain:: blockchain
preferred-term:: Smart Contract
content-hash:: sha256-12-abcdef123456
legacy-term-id:: BC-0010
status:: complete
quality-score:: 0.85
authority-score:: 0.90
public:: true

- ### Definition
  - A self-executing program stored on a blockchain.

- ### Semantic Classification
  - owl-class:: blockchain:SmartContract
  - owl-role:: Process

- ### Relationships
  - is-subclass-of:: [[Distributed Application]]
  - requires:: [[Blockchain Network]]
"#;

        let result = parser.parse(content, "Smart Contract.md");
        assert!(result.is_ok());

        let data = result.unwrap();
        assert_eq!(data.classes.len(), 1);
        assert_eq!(
            data.classes[0].iri,
            "http://narrativegoldmine.com/blockchain#SmartContract"
        );
        assert_eq!(
            data.classes[0].preferred_term,
            Some("Smart Contract".to_string())
        );
        assert_eq!(data.axioms.len(), 1);
        assert_eq!(data.class_hierarchy.len(), 1);

        // Verify properties carry through
        assert_eq!(
            data.classes[0].properties.get("content_hash"),
            Some(&"sha256-12-abcdef123456".to_string())
        );
        assert_eq!(
            data.classes[0].properties.get("uri"),
            Some(&"urn:visionclaw:concept:blockchain:smart-contract".to_string())
        );
    }

    #[test]
    fn test_v2_domain_namespace_resolution() {
        let mut block = OntologyBlock::new("test.md".to_string());
        block.owl_class = Some("artificial-intelligence:TestConcept".to_string());

        let iri = block.get_full_iri();
        assert_eq!(
            iri,
            Some(
                "http://narrativegoldmine.com/artificial-intelligence#TestConcept".to_string()
            )
        );
    }

    #[test]
    fn test_v2_iri_field_takes_precedence() {
        let mut block = OntologyBlock::new("test.md".to_string());
        block.iri =
            Some("http://narrativegoldmine.com/robotics#TestRobot".to_string());
        block.owl_class = Some("robotics:TestRobot".to_string());

        let iri = block.get_full_iri();
        assert_eq!(
            iri,
            Some("http://narrativegoldmine.com/robotics#TestRobot".to_string())
        );
    }

    #[test]
    fn test_v2_bridge_domain_parsing() {
        let parser = OntologyParser::new();
        let value =
            "[[Robot Control]] (domain: robotics), [[Smart Contract]] (domain: blockchain - for autonomous agents)";
        let bridges = parser.extract_v2_bridges(value);
        assert_eq!(bridges.len(), 2);
        assert_eq!(bridges[0], "Robot Control (domain: robotics)");
        assert!(bridges[1].contains("Smart Contract"));
        assert!(bridges[1].contains("blockchain - for autonomous agents"));
    }

    // ====================================================================
    // v4 parser tests (unchanged from original)
    // ====================================================================

    #[test]
    fn test_parse_enhanced_complete_block() {
        let parser = OntologyParser::new();
        let content = r#"
# Large Language Models

- ### OntologyBlock
  id:: llm-ontology
  collapsed:: true

  - **Identification**
    - ontology:: true
    - term-id:: AI-0850
    - preferred-term:: Large Language Models
    - alt-terms:: [[LLM]], [[Foundation Models]]
    - source-domain:: ai
    - status:: complete
    - public-access:: true
    - version:: 1.0.0
    - last-updated:: 2025-11-21
    - quality-score:: 0.85

  - **Definition**
    - definition:: A Large Language Model (LLM) is an artificial intelligence system based on deep neural networks (typically [[Transformer]] architectures) trained on vast text corpora to understand and generate human-like text.
    - maturity:: mature
    - source:: [[OpenAI Research]], [[Stanford AI Index]]
    - authority-score:: 0.95

  - **Semantic Classification**
    - owl:class:: ai:LargeLanguageModel
    - owl:physicality:: VirtualEntity
    - owl:role:: Process
    - belongsToDomain:: [[AI-GroundedDomain]]

  - #### Relationships
    id:: llm-relationships
    - is-subclass-of:: [[Artificial Intelligence]], [[Neural Network Architecture]]
    - requires:: [[Training Data]], [[Computational Resources]]
    - enables:: [[Few-Shot Learning]], [[Text Generation]]
"#;

        let result = parser.parse_enhanced(content, "test.md");
        assert!(result.is_ok());

        let block = result.unwrap();

        // Tier 1 properties
        assert_eq!(block.term_id, Some("AI-0850".to_string()));
        assert_eq!(
            block.preferred_term,
            Some("Large Language Models".to_string())
        );
        assert_eq!(block.source_domain, Some("ai".to_string()));
        assert_eq!(block.status, Some("complete".to_string()));
        assert_eq!(block.public_access, Some(true));
        assert_eq!(block.owl_class, Some("ai:LargeLanguageModel".to_string()));
        assert_eq!(block.owl_physicality, Some("VirtualEntity".to_string()));
        assert_eq!(block.owl_role, Some("Process".to_string()));

        // Tier 2 properties
        assert_eq!(block.alt_terms, vec!["LLM", "Foundation Models"]);
        assert_eq!(block.version, Some("1.0.0".to_string()));
        assert_eq!(block.quality_score, Some(0.85));
        assert_eq!(block.maturity, Some("mature".to_string()));
        assert_eq!(block.authority_score, Some(0.95));

        // Relationships
        assert_eq!(block.is_subclass_of.len(), 2);
        assert!(block
            .is_subclass_of
            .contains(&"Artificial Intelligence".to_string()));
        assert_eq!(
            block.requires,
            vec!["Training Data", "Computational Resources"]
        );
        assert_eq!(block.enables, vec!["Few-Shot Learning", "Text Generation"]);

        // Validation
        let errors = block.validate();
        assert!(errors.is_empty(), "Validation errors: {:?}", errors);
    }

    #[test]
    fn test_parse_enhanced_with_bridges() {
        let parser = OntologyParser::new();
        let content = r#"
- ### OntologyBlock
  - ontology:: true
  - term-id:: AI-0001
  - preferred-term:: Test Concept
  - source-domain:: ai
  - status:: complete
  - public-access:: true
  - last-updated:: 2025-11-21
  - definition:: A test concept
  - owl:class:: ai:TestConcept
  - owl:physicality:: VirtualEntity
  - owl:role:: Concept

  - #### Relationships
    - is-subclass-of:: [[owl:Thing]]

  - #### CrossDomainBridges
    - bridges-to:: [[Blockchain Verification]] via enables
    - bridges-from:: [[Robot Control]] via requires
"#;

        let result = parser.parse_enhanced(content, "test.md");
        assert!(result.is_ok());

        let block = result.unwrap();
        assert_eq!(
            block.bridges_to,
            vec!["Blockchain Verification via enables"]
        );
        assert_eq!(block.bridges_from, vec!["Robot Control via requires"]);
    }

    #[test]
    fn test_parse_enhanced_with_owl_axioms() {
        let parser = OntologyParser::new();
        let content = r#"
- ### OntologyBlock
  - term-id:: AI-0002
  - preferred-term:: Test
  - source-domain:: ai
  - status:: draft
  - public-access:: true
  - last-updated:: 2025-11-21
  - definition:: Test
  - owl:class:: ai:Test
  - owl:physicality:: VirtualEntity
  - owl:role:: Concept

  - #### Relationships
    - is-subclass-of:: [[owl:Thing]]

  - #### OWL Axioms
    - ```clojure
      (Declaration (Class :TestConcept))
      (SubClassOf :TestConcept :ParentClass)
      ```
"#;

        let result = parser.parse_enhanced(content, "test.md");
        assert!(result.is_ok());

        let block = result.unwrap();
        assert_eq!(block.owl_axioms.len(), 1);
        assert!(block.owl_axioms[0].contains("Declaration"));
    }

    #[test]
    fn test_validation_missing_required() {
        let mut block = OntologyBlock::new("test.md".to_string());
        block.term_id = Some("AI-0001".to_string());
        // Missing other required fields

        let errors = block.validate();
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.contains("preferred-term")));
        assert!(errors.iter().any(|e| e.contains("definition")));
    }

    #[test]
    fn test_get_domain() {
        let mut block = OntologyBlock::new("test.md".to_string());

        // From source-domain
        block.source_domain = Some("ai".to_string());
        assert_eq!(block.get_domain(), Some("ai".to_string()));

        // From term-id
        block.source_domain = None;
        block.term_id = Some("BC-0001".to_string());
        assert_eq!(block.get_domain(), Some("bc".to_string()));

        // From v2 full-word domain in owl_class
        block.term_id = None;
        block.owl_class = Some("artificial-intelligence:TestConcept".to_string());
        assert_eq!(
            block.get_domain(),
            Some("artificial-intelligence".to_string())
        );
    }

    #[test]
    fn test_get_full_iri() {
        let mut block = OntologyBlock::new("test.md".to_string());
        block.owl_class = Some("ai:LargeLanguageModel".to_string());

        let iri = block.get_full_iri();
        assert_eq!(
            iri,
            Some("http://narrativegoldmine.com/ai#LargeLanguageModel".to_string())
        );
    }

    #[test]
    fn test_legacy_parse_backward_compatibility() {
        let parser = OntologyParser::new();
        let content = r#"
- ### OntologyBlock
  - owl:class:: ai:Test
  - preferred-term:: Test Concept
  - definition:: A test
  - owl:physicality:: VirtualEntity
  - owl:role:: Concept
  - term-id:: AI-0001
  - source-domain:: ai
  - status:: complete
  - public-access:: true
  - last-updated:: 2025-11-21

  - #### Relationships
    - is-subclass-of:: [[Parent]]
"#;

        let result = parser.parse(content, "test.md");
        assert!(result.is_ok());

        let data = result.unwrap();
        assert_eq!(data.classes.len(), 1);
        assert_eq!(data.classes[0].iri, "ai:Test");
        assert_eq!(data.axioms.len(), 1);
        assert_eq!(data.class_hierarchy.len(), 1);
    }
}
