//! `GET /.well-known/did/nostr/{pubkey}.json` — DID document endpoint.
//!
//! Serves a Tier-1 DID document for `did:nostr:<pubkey>` identities.
//! Document rendering is delegated to `solid_pod_rs::did_nostr_types` --
//! the canonical single-source implementation (ADR-074 D1). This handler
//! adds only VisionClaw-specific concerns: Neo4j existence gating, HTTP
//! response building, server-identity bypass, and caching headers.
//!
//! The canonical renderer produces:
//!   * `@context` with W3C DID Core v1 + secp256k1-2019 suite
//!   * `SchnorrSecp256k1VerificationKey2019` verification method
//!   * `publicKeyMultibase` in z-form base58btc (multicodec `0xe7`)
//!   * `publicKeyHex` for backward compatibility
//!
//! Response headers:
//!   * `Content-Type: application/did+json`
//!   * `Cache-Control: public, max-age=300`
//!
//! Errors:
//!   * 400 — invalid pubkey (not 64-char hex)
//!   * 404 — pubkey not found in user storage (Neo4j)

use actix_web::{web, HttpResponse};
use serde_json::json;
use solid_pod_rs::did_nostr_types::{
    did_nostr_uri, is_valid_hex_pubkey, render_did_document_tier1, NostrPubkey,
};

use crate::adapters::neo4j_adapter::Neo4jAdapter;
use crate::AppState;

// ─────────────────────────────────────────────────────────────────────────────
// Neo4j user lookup
// ─────────────────────────────────────────────────────────────────────────────

