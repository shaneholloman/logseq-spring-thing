//! URN-keyed resolver (PRD-006 §5.2).
//!
//! Three routes, semantically symmetric with agentbox's `/v1/uri/<urn>`:
//!
//!   - `GET /api/v1/uri/{urn}`             — resolve a URN to a JSON-LD or
//!                                            DID document via 307 Location.
//!   - `GET /api/v1/uri/by-curie/{curie}`  — same, taking the `vc:<dom>/<slug>`
//!                                            CURIE form (the database key).
//!   - `GET /api/v1/uri`                   — self-describing index of the
//!                                            grammar + supported kinds.
//!
//! Status semantics (PRD-006 §5.2 + agentbox `routes/uri-resolver.js`):
//!
//!   | parsed kind        | hit          | miss             | malformed |
//!   |--------------------|--------------|------------------|-----------|
//!   | Concept            | 307          | 404              | 400       |
//!   | OwnedKg            | 307          | 404              | 400       |
//!   | Did                | 307          | 404              | 400       |
//!   | Group              | 307          | 404              | 400       |
//!   | Bead               | 404 + hint*  | 404              | 400       |
//!   | AgentExecution     | 404 + hint*  | 404              | 400       |
//!
//! *Bead and AgentExecution federate to agentbox via the BC20 ACL (PRD-006
//! §5.5). Until BC20 lands, the resolver returns 404 with a "federation-hop"
//! hint so callers know to try the agentbox sibling.
//!
//! NOTE: this handler currently only ANSWERS resolves; the actual lookup
//! against Neo4j (`MATCH (n) WHERE n.iri = $key OR n.visionclaw_uri = $urn`)
//! is wired here as a stub returning 404. Wiring the live query is a P3
//! concern paired with the BC20 federation work; for P2 we ship the grammar
//! surface, error envelopes, and CURIE↔URN normalisation so agentbox can
//! point at us today.

use actix_web::{web, HttpResponse, Responder};
use serde::Serialize;

use crate::uri::{from_curie, parse, ParsedUri, UriError};

// ----------------------------------------------------------------------------
// Response envelope
// ----------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct ErrorBody {
    error: &'static str,
    detail: String,
    /// Human-readable hint pointing at the canonical grammar; included on
    /// 400s so the caller knows what was expected.
    grammar_hint: Option<&'static str>,
}

/// Public grammar reference for `GET /api/v1/uri` (no urn).
#[derive(Debug, Serialize)]
struct GrammarDescription {
    description: &'static str,
    forms: Vec<KindEntry>,
}

#[derive(Debug, Serialize)]
struct KindEntry {
    kind: &'static str,
    urn_template: &'static str,
    curie_template: Option<&'static str>,
    resolvable: bool,
    notes: &'static str,
}

const GRAMMAR_HINT: &str =
    "URN forms: urn:visionclaw:concept:<domain>:<slug> | \
     urn:visionclaw:group:<team>#members | \
     urn:visionclaw:kg:<npub>:<sha256-12-hex> | \
     urn:visionclaw:bead:<npub>:<sha256-12-hex> | \
     urn:visionclaw:execution:<sha256-12-hex> | \
     did:nostr:<64-hex>. \
     CURIE: vc:<domain>/<slug>.";

// ----------------------------------------------------------------------------
// Handlers
// ----------------------------------------------------------------------------

/// `GET /api/v1/uri` — describe the grammar surface.
async fn describe() -> impl Responder {
    HttpResponse::Ok().json(GrammarDescription {
        description: "VisionClaw URN/CURIE resolver. PRD-006 §5.2.",
        forms: vec![
            KindEntry {
                kind: "concept",
                urn_template: "urn:visionclaw:concept:<domain>:<slug>",
                curie_template: Some("vc:<domain>/<slug>"),
                resolvable: true,
                notes: "R3 (stable on identity). Joined to :KGNode/:OntologyClass via .iri (CURIE) or .visionclaw_uri (URN).",
            },
            KindEntry {
                kind: "group",
                urn_template: "urn:visionclaw:group:<team>#members",
                curie_template: None,
                resolvable: true,
                notes: "R3. Resolves to /api/v1/wac/groups/<team>.",
            },
            KindEntry {
                kind: "kg",
                urn_template: "urn:visionclaw:kg:<npub>:<sha256-12-hex>",
                curie_template: None,
                resolvable: true,
                notes: "R1+R2. Owner-scoped, content-addressed.",
            },
            KindEntry {
                kind: "bead",
                urn_template: "urn:visionclaw:bead:<npub>:<sha256-12-hex>",
                curie_template: None,
                resolvable: false,
                notes: "Federates to agentbox. 404 with federation-hop hint until BC20 lands.",
            },
            KindEntry {
                kind: "execution",
                urn_template: "urn:visionclaw:execution:<sha256-12-hex>",
                curie_template: None,
                resolvable: false,
                notes: "Federates to agentbox.",
            },
            KindEntry {
                kind: "did",
                urn_template: "did:nostr:<64-hex>",
                curie_template: None,
                resolvable: true,
                notes: "R3. Resolves to /api/v1/identity/<hex>/did.json.",
            },
        ],
    })
}

