// tests/parser_sovereign.rs
//
// Sprint-A parser tests — ADR-050 / ADR-051.
//
// Exercises the two-pass parser, visibility classification, stub
// synthesis, and WikilinkRef run-id tagging. These tests are
// implementation-agnostic of the sibling agent's KGNode rename; they
// assert against `KGNodeDraft` as emitted by the parser today.

use webxr::services::parsers::knowledge_graph_parser::{
    canonical_iri, deterministic_id_from_iri, pod_url_for, resolve_wikilink_to_iri,
    slugify_title, FileBundle, KnowledgeGraphParser,
};
use webxr::services::parsers::visibility::{classify_visibility, Visibility};

const OWNER: &str = "npub1testowner00000000000000000000000000000000000000000000";

fn parser() -> KnowledgeGraphParser {
    KnowledgeGraphParser::new_with_owner(OWNER)
}

// ---------------------------------------------------------------------------
// visibility classifier
// ---------------------------------------------------------------------------

#[test]
fn public_flag_at_top_of_page_is_public() {
    let src = "public:: true\ntitle:: Alpha\n\n- body block\n";
    assert_eq!(classify_visibility(src), Visibility::Public);
}

#[test]
fn page_without_flag_is_private() {
    let src = "title:: Alpha\ntags:: draft\n\n- body\n";
    assert_eq!(classify_visibility(src), Visibility::Private);
}

#[test]
fn public_in_block_body_is_private() {
    // `public:: true` appears on a bullet line after page-properties end
    // — it's a block property, not a page-level publishing flag.
    let src = "title:: Alpha\n- first block\n  public:: true\n";
    assert_eq!(classify_visibility(src), Visibility::Private);
}

#[test]
fn public_access_owl_is_never_public_visibility() {
    // `public-access:: true` is an OWL property on an ontology class,
    // never a page-level publishing flag. Reference commit b501942b1.
    let src = "public-access:: true\ntitle:: Alpha\n";
    assert_eq!(classify_visibility(src), Visibility::Private);
}

#[test]
fn heading_terminates_page_properties() {
    // A `#` heading ends the page-properties block; `public:: true`
    // after that is body content.
    let src = "title:: Alpha\n# Heading\npublic:: true\n";
    assert_eq!(classify_visibility(src), Visibility::Private);
}

// ---------------------------------------------------------------------------
// canonical IRI & stable ids
// ---------------------------------------------------------------------------

#[test]
fn canonical_iri_is_deterministic() {
    let a = canonical_iri(OWNER, "pages/foo.md");
    let b = canonical_iri(OWNER, "pages/foo.md");
    assert_eq!(a, b);
    assert!(a.starts_with("visionclaw:owner:"));
    assert!(a.contains("/kg/"));
}

#[test]
fn canonical_iri_changes_on_rename() {
    // ADR-050 §"Canonical IRI" — rename-proof identity is a non-goal.
    let a = canonical_iri(OWNER, "pages/foo.md");
    let b = canonical_iri(OWNER, "folder/pages/foo.md");
    assert_ne!(a, b);
}

#[test]
fn deterministic_id_is_nonzero_and_no_bit29() {
    let iri = canonical_iri(OWNER, "pages/demo.md");
    let id = deterministic_id_from_iri(&iri);
    assert_ne!(id, 0, "id must never be the 0 sentinel");
    assert_eq!(id & 0x2000_0000, 0, "bit 29 is reserved for on-wire opacity");
}

#[test]
fn deterministic_id_is_stable_across_runs() {
    let iri = canonical_iri(OWNER, "pages/demo.md");
    let a = deterministic_id_from_iri(&iri);
    let b = deterministic_id_from_iri(&iri);
    assert_eq!(a, b);
}

// ---------------------------------------------------------------------------
// wikilink resolution & slug helpers
// ---------------------------------------------------------------------------

#[test]
fn slugify_replaces_spaces_and_lowercases() {
    assert_eq!(slugify_title("My Page"), "my_page");
    assert_eq!(slugify_title("Foo-Bar_Baz"), "foo-bar_baz");
    assert_eq!(slugify_title("With Punct?!"), "with_punct");
}

#[test]
fn resolve_wikilink_hits_title_index_directly() {
    let mut index = std::collections::HashMap::new();
    let iri = canonical_iri(OWNER, "pages/foo.md");
    index.insert("Foo".to_string(), iri.clone());

    let resolved = resolve_wikilink_to_iri("Foo", &index, OWNER);
    assert_eq!(resolved, iri);
}

#[test]
fn resolve_wikilink_falls_back_to_slug_based_iri() {
    let empty = std::collections::HashMap::new();
    let a = resolve_wikilink_to_iri("Unknown Page", &empty, OWNER);
    let b = resolve_wikilink_to_iri("Unknown Page", &empty, OWNER);
    assert_eq!(a, b, "fallback IRI must be deterministic across calls");
    assert!(a.starts_with("visionclaw:owner:"));
}

// ---------------------------------------------------------------------------
// pod_url composition
// ---------------------------------------------------------------------------

#[test]
fn pod_url_private_uses_private_container() {
    let url = pod_url_for(OWNER, "pages/secret.md", Visibility::Private);
    assert!(url.contains("/private/kg/"), "got: {}", url);
    assert!(!url.contains("/public/kg/"), "got: {}", url);
}

#[test]
fn pod_url_public_uses_public_container() {
    let url = pod_url_for(OWNER, "pages/open.md", Visibility::Public);
    assert!(url.contains("/public/kg/"), "got: {}", url);
}

// ---------------------------------------------------------------------------
// two-pass parse_bundle
// ---------------------------------------------------------------------------