/// Check whether a pubkey is known to the system. The server's own Nostr
/// identity is always considered "known"; for other pubkeys we probe Neo4j
/// for any node whose owner matches `did:nostr:<pubkey>`.
async fn pubkey_exists(neo4j: &Neo4jAdapter, pubkey_hex: &str) -> bool {
    let cypher = "MATCH (n) WHERE n.owner = $owner RETURN n LIMIT 1";
    let did_uri = format!("did:nostr:{}", pubkey_hex);
    let q = neo4rs::Query::new(cypher.to_string()).param("owner", did_uri);
    let result = neo4j.graph().execute(q).await;
    match result {
        Ok(mut stream) => stream.next().await.ok().flatten().is_some(),
        Err(_) => false,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Handler
// ─────────────────────────────────────────────────────────────────────────────

/// `GET /.well-known/did/nostr/{pubkey}.json`
///
/// Returns a Tier-1 DID document for the given Nostr pubkey. Document
/// generation is delegated to `solid_pod_rs::did_nostr_types` -- the
/// canonical renderer shared across all DreamLab services.
///
/// VisionClaw extends the upstream Tier-1 skeleton with `authentication`
/// and `assertionMethod` arrays (matching the NRF forum convention) so
/// that clients checking authentication purpose accept the document.
pub async fn get_did_document(
    path: web::Path<String>,
    app_state: web::Data<AppState>,
    server_identity: web::Data<std::sync::Arc<crate::services::server_identity::ServerIdentity>>,
) -> HttpResponse {
    let raw_pubkey = path.into_inner();

    // Strip the `.json` suffix if present (the route captures it as part of {pubkey}).
    let pubkey = raw_pubkey.strip_suffix(".json").unwrap_or(&raw_pubkey);

    // Validate: must be 64-char hex.
    let pubkey_lower = pubkey.to_lowercase();
    if !is_valid_hex_pubkey(&pubkey_lower) {
        return HttpResponse::BadRequest()
            .content_type("application/json")
            .body(r#"{"error":"invalid pubkey: must be 64-char hex"}"#);
    }

    // The server's own pubkey is always resolvable without a Neo4j lookup.
    let is_server_key = pubkey_lower == server_identity.pubkey_hex().to_lowercase();

    if !is_server_key && !pubkey_exists(&app_state.neo4j_adapter, &pubkey_lower).await {
        return HttpResponse::NotFound()
            .content_type("application/json")
            .body(r#"{"error":"pubkey not found"}"#);
    }

    // Delegate to the canonical did:nostr renderer from solid-pod-rs.
    let pk = match NostrPubkey::from_hex(&pubkey_lower) {
        Ok(pk) => pk,
        Err(e) => {
            return HttpResponse::BadRequest()
                .content_type("application/json")
                .body(format!(r#"{{"error":"{}"}}"#, e));
        }
    };

    let mut doc = render_did_document_tier1(&pk);

    // Extend Tier-1 with authentication + assertionMethod (VisionClaw
    // convention, matching NRF forum -- clients that check the
    // `authentication` relationship before accepting signatures need these).
    let did = did_nostr_uri(&pk);
    let vm_ref = format!("{did}#nostr-schnorr");
    doc["authentication"] = json!([&vm_ref]);
    doc["assertionMethod"] = json!([&vm_ref]);

    let body = match serde_json::to_string(&doc) {
        Ok(b) => b,
        Err(e) => {
            return HttpResponse::InternalServerError()
                .content_type("application/json")
                .body(format!(r#"{{"error":"serialization: {}"}}"#, e));
        }
    };

    HttpResponse::Ok()
        .content_type("application/did+json")
        .insert_header(("Cache-Control", "public, max-age=300"))
        .body(body)
}

// ─────────────────────────────────────────────────────────────────────────────
// Route configuration
// ─────────────────────────────────────────────────────────────────────────────

/// Mount the DID document endpoint. Called from `main.rs` outside the `/api`
/// scope so the route lives at the server root:
///
/// ```ignore
/// .configure(webxr::handlers::configure_did_nostr_routes)
/// ```
///
/// The route pattern `/.well-known/did/nostr/{pubkey}` captures both
/// `/.../{hex}.json` (browser) and `/.../{hex}` (programmatic) forms.
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.route(
        "/.well-known/did/nostr/{pubkey}",
        web::get().to(get_did_document),
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use solid_pod_rs::did_nostr_types::{
        is_valid_hex_pubkey, render_did_document_tier1, NostrPubkey,
    };

    const PK_HEX: &str = "0000000000000000000000000000000000000000000000000000000000000001";

    #[test]
    fn valid_hex_pubkeys() {
        assert!(is_valid_hex_pubkey(
            "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2"
        ));
        assert!(is_valid_hex_pubkey(PK_HEX));
    }

    #[test]
    fn invalid_hex_pubkeys() {
        assert!(!is_valid_hex_pubkey("abcd"));
        assert!(!is_valid_hex_pubkey(
            "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2aa"
        ));
        assert!(!is_valid_hex_pubkey(
            "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"
        ));
        assert!(!is_valid_hex_pubkey(""));
    }

    #[test]
    fn tier1_uses_canonical_renderer() {
        let pk = NostrPubkey::from_hex(PK_HEX).unwrap();
        let doc = render_did_document_tier1(&pk);
        assert_eq!(doc["id"], format!("did:nostr:{PK_HEX}"));

        // Correct @context (secp256k1-2019, not ed2020).
        assert_eq!(doc["@context"][0], "https://www.w3.org/ns/did/v1");
        assert_eq!(
            doc["@context"][1],
            "https://w3id.org/security/suites/secp256k1-2019/v1"
        );

        // Canonical verification method type and fragment.
        let vm = &doc["verificationMethod"][0];
        assert_eq!(vm["type"], "SchnorrSecp256k1VerificationKey2019");
        assert_eq!(vm["id"], format!("did:nostr:{PK_HEX}#nostr-schnorr"));

        // publicKeyMultibase present with z-prefix (base58btc).
        assert!(vm["publicKeyMultibase"].as_str().unwrap().starts_with('z'));

        // publicKeyHex present for backward compat.
        assert_eq!(vm["publicKeyHex"], PK_HEX);
    }

    #[test]
    fn tier1_uppercase_rejected_by_canonical_validator() {
        // solid-pod-rs canonical is_valid_hex_pubkey rejects uppercase.
        let upper = "611DF01BFCF85C26AE65453B772D8F1DFD25C264621C0277E1FC1518686FAEF9";
        assert!(!is_valid_hex_pubkey(upper));
        // But NostrPubkey::from_hex also rejects (upstream requires lowercase).
        assert!(NostrPubkey::from_hex(upper).is_err());
    }
}
