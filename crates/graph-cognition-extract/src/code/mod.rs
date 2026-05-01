//! Code analysis pipeline per ADR-065 (PRD-005 Epic B).
//!
//! Extracts typed graph nodes and edges from source code files.
//! Supports multiple languages through the [`CodeExtractor`] trait.
//!
//! # Supported Languages
//!
//! | Language   | Extractor           | Status      |
//! |------------|---------------------|-------------|
//! | Rust       | [`RustExtractor`]   | Implemented |
//! | TypeScript | —                   | Planned     |
//! | Python     | —                   | Planned     |
//! | Go         | —                   | Planned     |
//! | Java       | —                   | Planned     |
//!
//! # URN Scheme
//!
//! Extracted items are minted with URNs following the VisionClaw scheme:
//! ```text
//! urn:visionclaw:concept:code:{file_path}:{item_name}
//! ```

pub mod extractor;
pub mod rust_extractor;

pub use extractor::{CodeExtractor, ExtractionResult, Language};
pub use rust_extractor::RustExtractor;

/// Detect language from file extension and return the appropriate extractor.
///
/// Returns `None` for unsupported languages.
pub fn extractor_for_path(path: &str) -> Option<Box<dyn CodeExtractor>> {
    let lang = Language::from_extension(path)?;
    match lang {
        Language::Rust => Some(Box::new(RustExtractor::new())),
        // Future extractors will be added here as they are implemented.
        _ => None,
    }
}
