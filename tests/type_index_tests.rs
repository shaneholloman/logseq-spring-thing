//! ADR-029 Type Index integration tests.
//!
//! These focus on the JSON-LD parse/serialise envelope and the idempotent
//! upsert semantics. HTTP-driven flows (ensure/register/discover against a
//! real Pod) belong in a separate test harness that stands up a JSS shim
//! and is wired in a follow-up sprint — the unit tests in
//! `src/services/type_index_discovery.rs` cover the pure logic today.

use serde_json::Value;
use webxr::services::type_index_discovery::{
    type_index_url_for, uris, TypeIndexDocument, TypeRegistration,
};

#[test]
fn full_round_trip_preserves_agent_skill_registration() {
    let mut doc = TypeIndexDocument::empty(
        "https://pod.example.org/alice/settings/publicTypeIndex.jsonld",
    );
    let mut extra = serde_json::Map::new();
    extra.insert(
        "vf:capabilities".into(),
        Value::Array(vec![
            Value::String("code-review".into()),
            Value::String("security-audit".into()),
        ]),
    );
    extra.insert("vf:label".into(), Value::String("ReviewerBot".into()));

    doc.upsert(TypeRegistration {
        for_class: uris::AGENT_SKILL.into(),
        instance: Some("https://pod.example.org/alice/public/skills/reviewer/".into()),
        registered_at: Some(chrono::Utc::now()),
        extra,
    });

    let ser = serde_json::to_string(&doc.to_jsonld()).unwrap();
    let parsed = TypeIndexDocument::from_jsonld(&doc.url, &ser).unwrap();

    assert_eq!(parsed.registrations.len(), 1);
    let r = &parsed.registrations[0];
    assert_eq!(r.for_class, uris::AGENT_SKILL);
    assert_eq!(
        r.instance.as_deref(),
        Some("https://pod.example.org/alice/public/skills/reviewer/")
    );
    assert!(r.registered_at.is_some());
    assert_eq!(
        r.extra.get("vf:label").and_then(Value::as_str),
        Some("ReviewerBot")
    );
    let caps = r
        .extra
        .get("vf:capabilities")
        .and_then(Value::as_array)
        .expect("capabilities array preserved");
    assert_eq!(caps.len(), 2);
}

#[test]
fn contributor_profile_and_skill_coexist_in_one_index() {
    let mut doc = TypeIndexDocument::empty("https://pod.example.org/alice/ti");
    doc.upsert(TypeRegistration {
        for_class: uris::CONTRIBUTOR_PROFILE.into(),
        instance: Some("https://pod.example.org/alice/public/profile/".into()),
        registered_at: Some(chrono::Utc::now()),
        extra: Default::default(),
    });
    doc.upsert(TypeRegistration {
        for_class: uris::AGENT_SKILL.into(),
        instance: Some("https://pod.example.org/alice/public/skills/reviewer/".into()),
        registered_at: Some(chrono::Utc::now()),
        extra: Default::default(),
    });
    let skills: Vec<_> = doc.filter_by_class(uris::AGENT_SKILL).collect();
    let profiles: Vec<_> = doc.filter_by_class(uris::CONTRIBUTOR_PROFILE).collect();
    assert_eq!(skills.len(), 1);
    assert_eq!(profiles.len(), 1);
}

#[test]
fn upsert_replaces_entry_with_same_class_and_instance() {
    let mut doc = TypeIndexDocument::empty("http://p/ti");
    let instance = "https://pod.example.org/alice/public/skills/reviewer/";

    doc.upsert(TypeRegistration {
        for_class: uris::AGENT_SKILL.into(),
        instance: Some(instance.into()),
        registered_at: Some(chrono::Utc::now()),
        extra: Default::default(),
    });
    // Upsert again with refreshed timestamp — count must remain 1.
    let refreshed_at = chrono::Utc::now();
    doc.upsert(TypeRegistration {
        for_class: uris::AGENT_SKILL.into(),
        instance: Some(instance.into()),
        registered_at: Some(refreshed_at),
        extra: Default::default(),
    });
    assert_eq!(doc.registrations.len(), 1);
    assert_eq!(doc.registrations[0].registered_at, Some(refreshed_at));
}

#[test]
fn type_index_url_handles_all_webid_shapes() {
    assert_eq!(
        type_index_url_for("https://pod.example.org/alice/profile/card#me"),
        "https://pod.example.org/alice/settings/publicTypeIndex.jsonld"
    );
    assert_eq!(
        type_index_url_for("https://pod.example.org/alice/"),
        "https://pod.example.org/alice/settings/publicTypeIndex.jsonld"
    );
    assert_eq!(
        type_index_url_for("https://pod.example.org/alice/profile/card"),
        "https://pod.example.org/alice/settings/publicTypeIndex.jsonld"
    );
}

#[test]
fn serialised_document_has_solid_type_index_type() {
    let doc = TypeIndexDocument::empty("http://p/ti");
    let v = doc.to_jsonld();
    assert_eq!(v.get("@type").and_then(Value::as_str), Some("solid:TypeIndex"));
    assert!(v.get("solid:typeRegistration").is_some());
    assert!(v.get("@context").is_some());
}
