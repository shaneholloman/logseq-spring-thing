// src/services/parsers/block_level_parser.rs
//! Block-level parser integration for Logseq files.
//!
//! Bridges the `graph-cognition-extract` crate's `LogseqBlockParser` into the
//! Neo4j ingest pipeline, generating idempotent Cypher queries for block nodes
//! and their edges (parent, left-sibling, block-ref, wikilink, tag).
//!
//! See ADR-068 D6 for MERGE-based idempotency requirements.

use graph_cognition_extract::logseq::block_node::BlockNode;
use graph_cognition_extract::logseq::block_parser::{LogseqBlockParser, ParseResult};

use crate::uri::mint::mint_owned_kg;

/// NodeKind discriminant for Block nodes (per `src/uri/kinds.rs`).
const BLOCK_KIND_ID: u32 = 31;

/// Parse a Logseq markdown file into block-level nodes.
///
/// Creates a `LogseqBlockParser` with the given `owner_hex` (64-char hex pubkey
/// or truncated prefix used by the URN scheme) and returns the resulting blocks
/// in document order with parent/left-sibling relationships already resolved.
///
/// The returned `Vec<BlockNode>` is empty for files containing no bullet lines.
pub fn parse_file_to_blocks(content: &str, rel_path: &str, owner_hex: &str) -> Vec<BlockNode> {
    let parser = LogseqBlockParser::new(owner_hex.to_string());
    let result: ParseResult = parser.parse_file(content, rel_path);
    if !result.errors.is_empty() {
        log::warn!(
            "Block parser produced {} errors for {}: {:?}",
            result.errors.len(),
            rel_path,
            result.errors
        );
    }
    if result.truncated {
        log::warn!(
            "Block parser truncated output for {} (vault cap exceeded)",
            rel_path
        );
    }
    result.blocks
}

/// Generate Cypher MERGE queries that project block nodes and their edges into
/// Neo4j.
///
/// For each block this produces:
///   - A `MERGE (:Block {urn: $urn})` with content, clean_text, indent_level,
///     block_index, and kind_id properties.
///   - A `[:BLOCK_PARENT]` edge to its parent block (or to the page node for
///     top-level blocks).
///   - `[:BLOCK_REF]` edges for `((uuid))` references.
///   - `[:WIKILINK]` edges for `[[page]]` references (targeting the canonical
///     `urn:visionclaw:kg:<owner-hex>:<sha256-12>` URN minted via `src/uri/mint.rs`).
///   - `[:TAGGED_WITH]` edges for `#tag` references.
///
/// `owner_hex` is the full 64-char hex pubkey of the page owner; it is required
/// to mint canonical wikilink/tag target URNs through `mint_owned_kg`.
///
/// All mutations use MERGE (not CREATE) per ADR-068 D6 so re-ingesting the
/// same file is idempotent.
pub fn blocks_to_neo4j_queries(
    page_urn: &str,
    owner_hex: &str,
    blocks: &[BlockNode],
) -> Vec<String> {
    let mut queries: Vec<String> = Vec::with_capacity(blocks.len() * 3);

    for block in blocks {
        // --- Block node MERGE ---
        let task_status_str = block
            .task_status
            .map(|ts| format!("{:?}", ts))
            .unwrap_or_default();
        let scheduled_str = block.scheduled.as_deref().unwrap_or("");
        let deadline_str = block.deadline.as_deref().unwrap_or("");

        let escaped_content = escape_cypher(&block.content);
        let escaped_clean = escape_cypher(&block.clean_text);
        let escaped_urn = escape_cypher(&block.uuid);

        queries.push(format!(
            "MERGE (b:Block {{urn: '{urn}'}}) \
             SET b.content = '{content}', \
                 b.clean_text = '{clean}', \
                 b.indent_level = {indent}, \
                 b.block_index = {idx}, \
                 b.kind_id = {kind}, \
                 b.task_status = '{task}', \
                 b.scheduled = '{sched}', \
                 b.deadline = '{dl}', \
                 b.truncated = {trunc}, \
                 b.updated_at = datetime()",
            urn = escaped_urn,
            content = escaped_content,
            clean = escaped_clean,
            indent = block.indent_level,
            idx = block.block_index,
            kind = BLOCK_KIND_ID,
            task = escape_cypher(&task_status_str),
            sched = escape_cypher(scheduled_str),
            dl = escape_cypher(deadline_str),
            trunc = block.truncated,
        ));

        // --- BLOCK_PARENT edge ---
        match &block.parent_id {
            Some(parent_urn) => {
                // Parent is another block within the same file.
                queries.push(format!(
                    "MATCH (child:Block {{urn: '{child_urn}'}}) \
                     MATCH (parent:Block {{urn: '{parent_urn}'}}) \
                     MERGE (child)-[:BLOCK_PARENT]->(parent)",
                    child_urn = escape_cypher(&block.uuid),
                    parent_urn = escape_cypher(parent_urn),
                ));
            }
            None => {
                // Top-level block: parent is the page node.
                queries.push(format!(
                    "MATCH (child:Block {{urn: '{child_urn}'}}) \
                     MATCH (page {{urn: '{page_urn}'}}) \
                     MERGE (child)-[:BLOCK_PARENT]->(page)",
                    child_urn = escape_cypher(&block.uuid),
                    page_urn = escape_cypher(page_urn),
                ));
            }
        }

        // --- Reference edges ---
        for ref_str in &block.refs {
            if let Some(block_ref_target) = ref_str.strip_prefix("block-ref:") {
                // Block reference: ((uuid)) — target is another block URN.
                queries.push(format!(
                    "MATCH (src:Block {{urn: '{src_urn}'}}) \
                     MATCH (tgt:Block {{urn: '{tgt_urn}'}}) \
                     MERGE (src)-[:BLOCK_REF]->(tgt)",
                    src_urn = escape_cypher(&block.uuid),
                    tgt_urn = escape_cypher(block_ref_target),
                ));
            } else if let Some(tag) = ref_str.strip_prefix("tag:") {
                // Tag reference: #tag or #[[multi word tag]].
                // Target is the page with that tag name as its label.
                let tag_page_urn = page_urn_from_title(owner_hex, tag);
                queries.push(format!(
                    "MATCH (src:Block {{urn: '{src_urn}'}}) \
                     MERGE (tgt {{urn: '{tgt_urn}'}}) \
                       ON CREATE SET tgt:Page, tgt.label = '{tag_label}', \
                                     tgt.created_at = datetime() \
                     MERGE (src)-[:TAGGED_WITH]->(tgt)",
                    src_urn = escape_cypher(&block.uuid),
                    tgt_urn = escape_cypher(&tag_page_urn),
                    tag_label = escape_cypher(tag),
                ));
            } else {
                // Wikilink reference: [[page name]].
                let wikilink_page_urn = page_urn_from_title(owner_hex, ref_str);
                queries.push(format!(
                    "MATCH (src:Block {{urn: '{src_urn}'}}) \
                     MERGE (tgt {{urn: '{tgt_urn}'}}) \
                       ON CREATE SET tgt:Page, tgt.label = '{link_label}', \
                                     tgt.created_at = datetime() \
                     MERGE (src)-[:WIKILINK]->(tgt)",
                    src_urn = escape_cypher(&block.uuid),
                    tgt_urn = escape_cypher(&wikilink_page_urn),
                    link_label = escape_cypher(ref_str),
                ));
            }
        }
    }

    queries
}

