//! ADR-029 Type Index Discovery
//!
//! Implements the Solid Type Index for agent and view discovery per
//! ADR-029. Two flows are exposed:
//!
//! - **Write side (self)**: `ensure_public_type_index` creates
//!   `/settings/publicTypeIndex.jsonld` if absent and links it from the
//!   owner's WebID profile. `register_agent_in_type_index` adds (or upserts)
//!   a `solid:TypeRegistration` entry for `urn:solid:AgentSkill` or
//!   `urn:solid:ContributorProfile` instances.
//!
//! - **Read side (peers)**: `discover_peer_registrations` fetches a remote
//!   user's Type Index by WebID and returns the registrations matching a
//!   given `forClass` URI. This powers the `DojoDiscoveryActor` crawl and
//!   eventually feeds the `SkillIndex` read model owned by agent C2.
//!
//! ### Skill/profile-specific handling is **out of scope** here.
//!
//! This module only parses/serialises the Type Index envelope and the
//! generic registration envelope (class URI + instance URL + opaque
//! extension fields). Agent C2 consumes `discover_peer_registrations`
//! output and materialises `SkillPackage` projections; agent X1 wires the
//! MCP surface. This split keeps the discovery primitive reusable for
//! future registration types (e.g. `schema:ViewAction` from ADR-027).
//!
//! WAC enforcement is already handled by ADR-052: reads against public
//! containers succeed; reads against restricted resources return 403 and
//! are silently elided here (see `discover_peer_registrations`).

use bytes::Bytes;
use chrono::{DateTime, Utc};
use log::{debug, warn};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use thiserror::Error;

use crate::services::pod_client::{PodClient, PodClientError};

/// Canonical URIs recognised by the discovery layer.
pub mod uris {
    /// Entry type URI for agent skills per Contributor Nexus BC19.
    /// Used as `solid:forClass` value when registering a skill package.
    pub const AGENT_SKILL: &str = "urn:solid:AgentSkill";
    /// Entry type URI for contributor profiles per BC18.
    pub const CONTRIBUTOR_PROFILE: &str = "urn:solid:ContributorProfile";
    /// WebID predicate linking the profile to its public Type Index.
    pub const SOLID_PUBLIC_TYPE_INDEX: &str = "solid:publicTypeIndex";
    /// Registration class URI.
    pub const SOLID_TYPE_INDEX: &str = "solid:TypeIndex";
    pub const SOLID_TYPE_REGISTRATION: &str = "solid:TypeRegistration";
    pub const SOLID_FOR_CLASS: &str = "solid:forClass";
    pub const SOLID_INSTANCE: &str = "solid:instance";
}

/// Errors from Type Index operations.
#[derive(Debug, Error)]
pub enum TypeIndexError {
    #[error("Pod I/O error: {0}")]
    Pod(#[from] PodClientError),

    #[error("Malformed Type Index JSON-LD: {0}")]
    Malformed(String),

    #[error("WebID profile did not reference a publicTypeIndex and auto-create is disabled")]
    MissingTypeIndexLink,

    #[error("JSON (de)serialisation: {0}")]
    Json(#[from] serde_json::Error),
}

pub type TypeIndexResult<T> = Result<T, TypeIndexError>;

/// A parsed Type Registration entry. Extension fields are preserved verbatim
/// under `extra` so agent-specific payload (e.g. skill capabilities, profile
/// bio) round-trips without this module needing to know about them.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TypeRegistration {
    /// `solid:forClass` — the URI of the registered type (e.g.
    /// `urn:solid:AgentSkill`).
    pub for_class: String,
    /// `solid:instance` — absolute URL pointing at the container or resource
    /// holding instances of `for_class`. Optional for registrations that
    /// carry inline metadata instead of a URL (agent registrations that
    /// describe a capability set rather than a resource location).
    pub instance: Option<String>,
    /// ISO-8601 registration timestamp (`vf:registeredAt`). Always set for
    /// entries written by this module.
    pub registered_at: Option<DateTime<Utc>>,
    /// Verbatim remainder of the JSON-LD object minus the recognised fields.
    /// Keyed by the original JSON-LD property name (e.g. `"vf:capabilities"`).
    #[serde(default)]
    pub extra: Map<String, Value>,
}

/// Full Type Index document — `solid:TypeIndex` with zero or more
/// registrations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TypeIndexDocument {
    pub url: String,
    pub registrations: Vec<TypeRegistration>,
}

