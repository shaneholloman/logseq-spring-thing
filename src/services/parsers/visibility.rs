// src/services/parsers/visibility.rs
//! Visibility classification for Logseq-authored knowledge graph pages.
//!
//! The sovereign-private-node model (ADR-050 / ADR-051) promotes every
//! wikilink target to a first-class `:KGNode`. Some of those nodes are
//! public (their owner has explicitly flipped the `public:: true` flag at
//! page-properties level); the rest are private stubs whose content lives
//! in the owner's Pod and whose label/metadata must never reach the
//! anonymous API surface.
//!
//! `public:: true` is a Logseq PAGE property. It MUST appear as a
//! line-anchored `key:: value` line within the page-properties block —
//! the region at the very top of the file before the first bullet (`- `)
//! or heading (`#`). A body-level mention, or the unrelated OWL
//! `public-access:: true` inside an `### OntologyBlock`, does NOT count.
//!
//! Reference: commit `b501942b1` — `fix(ingest): correct public-gating…`
//! which established the line-anchored rule after a regression that
//! conflated `public:: true` with `public-access:: true` and pulled ~2k
//! ontology-stub pages into the KG.
//!
//! `Visibility` is canonically owned by `crate::models::node` (ADR-050);
//! this module re-exports the type and provides the `classify_visibility`
//! function used by the two-pass parser.

pub use crate::models::node::Visibility;

/// Classify a Logseq page's visibility.
///
/// Returns `Visibility::Public` if and only if the page declares
/// `public:: true` as a line-anchored page-property in the block before the
/// first bullet (`- `) or heading (`#`). Any other string — including
/// `public-access:: true`, an indented bullet-level `- public:: true`, or
/// a body mention — yields `Visibility::Private`.
///
/// Empty lines, a leading UTF-8 BOM, and leading whitespace on property
/// lines are tolerated. The scan stops at the first bullet or heading; a
/// `public:: true` that appears after page content begins is ignored.
pub fn classify_visibility(raw: &str) -> Visibility {
    // Strip a single leading UTF-8 BOM if present.
    let raw = raw.trim_start_matches('\u{feff}');

    for line in raw.lines() {
        // Pure whitespace lines are part of the page-properties block.
        if line.trim().is_empty() {
            continue;
        }

        // A `key:: value` property line in Logseq is authored either flush-
        // left (`public:: true`) or — when a page-properties block has been
        // promoted into the first bullet — prefixed with whitespace only.
        // A `-` or `#` marker signals the end of the properties block.
        let leading_trimmed = line.trim_start();
        if leading_trimmed.starts_with('-') || leading_trimmed.starts_with('#') {
            // First bullet or heading: we are past page properties. Any
            // `public:: true` past this point is a bullet-level block
            // property, NOT a page-level publishing flag, per the
            // line-anchoring rule from commit b501942b1.
            return Visibility::Private;
        }

        // Must be a line-anchored property: exactly `public:: true`
        // (trailing whitespace tolerated, nothing else on the line).
        if leading_trimmed.trim_end() == "public:: true" {
            return Visibility::Public;
        }

        // Any other property line (`title:: …`, `tags:: …`,
        // `public-access:: true`, etc.) is fine — keep scanning.
    }

    Visibility::Private
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_with_line_anchored_public_is_public() {
        let src = "public:: true\ntitle:: Foo\n\n- body line\n";
        assert_eq!(classify_visibility(src), Visibility::Public);
    }

    #[test]
    fn page_without_flag_is_private() {
        let src = "title:: Foo\n\n- body line\n";
        assert_eq!(classify_visibility(src), Visibility::Private);
    }

    #[test]
    fn public_in_body_block_is_private() {
        // `public:: true` appears *after* the first bullet — this is a
        // block-level authoring artefact, not a page property.
        let src = "title:: Foo\n- first block\n- public:: true\n";
        assert_eq!(classify_visibility(src), Visibility::Private);
    }

    #[test]
    fn public_access_owl_property_is_never_public() {
        // `public-access:: true` is an OWL property on an ontology class
        // (ADR-048), NOT a Logseq page-level publishing flag.
        let src = "public-access:: true\ntitle:: Foo\n";
        assert_eq!(classify_visibility(src), Visibility::Private);
    }

    #[test]
    fn bom_is_tolerated() {
        let src = "\u{feff}public:: true\n";
        assert_eq!(classify_visibility(src), Visibility::Public);
    }
}
