// tests/uri_grammar.rs
//! PRD-QE-001 §4.1 P2 row — URI grammar contract tests.
//!
//! Covers the full P2 surface of `crate::uri::*` without requiring a live
//! Neo4j or HTTP server: mint determinism, parse round-trip, owner-scope
//! enforcement, pubkey normalisation, content-hash shape, CURIE↔URN
//! translation, and the negative cases that the resolver's 400 path
//! depends on. Pure-computation; runs unconditionally.

use webxr::uri::{
    content_hash_12, from_curie, is_canonical, mint_bead, mint_concept, mint_did_nostr,
    mint_execution, mint_group_members, mint_owned_kg, normalise_pubkey, parse, to_curie,
    Kind, ParsedUri, UriError,
};

// secp256k1 generator x-coord — a real 32-byte hex pubkey accepted by
// `nostr_sdk::PublicKey::from_hex`. Same vector used elsewhere in the suite
// (tests/schema_sovereign_fields.rs:31).
const TEST_PUBKEY: &str = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
const TEST_PUBKEY_2: &str = "0000000000000000000000000000000000000000000000000000000000000001";

// ---------------------------------------------------------------------------
// Mint determinism — same input → same URN, every kind
// ---------------------------------------------------------------------------

#[test]
fn mint_concept_is_deterministic() {
    let a = mint_concept("bc", "smart-contract");
    let b = mint_concept("bc", "smart-contract");
    assert_eq!(a, b);
    assert_eq!(a, "urn:visionclaw:concept:bc:smart-contract");
}

#[test]
fn mint_concept_differs_on_inputs() {
    assert_ne!(mint_concept("bc", "a"), mint_concept("bc", "b"));
    assert_ne!(mint_concept("bc", "a"), mint_concept("ai", "a"));
}

#[test]
fn mint_group_members_is_deterministic_and_shaped() {
    let a = mint_group_members("astro");
    let b = mint_group_members("astro");
    assert_eq!(a, b);
    assert_eq!(a, "urn:visionclaw:group:astro#members");
}

#[test]
fn mint_owned_kg_same_payload_same_urn() {
    let a = mint_owned_kg(TEST_PUBKEY, b"pages/Note.md").unwrap();
    let b = mint_owned_kg(TEST_PUBKEY, b"pages/Note.md").unwrap();
    assert_eq!(a, b);
    assert!(a.starts_with("urn:visionclaw:kg:npub1"));
}

#[test]
fn mint_owned_kg_different_payload_different_urn() {
    let a = mint_owned_kg(TEST_PUBKEY, b"pages/A.md").unwrap();
    let b = mint_owned_kg(TEST_PUBKEY, b"pages/B.md").unwrap();
    assert_ne!(a, b);
}

#[test]
fn mint_owned_kg_different_owner_different_urn() {
    let a = mint_owned_kg(TEST_PUBKEY, b"pages/Note.md").unwrap();
    let b = mint_owned_kg(TEST_PUBKEY_2, b"pages/Note.md").unwrap();
    assert_ne!(a, b);
}

#[test]
fn mint_did_nostr_round_trips_hex() {
    let did = mint_did_nostr(TEST_PUBKEY).unwrap();
    assert_eq!(did, format!("did:nostr:{}", TEST_PUBKEY));
}

#[test]
fn mint_did_nostr_normalises_uppercase_hex() {
    let upper = TEST_PUBKEY.to_uppercase();
    let did = mint_did_nostr(&upper).unwrap();
    assert_eq!(did, format!("did:nostr:{}", TEST_PUBKEY));
}

#[test]
fn mint_did_nostr_accepts_npub_form() {
    use nostr_sdk::{PublicKey, ToBech32};
    let pk = PublicKey::from_hex(TEST_PUBKEY).unwrap();
    let npub = pk.to_bech32().unwrap();
    let did = mint_did_nostr(&npub).unwrap();
    assert_eq!(did, format!("did:nostr:{}", TEST_PUBKEY));
}

#[test]
fn mint_did_nostr_accepts_already_did_form() {
    let did_in = format!("did:nostr:{}", TEST_PUBKEY);
    let did_out = mint_did_nostr(&did_in).unwrap();
    assert_eq!(did_out, did_in);
}