impl TypeIndexDocument {
    /// Build an empty Type Index at the given URL.
    pub fn empty(url: impl Into<String>) -> Self {
        Self { url: url.into(), registrations: Vec::new() }
    }

    /// Parse a JSON-LD Type Index document. Tolerant of both the array-form
    /// `solid:typeRegistration` (canonical) and the single-object form
    /// (produced by some clients when there is exactly one registration).
    pub fn from_jsonld(url: impl Into<String>, body: &str) -> TypeIndexResult<Self> {
        let value: Value = serde_json::from_str(body)
            .map_err(|e| TypeIndexError::Malformed(format!("not json: {e}")))?;

        let obj = value
            .as_object()
            .ok_or_else(|| TypeIndexError::Malformed("root is not an object".into()))?;

        let regs_field = obj.get("solid:typeRegistration").cloned();
        let registrations = match regs_field {
            None => Vec::new(),
            Some(Value::Array(a)) => a
                .into_iter()
                .filter_map(|v| parse_registration(&v).ok())
                .collect(),
            Some(other @ Value::Object(_)) => parse_registration(&other)
                .map(|r| vec![r])
                .unwrap_or_default(),
            Some(_) => {
                return Err(TypeIndexError::Malformed(
                    "solid:typeRegistration must be object or array".into(),
                ))
            }
        };

        Ok(Self { url: url.into(), registrations })
    }

    /// Serialise to canonical JSON-LD. Order-preserving on registrations so
    /// round-trips stay stable.
    pub fn to_jsonld(&self) -> Value {
        let regs: Vec<Value> = self.registrations.iter().map(registration_to_jsonld).collect();
        json!({
            "@context": {
                "solid": "http://www.w3.org/ns/solid/terms#",
                "schema": "https://schema.org/",
                "vf": "https://narrativegoldmine.com/ontology#"
            },
            "@type": uris::SOLID_TYPE_INDEX,
            "solid:typeRegistration": regs,
        })
    }

    /// Idempotent upsert: if a registration with the same `for_class` AND
    /// `instance` already exists, replace it; otherwise append. Returns
    /// `true` if a new entry was inserted, `false` if it replaced one.
    pub fn upsert(&mut self, reg: TypeRegistration) -> bool {
        let existing = self.registrations.iter_mut().find(|r| {
            r.for_class == reg.for_class && r.instance == reg.instance
        });
        match existing {
            Some(slot) => {
                *slot = reg;
                false
            }
            None => {
                self.registrations.push(reg);
                true
            }
        }
    }

    /// Filter registrations by `for_class` URI.
    pub fn filter_by_class<'a>(&'a self, class_uri: &'a str) -> impl Iterator<Item = &'a TypeRegistration> {
        self.registrations.iter().filter(move |r| r.for_class == class_uri)
    }
}

fn parse_registration(value: &Value) -> TypeIndexResult<TypeRegistration> {
    let obj = value
        .as_object()
        .ok_or_else(|| TypeIndexError::Malformed("registration is not an object".into()))?;

    // `@type` must be solid:TypeRegistration (if present); tolerate absence.
    if let Some(ty) = obj.get("@type").and_then(Value::as_str) {
        if ty != uris::SOLID_TYPE_REGISTRATION {
            warn!("[type_index] unexpected @type: {ty}");
        }
    }

    let for_class = obj
        .get(uris::SOLID_FOR_CLASS)
        .and_then(Value::as_str)
        .ok_or_else(|| TypeIndexError::Malformed(format!("missing {}", uris::SOLID_FOR_CLASS)))?
        .to_string();

    let instance = obj
        .get(uris::SOLID_INSTANCE)
        .and_then(Value::as_str)
        .map(str::to_string);

    let registered_at = obj
        .get("vf:registeredAt")
        .and_then(Value::as_str)
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc));

    let mut extra = Map::new();
    for (k, v) in obj.iter() {
        match k.as_str() {
            "@type" | "@id" | "solid:forClass" | "solid:instance" | "vf:registeredAt" => {}
            _ => {
                extra.insert(k.clone(), v.clone());
            }
        }
    }

    Ok(TypeRegistration { for_class, instance, registered_at, extra })
}