/// `GET /api/v1/uri/{urn}` — the main resolver.
async fn resolve(path: web::Path<String>) -> impl Responder {
    let urn = path.into_inner();
    resolve_inner(&urn)
}

/// `GET /api/v1/uri/by-curie/{curie}` — CURIE entry point. Normalises to a
/// URN and delegates to the same resolution logic.
async fn resolve_by_curie(path: web::Path<String>) -> impl Responder {
    let curie = path.into_inner();
    match from_curie(&curie) {
        Ok(urn) => resolve_inner(&urn),
        Err(e) => malformed(&curie, e),
    }
}

fn resolve_inner(urn: &str) -> HttpResponse {
    match parse(urn) {
        Ok(parsed) => match parsed {
            ParsedUri::Concept { .. } => redirect_307(format!(
                "/api/v1/nodes/by-uri/{}/jsonld",
                urlencoded(urn)
            )),
            ParsedUri::OwnedKg { .. } => redirect_307(format!(
                "/api/v1/nodes/by-uri/{}/jsonld",
                urlencoded(urn)
            )),
            ParsedUri::Did { pubkey_hex } => redirect_307(format!(
                "/api/v1/identity/{}/did.json",
                pubkey_hex
            )),
            ParsedUri::Group { team } => redirect_307(format!(
                "/api/v1/wac/groups/{}",
                urlencoded(&team)
            )),
            ParsedUri::Bead { .. } | ParsedUri::AgentExecution { .. } => {
                federation_hop(urn)
            }
        },
        Err(e) => malformed(urn, e),
    }
}

// ----------------------------------------------------------------------------
// Status helpers
// ----------------------------------------------------------------------------

fn redirect_307(location: String) -> HttpResponse {
    HttpResponse::TemporaryRedirect()
        .insert_header(("Location", location))
        .finish()
}

fn malformed(input: &str, err: UriError) -> HttpResponse {
    HttpResponse::BadRequest().json(ErrorBody {
        error: "malformed_uri",
        detail: format!("could not parse '{}': {}", input, err),
        grammar_hint: Some(GRAMMAR_HINT),
    })
}

fn federation_hop(urn: &str) -> HttpResponse {
    // 404 with a `federation-hop` hint. PRD-006 §5.5 — once BC20 ships,
    // this becomes a 307 to the agentbox sibling's `/v1/uri/<urn>`.
    HttpResponse::NotFound().json(ErrorBody {
        error: "federation_hop_required",
        detail: format!(
            "URN '{}' is owned by the agentbox sibling. Try its /v1/uri/ endpoint. \
             Federation hop is wired by BC20 (not yet shipped).",
            urn
        ),
        grammar_hint: Some(GRAMMAR_HINT),
    })
}

/// Minimal URL-encoder for the bits we put in a Location header.
/// Only escapes the characters that would otherwise break path parsing.
fn urlencoded(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            ' ' => out.push_str("%20"),
            '#' => out.push_str("%23"),
            '?' => out.push_str("%3F"),
            // Letters, digits, and ':-./_~' pass through.
            c if c.is_ascii_alphanumeric() => out.push(c),
            ':' | '-' | '.' | '/' | '_' | '~' => out.push(c),
            // Anything else: percent-encode as UTF-8 bytes.
            c => {
                let mut buf = [0u8; 4];
                for b in c.encode_utf8(&mut buf).bytes() {
                    out.push_str(&format!("%{:02X}", b));
                }
            }
        }
    }
    out
}

// ----------------------------------------------------------------------------
// Route registration
// ----------------------------------------------------------------------------

/// Register `/v1/uri/...` routes under the parent scope.
/// Mounted by main.rs under `web::scope("/api")` so the full path becomes
/// `/api/v1/uri/...`.
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/v1/uri")
            .route("", web::get().to(describe))
            // CURIE form takes precedence — must come before the catch-all
            // `/{urn}` route or actix would try to interpret "by-curie" as
            // a URN.
            .route("/by-curie/{curie:.*}", web::get().to(resolve_by_curie))
            // The URN may contain colons, dots, slashes — `:.*` lets actix
            // pass the whole tail as a single path segment.
            .route("/{urn:.*}", web::get().to(resolve)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urlencoded_passes_safe_chars_through() {
        assert_eq!(urlencoded("vc:bc/smart-contract"), "vc:bc/smart-contract");
        assert_eq!(urlencoded("urn:visionclaw:concept:bc:foo"), "urn:visionclaw:concept:bc:foo");
    }

    #[test]
    fn urlencoded_escapes_hash_and_space() {
        assert_eq!(urlencoded("foo bar#baz"), "foo%20bar%23baz");
    }
}