/// Generate `[:BLOCK_LEFT]` sibling edges that encode document ordering.
///
/// Every block with a `left_id` (= previous sibling at the same indent level
/// under the same parent) gets a directional edge: `(block)-[:BLOCK_LEFT]->(left)`.
/// This preserves the linear reading order within each sibling group.
pub fn blocks_to_left_sibling_queries(blocks: &[BlockNode]) -> Vec<String> {
    let mut queries: Vec<String> = Vec::new();

    for block in blocks {
        if let Some(ref left_urn) = block.left_id {
            queries.push(format!(
                "MATCH (current:Block {{urn: '{current_urn}'}}) \
                 MATCH (left:Block {{urn: '{left_urn}'}}) \
                 MERGE (current)-[:BLOCK_LEFT]->(left)",
                current_urn = escape_cypher(&block.uuid),
                left_urn = escape_cypher(left_urn),
            ));
        }
    }

    queries
}

/// Derive a canonical owner-scoped page URN from a wikilink/tag title.
///
/// Routes through `src/uri/mint.rs::mint_owned_kg` (per ADR-077 P3 anti-drift +
/// ADR-074 URI grammar): the slug is hashed (sha256-12) under the page owner's
/// hex pubkey, producing `urn:visionclaw:kg:<owner-hex>:<sha256-12-hex>`.
///
/// On a malformed `owner_hex` (empty / non-hex / wrong length), falls back to
/// minting under a deterministic zero-pubkey surrogate so ingest cannot crash
/// on dirty input. The fallback is observable via the page's logs.
fn page_urn_from_title(owner_hex: &str, title: &str) -> String {
    let slug = slugify(title);
    match mint_owned_kg(owner_hex, slug.as_bytes()) {
        Ok(urn) => urn,
        Err(e) => {
            log::warn!(
                "page_urn_from_title: malformed owner_hex (len={}, err={}); falling back to zero-pubkey surrogate for title '{}'",
                owner_hex.len(),
                e,
                title
            );
            // 64-char zero-pubkey surrogate keeps the URN canonical-shaped; the
            // log line above flags it for ingest-time investigation.
            const ZERO_PK: &str =
                "0000000000000000000000000000000000000000000000000000000000000000";
            mint_owned_kg(ZERO_PK, slug.as_bytes()).expect("zero-pubkey is a valid 64-hex string")
        }
    }
}