fn registration_to_jsonld(reg: &TypeRegistration) -> Value {
    let mut obj = Map::new();
    obj.insert("@type".into(), Value::String(uris::SOLID_TYPE_REGISTRATION.into()));
    obj.insert("solid:forClass".into(), Value::String(reg.for_class.clone()));
    if let Some(inst) = &reg.instance {
        obj.insert("solid:instance".into(), Value::String(inst.clone()));
    }
    if let Some(ts) = &reg.registered_at {
        obj.insert(
            "vf:registeredAt".into(),
            Value::String(ts.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)),
        );
    }
    for (k, v) in &reg.extra {
        obj.insert(k.clone(), v.clone());
    }
    Value::Object(obj)
}

/// Derive the canonical `publicTypeIndex.jsonld` URL from a WebID or Pod
/// root URL. If `webid_or_pod_root` ends with `/profile/card#me` or similar,
/// the `#fragment` is stripped and we ascend to the Pod root, appending
/// `/settings/publicTypeIndex.jsonld`.
pub fn type_index_url_for(webid_or_pod_root: &str) -> String {
    let trimmed = webid_or_pod_root
        .split('#')
        .next()
        .unwrap_or(webid_or_pod_root)
        .trim_end_matches('/');
    // Strip a trailing `/profile/card` (Solid convention for WebID docs).
    let root = trimmed
        .strip_suffix("/profile/card")
        .unwrap_or(trimmed)
        .trim_end_matches('/');
    format!("{root}/settings/publicTypeIndex.jsonld")
}

/// Ensure the `publicTypeIndex.jsonld` for the given WebID exists on the Pod;
/// if absent, create an empty one and (best-effort) link it from the WebID
/// profile document.
///
/// Returns the fetched-or-freshly-created document.
pub async fn ensure_public_type_index(
    client: &PodClient,
    webid: &str,
    auth_header: Option<&str>,
) -> TypeIndexResult<TypeIndexDocument> {
    let url = type_index_url_for(webid);

    match client.get_resource(&url, Some("application/ld+json"), auth_header).await? {
        Some((body, _ct)) => {
            debug!("[type_index] ensure: fetched existing index at {url}");
            TypeIndexDocument::from_jsonld(&url, &body)
        }
        None => {
            debug!("[type_index] ensure: creating new index at {url}");
            let doc = TypeIndexDocument::empty(&url);
            write_type_index(client, &doc, auth_header).await?;
            // Link from WebID profile (best-effort — failure here is logged
            // but does not abort; a peer can still be discovered via direct
            // URL until the profile is patched).
            if let Err(e) = link_type_index_in_profile(client, webid, &url, auth_header).await {
                warn!("[type_index] failed to link publicTypeIndex in WebID profile: {e}");
            }
            Ok(doc)
        }
    }
}

/// Append or upsert an entry for `type_uri` pointing at `container_uri`.
///
/// `type_uri` should be one of `uris::AGENT_SKILL` or
/// `uris::CONTRIBUTOR_PROFILE`; this function does not validate it so that
/// future registration types can flow through without code changes.
pub async fn register_agent_in_type_index(
    client: &PodClient,
    webid: &str,
    type_uri: &str,
    container_uri: &str,
    auth_header: Option<&str>,
) -> TypeIndexResult<TypeIndexDocument> {
    let mut doc = ensure_public_type_index(client, webid, auth_header).await?;
    let reg = TypeRegistration {
        for_class: type_uri.to_string(),
        instance: Some(container_uri.to_string()),
        registered_at: Some(Utc::now()),
        extra: Map::new(),
    };
    doc.upsert(reg);
    write_type_index(client, &doc, auth_header).await?;
    Ok(doc)
}