#[test]
fn parse_bundle_public_page_emits_public_node() {
    let file = FileBundle::new(
        "Alpha.md",
        "pages/Alpha.md",
        "public:: true\ntitle:: Alpha\n\n- body\n",
    );
    let out = parser().parse_bundle(&[file]).expect("parse ok");

    assert_eq!(out.nodes.len(), 1);
    let n = &out.nodes[0];
    assert_eq!(n.visibility, Visibility::Public);
    assert_eq!(n.is_stub, false);
    assert!(n.pod_url.as_ref().unwrap().contains("/public/kg/"));
    assert!(!n.node.label.is_empty());
}

#[test]
fn parse_bundle_private_page_is_private_but_keeps_label() {
    // A page the owner can still see — has its real label, but the
    // schema fields say private. Opacification is the projection
    // layer's job (per ADR-050); the parser just classifies.
    let file = FileBundle::new(
        "Beta.md",
        "pages/Beta.md",
        "title:: Beta\n\n- body\n",
    );
    let out = parser().parse_bundle(&[file]).expect("parse ok");
    assert_eq!(out.nodes.len(), 1);
    let n = &out.nodes[0];
    assert_eq!(n.visibility, Visibility::Private);
    assert_eq!(n.is_stub, false);
}

#[test]
fn wikilink_to_unknown_page_creates_private_stub() {
    let file = FileBundle::new(
        "Alpha.md",
        "pages/Alpha.md",
        "public:: true\ntitle:: Alpha\n\n- body mentions [[Ghost Page]]\n",
    );
    let out = parser().parse_bundle(&[file]).expect("parse ok");

    assert_eq!(out.nodes.len(), 2, "source page + stub");
    let stub = out
        .nodes
        .iter()
        .find(|n| n.is_stub)
        .expect("stub not found");

    // Stubs must never leak content.
    assert!(stub.node.label.is_empty(), "stub must have no label");
    assert_eq!(stub.visibility, Visibility::Private);
    // A stub has no Pod content until the owner authors it.
    assert!(stub.pod_url.is_none());
    // There must be exactly one edge, from source to stub.
    assert_eq!(out.edges.len(), 1);
}

#[test]
fn wikilink_to_known_public_page_does_not_duplicate() {
    let a = FileBundle::new(
        "Alpha.md",
        "pages/Alpha.md",
        "public:: true\ntitle:: Alpha\n\n- see [[Beta]]\n",
    );
    let b = FileBundle::new(
        "Beta.md",
        "pages/Beta.md",
        "public:: true\ntitle:: Beta\n",
    );
    let out = parser().parse_bundle(&[a, b]).expect("parse ok");

    assert_eq!(out.nodes.len(), 2, "two pages, no duplicate");
    assert_eq!(
        out.nodes.iter().filter(|n| n.is_stub).count(),
        0,
        "no stubs when both targets are indexed"
    );
    assert_eq!(out.edges.len(), 1);
    assert!(out.nodes.iter().all(|n| n.visibility == Visibility::Public));
}

#[test]
fn wikilink_edge_carries_last_seen_run_id() {
    let file = FileBundle::new(
        "Alpha.md",
        "pages/Alpha.md",
        "public:: true\ntitle:: Alpha\n\n- [[Ghost]]\n",
    );
    let out = parser().parse_bundle(&[file]).expect("parse ok");

    assert_eq!(out.edges.len(), 1);
    let meta = out.edges[0].metadata.as_ref().expect("edge metadata");
    let run_id = meta.get("last_seen_run_id").expect("run_id missing");
    assert_eq!(run_id, &out.run_id);
    assert_eq!(
        meta.get("neo4j_relationship").map(String::as_str),
        Some("WikilinkRef")
    );
}

#[test]
fn two_pages_cross_link_produces_two_edges_no_stubs() {
    let a = FileBundle::new(
        "Alpha.md",
        "pages/Alpha.md",
        "public:: true\ntitle:: Alpha\n\n- refs [[Beta]]\n",
    );
    let b = FileBundle::new(
        "Beta.md",
        "pages/Beta.md",
        "public:: true\ntitle:: Beta\n\n- refs [[Alpha]]\n",
    );
    let out = parser().parse_bundle(&[a, b]).expect("parse ok");

    assert_eq!(out.nodes.len(), 2);
    assert_eq!(out.nodes.iter().filter(|n| n.is_stub).count(), 0);
    assert_eq!(out.edges.len(), 2, "one edge in each direction");
}

#[test]
fn stub_iri_is_deterministic_across_runs() {
    let make = || {
        FileBundle::new(
            "Alpha.md",
            "pages/Alpha.md",
            "public:: true\ntitle:: Alpha\n\n- [[Ghost Page]]\n",
        )
    };

    let out1 = parser().parse_bundle(&[make()]).expect("parse 1");
    let out2 = parser().parse_bundle(&[make()]).expect("parse 2");

    let stub1 = out1.nodes.iter().find(|n| n.is_stub).unwrap();
    let stub2 = out2.nodes.iter().find(|n| n.is_stub).unwrap();
    assert_eq!(stub1.canonical_iri, stub2.canonical_iri);
    assert_eq!(stub1.node.id, stub2.node.id);
    // run_id MUST differ so the orphan-retraction job can distinguish
    // the two runs.
    assert_ne!(out1.run_id, out2.run_id);
}

#[test]
fn parse_bundle_requires_owner_pubkey() {
    let parser = KnowledgeGraphParser::new();
    let file = FileBundle::new("Alpha.md", "pages/Alpha.md", "public:: true\n");
    let err = parser
        .parse_bundle(&[file])
        .expect_err("should fail without owner");
    assert!(err.contains("owner_pubkey"), "got: {}", err);
}