#[test]
fn mint_bead_same_payload_same_urn() {
    let payload = serde_json::json!({"kind": 30001, "content": "hello"});
    let a = mint_bead(TEST_PUBKEY, &payload).unwrap();
    let b = mint_bead(TEST_PUBKEY, &payload).unwrap();
    assert_eq!(a, b);
    assert!(a.starts_with("urn:visionclaw:bead:npub1"));
}

#[test]
fn mint_execution_includes_all_components_in_hash() {
    let a = mint_execution("spawn", "slot1", TEST_PUBKEY, 1234567890).unwrap();
    let b = mint_execution("spawn", "slot1", TEST_PUBKEY, 1234567890).unwrap();
    assert_eq!(a, b, "same components → same URN");

    // Each component changes the hash:
    let diff_action = mint_execution("kill", "slot1", TEST_PUBKEY, 1234567890).unwrap();
    let diff_slot = mint_execution("spawn", "slot2", TEST_PUBKEY, 1234567890).unwrap();
    let diff_pubkey = mint_execution("spawn", "slot1", TEST_PUBKEY_2, 1234567890).unwrap();
    let diff_ts = mint_execution("spawn", "slot1", TEST_PUBKEY, 1234567891).unwrap();
    assert_ne!(a, diff_action);
    assert_ne!(a, diff_slot);
    assert_ne!(a, diff_pubkey);
    assert_ne!(a, diff_ts);
}

// ---------------------------------------------------------------------------
// Owner-scope: mint_owned_kg / mint_bead / mint_did / mint_execution
// reject empty pubkey at mint time (R2 invariant).
// ---------------------------------------------------------------------------

#[test]
fn mint_owned_kg_rejects_empty_pubkey() {
    let err = mint_owned_kg("", b"pages/x").unwrap_err();
    assert_eq!(err, UriError::EmptyPubkey);
}

#[test]
fn mint_bead_rejects_empty_pubkey() {
    let err = mint_bead("", &serde_json::json!({})).unwrap_err();
    assert_eq!(err, UriError::EmptyPubkey);
}

#[test]
fn mint_did_nostr_rejects_empty() {
    let err = mint_did_nostr("").unwrap_err();
    assert_eq!(err, UriError::EmptyPubkey);
}

#[test]
fn mint_execution_rejects_empty_pubkey() {
    let err = mint_execution("spawn", "slot", "", 0).unwrap_err();
    assert_eq!(err, UriError::EmptyPubkey);
}

// ---------------------------------------------------------------------------
// Pubkey normalisation: every accepted form → same 64-char lowercase hex.
// ---------------------------------------------------------------------------

#[test]
fn normalise_pubkey_passes_lowercase_hex_through() {
    let n = normalise_pubkey(TEST_PUBKEY).unwrap();
    assert_eq!(n, TEST_PUBKEY);
}

#[test]
fn normalise_pubkey_lowercases_uppercase_hex() {
    let n = normalise_pubkey(&TEST_PUBKEY.to_uppercase()).unwrap();
    assert_eq!(n, TEST_PUBKEY);
}

#[test]
fn normalise_pubkey_accepts_did_nostr_prefix() {
    let did = format!("did:nostr:{}", TEST_PUBKEY);
    let n = normalise_pubkey(&did).unwrap();
    assert_eq!(n, TEST_PUBKEY);
}

#[test]
fn normalise_pubkey_decodes_npub() {
    use nostr_sdk::{PublicKey, ToBech32};
    let pk = PublicKey::from_hex(TEST_PUBKEY).unwrap();
    let npub = pk.to_bech32().unwrap();
    let n = normalise_pubkey(&npub).unwrap();
    assert_eq!(n, TEST_PUBKEY);
}

#[test]
fn normalise_pubkey_rejects_non_hex() {
    assert!(matches!(
        normalise_pubkey("not-a-pubkey"),
        Err(UriError::InvalidPubkeyHex(_))
    ));
}

#[test]
fn normalise_pubkey_rejects_wrong_length() {
    assert!(matches!(
        normalise_pubkey("abcd"),
        Err(UriError::InvalidPubkeyHex(_))
    ));
}

// ---------------------------------------------------------------------------
// Content hash format: sha256-12-<12 lowercase hex>
// ---------------------------------------------------------------------------