/// Crawl a peer's public Type Index by WebID and return all registrations
/// matching `class_filter` (e.g. `uris::AGENT_SKILL`).
///
/// Missing or unauthorised peer Type Indexes yield an empty vector rather
/// than erroring — discovery is best-effort by design (ADR-029 §Mitigations).
pub async fn discover_peer_registrations(
    client: &PodClient,
    peer_webid: &str,
    class_filter: &str,
    auth_header: Option<&str>,
) -> Vec<TypeRegistration> {
    let url = type_index_url_for(peer_webid);
    match client.get_resource(&url, Some("application/ld+json"), auth_header).await {
        Ok(Some((body, _ct))) => match TypeIndexDocument::from_jsonld(&url, &body) {
            Ok(doc) => doc.filter_by_class(class_filter).cloned().collect(),
            Err(e) => {
                warn!("[type_index] peer {peer_webid}: parse failed: {e}");
                Vec::new()
            }
        },
        Ok(None) => {
            debug!("[type_index] peer {peer_webid}: no public type index");
            Vec::new()
        }
        Err(e) => {
            warn!("[type_index] peer {peer_webid}: fetch failed: {e}");
            Vec::new()
        }
    }
}

async fn write_type_index(
    client: &PodClient,
    doc: &TypeIndexDocument,
    auth_header: Option<&str>,
) -> TypeIndexResult<()> {
    let body = serde_json::to_vec(&doc.to_jsonld())?;
    client
        .put_resource(
            &doc.url,
            Bytes::from(body),
            "application/ld+json",
            auth_header,
        )
        .await?;
    Ok(())
}