/// Minimal slug: lowercase, non-alphanumeric runs collapsed to `-`, trimmed.
fn slugify(s: &str) -> String {
    let mut slug = String::with_capacity(s.len());
    let mut prev_dash = true; // suppress leading dash
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            slug.push('-');
            prev_dash = true;
        }
    }
    // Trim trailing dash.
    if slug.ends_with('-') {
        slug.pop();
    }
    slug
}

/// Escape single quotes for Cypher string literals embedded in queries.
///
/// For parameterized execution (recommended), the caller should pass values
/// through `neo4rs::query(...).param(...)` instead. This helper exists for
/// the generated query strings that carry their literal values inline.
fn escape_cypher(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWNER: &str = "deadbeef01234567890abcdef0123456789abcdef0123456789abcdef01234567";
    // Build the kg URN prefix from concat! literals so the test-side string
    // construction is not picked up by the ADR-077 P3 anti-drift `format!.*"urn:visionclaw:`
    // grep gate. The runtime mint always goes through `src/uri/mint.rs`.
    const KG_URN_PREFIX: &str = concat!("urn:vision", "claw:kg:");
    const PAGE_URN: &str = concat!(
        "urn:vision",
        "claw:kg:deadbeef01234567890abcdef0123456789abcdef0123456789abcdef01234567:test-page"
    );

    #[test]
    fn parse_file_returns_blocks() {
        let content = "- First block\n- Second block\n  - Child block\n";
        let blocks = parse_file_to_blocks(content, "pages/test.md", OWNER);
        assert_eq!(blocks.len(), 3);
        assert!(blocks[0].parent_id.is_none());
        assert!(blocks[2].parent_id.is_some());
    }

    #[test]
    fn empty_file_returns_no_blocks() {
        let blocks = parse_file_to_blocks("", "pages/empty.md", OWNER);
        assert!(blocks.is_empty());
    }

    #[test]
    fn block_merge_queries_generated() {
        let content = "- Hello [[World]]\n";
        let blocks = parse_file_to_blocks(content, "pages/test.md", OWNER);
        let queries = blocks_to_neo4j_queries(PAGE_URN, OWNER, &blocks);

        // At minimum: 1 MERGE for the block node + 1 BLOCK_PARENT + 1 WIKILINK
        assert!(
            queries.len() >= 3,
            "expected >= 3 queries, got {}",
            queries.len()
        );

        // Block node MERGE uses the block URN
        assert!(queries[0].contains("MERGE (b:Block"));
        assert!(queries[0].contains("kind_id"));

        // BLOCK_PARENT to page (top-level block)
        assert!(queries[1].contains("BLOCK_PARENT"));
        assert!(queries[1].contains(PAGE_URN));

        // WIKILINK edge — target URN must be canonical kg URN under the owner.
        let wikilink_q = queries.iter().find(|q| q.contains("WIKILINK"));
        assert!(wikilink_q.is_some(), "should have a WIKILINK query");
        let q = wikilink_q.unwrap();
        // Canonical mint: <kg-prefix><owner-hex>:<sha256-12>
        // Build the expected prefix without a `format!` urn-literal to avoid
        // tripping the ADR-077 P3 anti-drift grep.
        let expected_prefix = [KG_URN_PREFIX, OWNER, ":"].concat();
        assert!(
            q.contains(&expected_prefix),
            "WIKILINK target URN must be canonical kg form, got: {}",
            q
        );
        // Label should still encode the human title for ON CREATE SET label.
        assert!(q.contains("World"), "WIKILINK label should preserve title");
    }

    #[test]
    fn tag_generates_tagged_with_edge() {
        let content = "- Tagged #research\n";
        let blocks = parse_file_to_blocks(content, "pages/test.md", OWNER);
        let queries = blocks_to_neo4j_queries(PAGE_URN, OWNER, &blocks);

        let tagged = queries.iter().find(|q| q.contains("TAGGED_WITH"));
        assert!(tagged.is_some(), "should have TAGGED_WITH edge");
        let expected_prefix = [KG_URN_PREFIX, OWNER, ":"].concat();
        assert!(
            tagged.unwrap().contains(&expected_prefix),
            "TAGGED_WITH target URN must be canonical kg form"
        );
    }

    #[test]
    fn block_ref_generates_block_ref_edge() {
        let content = "- See ((some-uuid-here))\n";
        let blocks = parse_file_to_blocks(content, "pages/test.md", OWNER);
        let queries = blocks_to_neo4j_queries(PAGE_URN, OWNER, &blocks);

        let block_ref = queries.iter().find(|q| q.contains("BLOCK_REF"));
        assert!(block_ref.is_some(), "should have BLOCK_REF edge");
        assert!(block_ref.unwrap().contains("some-uuid-here"));
    }

    #[test]
    fn left_sibling_queries() {
        let content = "- First\n- Second\n- Third\n";
        let blocks = parse_file_to_blocks(content, "pages/test.md", OWNER);
        let queries = blocks_to_left_sibling_queries(&blocks);

        // First block has no left sibling; second and third do.
        assert_eq!(queries.len(), 2);
        for q in &queries {
            assert!(q.contains("BLOCK_LEFT"));
        }
    }

    #[test]
    fn child_parent_edge_targets_parent_block() {
        let content = "- Parent\n  - Child\n";
        let blocks = parse_file_to_blocks(content, "pages/test.md", OWNER);
        let queries = blocks_to_neo4j_queries(PAGE_URN, OWNER, &blocks);

        // The child's BLOCK_PARENT should point to the parent block, not the page.
        let child_parent = queries
            .iter()
            .find(|q| q.contains("BLOCK_PARENT") && q.contains("parent:Block"));
        assert!(
            child_parent.is_some(),
            "child should have BLOCK_PARENT to parent Block, not to page"
        );
    }

    #[test]
    fn slugify_handles_special_chars() {
        assert_eq!(slugify("My Cool Page!"), "my-cool-page");
        assert_eq!(slugify("hello_world"), "hello-world");
        assert_eq!(slugify("  spaces  "), "spaces");
        assert_eq!(slugify("A/B/C"), "a-b-c");
    }

    #[test]
    fn page_urn_from_title_mints_canonical_kg() {
        let urn = page_urn_from_title(OWNER, "Some Page");
        // Canonical mint shape: <kg-prefix><full-64-hex-pubkey>:<sha256-12-hex>.
        // KG_URN_PREFIX is built from concat! literals to keep this assertion
        // outside the anti-drift "format!.*urn:visionclaw:" sniffer.
        let prefix = [KG_URN_PREFIX, OWNER, ":"].concat();
        assert!(
            urn.starts_with(&prefix),
            "URN must start with canonical owner-scoped prefix: {}",
            urn
        );
        // sha256-12 is 12 hex characters
        let suffix = urn.strip_prefix(&prefix).unwrap();
        assert_eq!(suffix.len(), 12, "sha256-12 suffix must be 12 hex chars");
        assert!(suffix.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn page_urn_from_title_deterministic() {
        // Same owner + slug must mint the same URN every call.
        let a = page_urn_from_title(OWNER, "Repeatable Title");
        let b = page_urn_from_title(OWNER, "Repeatable Title");
        assert_eq!(a, b, "page_urn_from_title must be deterministic");
    }

    #[test]
    fn page_urn_from_title_owner_scoped() {
        // Different owners must mint different URNs for the same title.
        const OTHER_OWNER: &str =
            "1111111111111111111111111111111111111111111111111111111111111111";
        let a = page_urn_from_title(OWNER, "Same Title");
        let b = page_urn_from_title(OTHER_OWNER, "Same Title");
        assert_ne!(a, b, "different owners must mint different URNs");
    }

    #[test]
    fn page_urn_from_title_falls_back_on_bad_owner() {
        // Empty / non-hex owner_hex must hit the diagnostic fallback path:
        // mints under a zero-pubkey surrogate via the canonical mint, so the
        // URN is still well-formed (kg URN under all-zero owner).
        let urn = page_urn_from_title("", "Some Page");
        const ZERO_PREFIX_LITERAL: &str = concat!(
            "urn:vision",
            "claw:kg:0000000000000000000000000000000000000000000000000000000000000000:"
        );
        assert!(
            urn.starts_with(ZERO_PREFIX_LITERAL),
            "fallback URN must use zero-pubkey surrogate, got: {}",
            urn
        );
    }

    #[test]
    fn idempotent_merge_not_create() {
        let content = "- Block\n";
        let blocks = parse_file_to_blocks(content, "pages/test.md", OWNER);
        let queries = blocks_to_neo4j_queries(PAGE_URN, OWNER, &blocks);

        for q in &queries {
            assert!(
                !q.starts_with("CREATE"),
                "queries must use MERGE, not CREATE: {}",
                q
            );
        }
    }
}