#[test]
fn content_hash_12_has_correct_shape() {
    let h = content_hash_12(b"abc");
    assert!(h.starts_with("sha256-12-"));
    let hex = h.trim_start_matches("sha256-12-");
    assert_eq!(hex.len(), 12);
    assert!(hex.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
}

#[test]
fn content_hash_12_is_deterministic() {
    assert_eq!(content_hash_12(b"abc"), content_hash_12(b"abc"));
}

#[test]
fn content_hash_12_matches_known_sha256_vector() {
    // SHA-256("abc") = ba7816bf8f01cfea... — first 6 bytes = 12 hex chars.
    let h = content_hash_12(b"abc");
    assert_eq!(h, "sha256-12-ba7816bf8f01");
}

#[test]
fn content_hash_12_varies_with_input() {
    assert_ne!(content_hash_12(b"a"), content_hash_12(b"b"));
}

// ---------------------------------------------------------------------------
// Parse round-trip: parse(mint(x)) yields a ParsedUri whose to_curie or URN
// equivalent recovers the original.
// ---------------------------------------------------------------------------

#[test]
fn parse_concept_urn_roundtrip() {
    let urn = mint_concept("bc", "smart-contract");
    match parse(&urn).unwrap() {
        ParsedUri::Concept { domain, slug } => {
            assert_eq!(domain, "bc");
            assert_eq!(slug, "smart-contract");
        }
        other => panic!("expected Concept, got {:?}", other),
    }
}

#[test]
fn parse_group_urn_roundtrip() {
    let urn = mint_group_members("astro");
    match parse(&urn).unwrap() {
        ParsedUri::Group { team } => assert_eq!(team, "astro"),
        other => panic!("expected Group, got {:?}", other),
    }
}

#[test]
fn parse_owned_kg_urn_roundtrip() {
    let urn = mint_owned_kg(TEST_PUBKEY, b"x").unwrap();
    match parse(&urn).unwrap() {
        ParsedUri::OwnedKg { pubkey_hex, npub, hash12 } => {
            assert_eq!(pubkey_hex, TEST_PUBKEY);
            assert!(npub.starts_with("npub1"));
            assert!(hash12.starts_with("sha256-12-"));
        }
        other => panic!("expected OwnedKg, got {:?}", other),
    }
}

#[test]
fn parse_bead_urn_roundtrip() {
    let urn = mint_bead(TEST_PUBKEY, &serde_json::json!({"k": 1})).unwrap();
    match parse(&urn).unwrap() {
        ParsedUri::Bead { pubkey_hex, .. } => assert_eq!(pubkey_hex, TEST_PUBKEY),
        other => panic!("expected Bead, got {:?}", other),
    }
}

#[test]
fn parse_execution_urn_roundtrip() {
    let urn = mint_execution("spawn", "slot1", TEST_PUBKEY, 1).unwrap();
    match parse(&urn).unwrap() {
        ParsedUri::AgentExecution { hash12 } => assert!(hash12.starts_with("sha256-12-")),
        other => panic!("expected AgentExecution, got {:?}", other),
    }
}

#[test]
fn parse_did_nostr_roundtrip() {
    let did = mint_did_nostr(TEST_PUBKEY).unwrap();
    match parse(&did).unwrap() {
        ParsedUri::Did { pubkey_hex } => assert_eq!(pubkey_hex, TEST_PUBKEY),
        other => panic!("expected Did, got {:?}", other),
    }
}

#[test]
fn parsed_uri_kind_matches_variant() {
    assert_eq!(parse(&mint_concept("bc", "x")).unwrap().kind(), Kind::Concept);
    assert_eq!(parse(&mint_group_members("t")).unwrap().kind(), Kind::Group);
    assert_eq!(parse(&mint_did_nostr(TEST_PUBKEY).unwrap()).unwrap().kind(), Kind::Did);
}

#[test]
fn parsed_uri_owner_scoped_classifier() {
    assert!(parse(&mint_owned_kg(TEST_PUBKEY, b"x").unwrap()).unwrap().is_owner_scoped());
    assert!(parse(&mint_bead(TEST_PUBKEY, &serde_json::json!({})).unwrap()).unwrap().is_owner_scoped());
    assert!(!parse(&mint_concept("bc", "x")).unwrap().is_owner_scoped());
    assert!(!parse(&mint_did_nostr(TEST_PUBKEY).unwrap()).unwrap().is_owner_scoped());
}

// ---------------------------------------------------------------------------
// CURIE ↔ URN translation (the database-key vs API-alias bridge)
// ---------------------------------------------------------------------------

#[test]
fn from_curie_translates_concept() {
    let urn = from_curie("vc:bc/smart-contract").unwrap();
    assert_eq!(urn, "urn:visionclaw:concept:bc:smart-contract");
}

#[test]
fn from_curie_passes_existing_urn_through() {
    let urn = "urn:visionclaw:concept:bc:smart-contract";
    assert_eq!(from_curie(urn).unwrap(), urn);
}

#[test]
fn from_curie_passes_did_through() {
    let did = format!("did:nostr:{}", TEST_PUBKEY);
    assert_eq!(from_curie(&did).unwrap(), did);
}

#[test]
fn from_curie_rejects_missing_slug() {
    assert!(matches!(
        from_curie("vc:bc"),
        Err(UriError::ParseFailed(_))
    ));
}

#[test]
fn from_curie_rejects_empty_segment() {
    assert!(matches!(
        from_curie("vc:/foo"),
        Err(UriError::ParseFailed(_))
    ));
    assert!(matches!(
        from_curie("vc:bc/"),
        Err(UriError::ParseFailed(_))
    ));
}

#[test]
fn to_curie_concept_short_form() {
    let parsed = parse("urn:visionclaw:concept:bc:smart-contract").unwrap();
    assert_eq!(to_curie(&parsed), "vc:bc/smart-contract");
}

#[test]
fn parse_accepts_vc_curie() {
    match parse("vc:bc/smart-contract").unwrap() {
        ParsedUri::Concept { domain, slug } => {
            assert_eq!(domain, "bc");
            assert_eq!(slug, "smart-contract");
        }
        other => panic!("expected Concept, got {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// Negative cases — what the resolver's 400 path depends on
// ---------------------------------------------------------------------------

#[test]
fn parse_rejects_empty_input() {
    assert!(matches!(parse(""), Err(UriError::ParseFailed(_))));
}

#[test]
fn parse_rejects_unknown_scheme() {
    assert!(matches!(parse("http://example.com/foo"), Err(UriError::ParseFailed(_))));
    assert!(matches!(parse("urn:agentbox:bead:scope:hash"), Err(UriError::ParseFailed(_))));
}

#[test]
fn parse_rejects_unknown_kind() {
    assert!(matches!(
        parse("urn:visionclaw:wibble:foo:bar"),
        Err(UriError::UnknownKind(_))
    ));
}

#[test]
fn parse_rejects_concept_missing_slug() {
    assert!(matches!(
        parse("urn:visionclaw:concept:bc"),
        Err(UriError::ParseFailed(_))
    ));
}

#[test]
fn parse_rejects_group_missing_members_anchor() {
    assert!(matches!(
        parse("urn:visionclaw:group:astro"),
        Err(UriError::ParseFailed(_))
    ));
}

#[test]
fn parse_rejects_owned_with_non_npub_scope() {
    assert!(matches!(
        parse("urn:visionclaw:kg:not-an-npub:sha256-12-deadbeef0001"),
        Err(UriError::ParseFailed(_))
    ));
}

#[test]
fn parse_rejects_owned_with_bad_hash() {
    use nostr_sdk::{PublicKey, ToBech32};
    let pk = PublicKey::from_hex(TEST_PUBKEY).unwrap();
    let npub = pk.to_bech32().unwrap();

    // Hash too short
    assert!(parse(&format!("urn:visionclaw:kg:{}:sha256-12-abc", npub)).is_err());
    // Hash uppercase
    assert!(parse(&format!("urn:visionclaw:kg:{}:sha256-12-ABCDEF012345", npub)).is_err());
    // Missing prefix
    assert!(parse(&format!("urn:visionclaw:kg:{}:abcdef012345", npub)).is_err());
}

#[test]
fn parse_rejects_did_with_wrong_length() {
    assert!(matches!(
        parse("did:nostr:abc"),
        Err(UriError::InvalidPubkeyHex(_))
    ));
}

#[test]
fn is_canonical_returns_true_for_valid_forms() {
    assert!(is_canonical(&mint_concept("bc", "x")));
    assert!(is_canonical(&mint_group_members("astro")));
    assert!(is_canonical(&mint_did_nostr(TEST_PUBKEY).unwrap()));
    assert!(is_canonical("vc:bc/smart-contract"));
}

#[test]
fn is_canonical_returns_false_for_garbage() {
    assert!(!is_canonical(""));
    assert!(!is_canonical("nope"));
    assert!(!is_canonical("urn:visionclaw:wibble:x:y"));
}