/// Patch the WebID profile to include `solid:publicTypeIndex -> type_index_url`.
///
/// If the profile document already references a Type Index, this is a no-op.
/// If the profile is unreachable or unparseable, an error surfaces to the
/// caller which logs and continues.
async fn link_type_index_in_profile(
    client: &PodClient,
    webid: &str,
    type_index_url: &str,
    auth_header: Option<&str>,
) -> TypeIndexResult<()> {
    let profile_url = webid
        .split('#')
        .next()
        .unwrap_or(webid)
        .to_string();

    let (existing_body, _ct) = match client
        .get_resource(&profile_url, Some("application/ld+json"), auth_header)
        .await?
    {
        Some(x) => x,
        None => {
            // No profile document → nothing to link. Caller will rely on
            // direct URL discovery until the profile is bootstrapped.
            warn!("[type_index] no WebID profile at {profile_url}; skipping link");
            return Ok(());
        }
    };

    let mut profile: Value = serde_json::from_str(&existing_body)
        .map_err(|e| TypeIndexError::Malformed(format!("profile json: {e}")))?;

    let obj = profile
        .as_object_mut()
        .ok_or_else(|| TypeIndexError::Malformed("profile root not object".into()))?;

    // Already linked? Skip.
    if let Some(v) = obj.get(uris::SOLID_PUBLIC_TYPE_INDEX) {
        let linked = match v {
            Value::String(s) => s.as_str() == type_index_url,
            Value::Object(m) => m
                .get("@id")
                .and_then(Value::as_str)
                .map(|s| s == type_index_url)
                .unwrap_or(false),
            _ => false,
        };
        if linked {
            return Ok(());
        }
    }

    obj.insert(
        uris::SOLID_PUBLIC_TYPE_INDEX.to_string(),
        json!({ "@id": type_index_url }),
    );

    let body = serde_json::to_vec(&profile)?;
    client
        .put_resource(
            &profile_url,
            Bytes::from(body),
            "application/ld+json",
            auth_header,
        )
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_index_url_derivation_strips_webid_fragment() {
        assert_eq!(
            type_index_url_for("https://pod.example.org/alice/profile/card#me"),
            "https://pod.example.org/alice/settings/publicTypeIndex.jsonld"
        );
        assert_eq!(
            type_index_url_for("https://pod.example.org/alice/"),
            "https://pod.example.org/alice/settings/publicTypeIndex.jsonld"
        );
        assert_eq!(
            type_index_url_for("https://pod.example.org/alice"),
            "https://pod.example.org/alice/settings/publicTypeIndex.jsonld"
        );
    }

    #[test]
    fn empty_document_round_trips() {
        let doc = TypeIndexDocument::empty("https://pod.example.org/a/settings/publicTypeIndex.jsonld");
        let ser = serde_json::to_string(&doc.to_jsonld()).unwrap();
        let parsed = TypeIndexDocument::from_jsonld(&doc.url, &ser).unwrap();
        assert_eq!(parsed.registrations.len(), 0);
        assert_eq!(parsed.url, doc.url);
    }

    #[test]
    fn upsert_is_idempotent_by_class_and_instance() {
        let mut doc = TypeIndexDocument::empty("http://p/ti");
        let r1 = TypeRegistration {
            for_class: uris::AGENT_SKILL.into(),
            instance: Some("http://p/skills/a/".into()),
            registered_at: Some(Utc::now()),
            extra: Map::new(),
        };
        assert!(doc.upsert(r1.clone())); // inserted
        assert!(!doc.upsert(r1.clone())); // replaced (same class+instance)
        assert_eq!(doc.registrations.len(), 1);

        let r2 = TypeRegistration {
            for_class: uris::AGENT_SKILL.into(),
            instance: Some("http://p/skills/b/".into()),
            registered_at: Some(Utc::now()),
            extra: Map::new(),
        };
        assert!(doc.upsert(r2)); // different instance → new entry
        assert_eq!(doc.registrations.len(), 2);
    }

    #[test]
    fn filter_by_class_only_returns_matches() {
        let mut doc = TypeIndexDocument::empty("http://p/ti");
        doc.upsert(TypeRegistration {
            for_class: uris::AGENT_SKILL.into(),
            instance: Some("http://p/skills/a/".into()),
            registered_at: None,
            extra: Map::new(),
        });
        doc.upsert(TypeRegistration {
            for_class: uris::CONTRIBUTOR_PROFILE.into(),
            instance: Some("http://p/profile/".into()),
            registered_at: None,
            extra: Map::new(),
        });
        let skills: Vec<_> = doc.filter_by_class(uris::AGENT_SKILL).collect();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].instance.as_deref(), Some("http://p/skills/a/"));
    }

    #[test]
    fn parse_tolerates_single_object_registration() {
        let body = r#"{
          "@context": { "solid": "http://www.w3.org/ns/solid/terms#" },
          "@type": "solid:TypeIndex",
          "solid:typeRegistration": {
            "@type": "solid:TypeRegistration",
            "solid:forClass": "urn:solid:AgentSkill",
            "solid:instance": "https://pod.example/alice/public/skills/"
          }
        }"#;
        let doc = TypeIndexDocument::from_jsonld("http://p/ti", body).unwrap();
        assert_eq!(doc.registrations.len(), 1);
        assert_eq!(doc.registrations[0].for_class, uris::AGENT_SKILL);
    }

    #[test]
    fn parse_preserves_extra_fields() {
        let body = r#"{
          "@context": { "solid": "http://www.w3.org/ns/solid/terms#" },
          "@type": "solid:TypeIndex",
          "solid:typeRegistration": [
            {
              "@type": "solid:TypeRegistration",
              "solid:forClass": "urn:solid:AgentSkill",
              "solid:instance": "https://pod.example/alice/public/skills/",
              "vf:capabilities": ["review", "audit"],
              "vf:label": "Reviewer"
            }
          ]
        }"#;
        let doc = TypeIndexDocument::from_jsonld("http://p/ti", body).unwrap();
        let reg = &doc.registrations[0];
        assert!(reg.extra.contains_key("vf:capabilities"));
        assert_eq!(reg.extra.get("vf:label").and_then(Value::as_str), Some("Reviewer"));
    }

    #[test]
    fn malformed_json_is_reported_not_panicked() {
        let err = TypeIndexDocument::from_jsonld("http://p/ti", "not-json").unwrap_err();
        matches!(err, TypeIndexError::Malformed(_));
    }
}
