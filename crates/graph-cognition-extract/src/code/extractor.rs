use graph_cognition_core::{TypedEdge, TypedNode};

/// Languages supported by the code analysis pipeline (ADR-065).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Rust,
    TypeScript,
    Python,
    Go,
    Java,
}

impl Language {
    /// Detect language from a file path's extension.
    pub fn from_extension(path: &str) -> Option<Self> {
        let ext = path.rsplit('.').next()?;
        match ext {
            "rs" => Some(Self::Rust),
            "ts" | "tsx" => Some(Self::TypeScript),
            "py" => Some(Self::Python),
            "go" => Some(Self::Go),
            "java" => Some(Self::Java),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::TypeScript => "typescript",
            Self::Python => "python",
            Self::Go => "go",
            Self::Java => "java",
        }
    }
}

/// Result of extracting graph structure from a source file.
#[derive(Debug, Clone)]
pub struct ExtractionResult {
    pub nodes: Vec<TypedNode>,
    pub edges: Vec<TypedEdge>,
    pub errors: Vec<String>,
}

impl ExtractionResult {
    pub fn empty() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            errors: Vec::new(),
        }
    }

    /// Merge another result into this one.
    pub fn merge(&mut self, other: ExtractionResult) {
        self.nodes.extend(other.nodes);
        self.edges.extend(other.edges);
        self.errors.extend(other.errors);
    }
}

/// Trait for language-specific code extractors.
///
/// Each implementation parses source code for a single language and produces
/// [`TypedNode`] and [`TypedEdge`] values suitable for the VisionClaw graph.
///
/// The current implementations use regex-based extraction. A future iteration
/// (ADR-065 Phase 2) will introduce tree-sitter grammars for full AST fidelity.
pub trait CodeExtractor: Send + Sync {
    /// The language this extractor handles.
    fn language(&self) -> Language;

    /// Extract graph nodes and edges from source code.
    ///
    /// `source` is the full file contents. `file_path` is used for URN minting
    /// and should be a project-relative path (e.g., `src/actors/graph.rs`).
    fn extract(&self, source: &str, file_path: &str) -> ExtractionResult;
}

/// Mint a URN for a code item following the VisionClaw concept scheme.
///
/// Format: `urn:visionclaw:concept:code:{file_path}:{item_name}`
pub fn mint_code_urn(file_path: &str, item_name: &str) -> String {
    // Normalize path separators and strip leading slashes
    let normalized = file_path
        .replace('\\', "/")
        .trim_start_matches('/')
        .to_string();
    format!("urn:visionclaw:concept:code:{}:{}", normalized, item_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_from_extension() {
        assert_eq!(Language::from_extension("foo.rs"), Some(Language::Rust));
        assert_eq!(Language::from_extension("bar.ts"), Some(Language::TypeScript));
        assert_eq!(Language::from_extension("baz.tsx"), Some(Language::TypeScript));
        assert_eq!(Language::from_extension("qux.py"), Some(Language::Python));
        assert_eq!(Language::from_extension("main.go"), Some(Language::Go));
        assert_eq!(Language::from_extension("App.java"), Some(Language::Java));
        assert_eq!(Language::from_extension("style.css"), None);
        assert_eq!(Language::from_extension("Makefile"), None);
    }

    #[test]
    fn language_from_path_with_directories() {
        assert_eq!(
            Language::from_extension("src/actors/graph.rs"),
            Some(Language::Rust)
        );
        assert_eq!(
            Language::from_extension("client/src/hooks/useGraph.ts"),
            Some(Language::TypeScript)
        );
    }

    #[test]
    fn mint_urn_basic() {
        let urn = mint_code_urn("src/lib.rs", "parse_file");
        assert_eq!(urn, "urn:visionclaw:concept:code:src/lib.rs:parse_file");
    }

    #[test]
    fn mint_urn_strips_leading_slash() {
        let urn = mint_code_urn("/src/lib.rs", "main");
        assert_eq!(urn, "urn:visionclaw:concept:code:src/lib.rs:main");
    }

    #[test]
    fn mint_urn_normalizes_backslashes() {
        let urn = mint_code_urn("src\\actors\\graph.rs", "handle");
        assert_eq!(urn, "urn:visionclaw:concept:code:src/actors/graph.rs:handle");
    }

    #[test]
    fn extraction_result_merge() {
        let mut a = ExtractionResult::empty();
        a.errors.push("warn-a".into());

        let mut b = ExtractionResult::empty();
        b.errors.push("warn-b".into());

        a.merge(b);
        assert_eq!(a.errors.len(), 2);
    }
}
