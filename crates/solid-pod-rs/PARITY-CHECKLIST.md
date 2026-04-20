# JSS ↔ solid-pod-rs Parity Checklist

Tracks every Solid protocol feature implemented by
JavaScriptSolidServer (JSS, vendored at
`references/javascript-solid-server/`) against solid-pod-rs. Updated
every time a feature ships or regresses.

## Status key

| Status                 | Meaning                                                        |
|------------------------|----------------------------------------------------------------|
| present                | Feature works in solid-pod-rs and has passing tests.            |
| explicitly-deferred    | Intentionally out of scope; rationale + target ADR recorded.    |
| partial                | Some sub-features exist; gaps documented in the Notes column.   |
| missing                | Not yet implemented.                                            |

**Sprint 3 close (2026-04-20): 67/67 — 62 present, 5 explicitly-deferred, 0 missing, 0 partial.**

## Core LDP

| Feature                                          | Status  | Target | Tests | Notes |
|--------------------------------------------------|---------|--------|-------|-------|
| LDP Resource (RDF/non-RDF) GET/HEAD              | present | P1     | `storage_trait.rs`, `interop_jss.rs` | `Storage::get`/`head`; Link `rel=type` emitted. |
| LDP Resource PUT (create-or-replace)             | present | P1     | `storage_trait.rs`, `interop_jss.rs::jss_put_resource_lifecycle` | `Storage::put`; returns ETag. |
| LDP Resource DELETE                              | present | P1     | `storage_trait.rs`, `interop_jss.rs::jss_delete_resource_lifecycle` | `Storage::delete`. |
| LDP Basic Container GET (contains triples)       | present | P1     | `interop_jss.rs::jss_get_container_*` (x4) | `ldp::render_container`. |
| LDP Container POST (Slug → child resource)       | present | P1     | `interop_jss.rs::jss_slug_safe_names_pass_through_unchanged`, `ldp::tests::slug_*` | `ldp::resolve_slug`; UUID fallback. |
| LDP PUT-to-container rejection                   | present | P1     | `interop_jss.rs::jss_not_found_shape` | Example server returns 405. |
| Server-managed triples (dateModified, size)      | present | P2     | `ldp::tests::server_managed_triples_*`, `find_illegal_*` | `ldp::server_managed_triples`. |
| `contains` triples include direct children only  | present | P1     | `storage_trait.rs` | `Storage::list` collapses nested. |
| Prefer headers (composed include/omit directives) | present | P2    | `ldp::tests::prefer_*`, `interop_jss.rs::jss_prefer_compose_include_minimal_and_contained_iris` | `ldp::PreferHeader::parse`; selects representation mode. Multi-include tolerated. |
| Accept-Post on containers                        | present | P2     | `interop_jss.rs::jss_options_container_advertises_accept_post_and_ranges` | `ldp::ACCEPT_POST` constant + `link_headers` emits all three media types. |
| `.meta` sidecar auto-link                        | present | P2     | `interop_jss.rs::jss_meta_sidecar_link_always_present_on_non_meta_resources` | `link_headers` emits `describedby` for every non-meta resource. |

## Web Access Control (WAC)

| Feature                                          | Status  | Target | Tests | Notes |
|--------------------------------------------------|---------|--------|-------|-------|
| `acl:Read` mode                                  | present | P1     | `wac::tests::public_read_grants_anonymous`, `wac_inheritance.rs` (x31) | `wac::evaluate_access`. |
| `acl:Write` mode (implies Append)                | present | P1     | `wac::tests::write_implies_append` | Unit-tested. |
| `acl:Append` mode                                | present | P1     | `wac_inheritance.rs` | |
| `acl:Control` mode                               | present | P1     | `interop_jss.rs::jss_turtle_acl_control_grants_acl_rw` | |
| `acl:agent` (specific agent)                     | present | P1     | `wac_inheritance.rs` | Multi-agent block covered by inheritance corpus. |
| `acl:agentClass foaf:Agent` (public)             | present | P1     | `wac::tests::public_read_grants_anonymous` | |
| `acl:agentClass acl:AuthenticatedAgent`          | present | P1     | `wac_inheritance.rs` | |
| `acl:agentGroup` (vcard:Group)                   | present | P2     | `wac_inheritance.rs::group_membership_grants_access` | evaluates group resolver. |
| `acl:accessTo` exact + child match               | present | P1     | `wac_inheritance.rs` | |
| `acl:default` container inheritance              | present | P1     | `wac_inheritance.rs` (15+ scenarios) | Expanded in inheritance corpus. |
| `.acl` sidecar resolution walking up tree        | present | P1     | `wac_inheritance.rs` | `StorageAclResolver`. |
| `WAC-Allow` response header                      | present | P1     | `wac::tests::wac_allow_shape` | `wac::wac_allow_header`. |
| ACL document read via HTTP (GET `.acl`)          | present | P2     | `interop_jss.rs::jss_acl_document_can_be_fetched_via_get` | Interop tests. |
| ACL document write requires `acl:Control`        | present | P2     | `wac_inheritance.rs` | Interop test. |
| Turtle-serialized ACL documents                  | present | P2     | `wac::tests::turtle_acl_round_trip_*` (x3), `parity_close.rs::turtle_acl_*` (x2), `interop_jss.rs::jss_turtle_acl_*` (x2) | `parse_turtle_acl` + `serialize_turtle_acl`; `StorageAclResolver` falls back to Turtle when JSON-LD parse fails. |

## Authentication

| Feature                                          | Status  | Target | Tests | Notes |
|--------------------------------------------------|---------|--------|-------|-------|
| NIP-98 HTTP auth (kind 27235, `u`/`method`/`payload` tags) | present | P1 | `auth::nip98::tests::*` (x7) | `auth::nip98::verify_at` — all structural checks. |
| NIP-98 Schnorr signature verification            | present | P2     | `auth::nip98::tests::compute_event_id_matches_canonical_hash`, `tests/schnorr_nip98.rs::*` (x2, feature-gated) | `auth::nip98::verify_schnorr_signature` under `nip98-schnorr` feature via `k256`. |
| NIP-98 timestamp tolerance (60s)                 | present | P1     | `auth::nip98::tests::rejects_expired_timestamp` | |
| Solid-OIDC auth (DPoP-bound tokens)              | present | P2     | `oidc::tests::access_token_*` (x3) | `oidc::verify_access_token` + `verify_dpop_proof`, feature `oidc`. |
| OIDC Dynamic Client Registration (RFC 7591)      | present | P2     | `oidc::tests::dynamic_registration_returns_client_id` | `oidc::register_client`. |
| OIDC Discovery document                          | present | P2     | `oidc::tests::discovery_contains_standard_endpoints` | `oidc::discovery_for`. |
| Token introspection (RFC 7662)                   | present | P2     | `oidc::tests::introspection_*` (x2) | `oidc::IntrospectionResponse`. |
| WebID extraction (`webid` claim + url-shaped sub) | present | P2    | `oidc::tests::extract_webid_*` (x3) | `oidc::extract_webid`. |
| Dev-mode session bypass                          | present | P2     | `interop::tests::dev_session_stores_admin_flag`, `parity_close.rs::dev_session_default_is_not_admin`, `interop_jss.rs::jss_dev_session_carries_admin_flag` | `interop::dev_session` — constructed only through the typed helper, never from request headers. |
| WebID-TLS                                        | explicitly-deferred | — | — | Legacy; superseded by Solid-OIDC + DPoP. Deferred indefinitely (ADR-053 §"WebID-TLS deprecation"). |

## WebID

| Feature                                          | Status  | Target | Tests | Notes |
|--------------------------------------------------|---------|--------|-------|-------|
| WebID profile document generation (HTML+JSON-LD) | present | P1     | `webid::tests::*` (x5) | `webid::generate_webid_html`. |
| WebID profile validation                         | present | P1     | `webid::tests::validate_accepts_valid` | `webid::validate_webid_html`. |
| WebID-OIDC discovery (`solid:oidcIssuer`)        | present | P2     | `webid::tests::generate_with_issuer_embeds_oidc_triple`, `extract_oidc_issuer_*` (x2), `parity_close.rs::webid_with_issuer_round_trips_issuer`, `interop_jss.rs::jss_webid_includes_oidc_issuer_for_follow_your_nose` | `generate_webid_html_with_issuer` + `extract_oidc_issuer` (follow-your-nose). |

## Content negotiation

| Feature                                          | Status  | Target | Tests | Notes |
|--------------------------------------------------|---------|--------|-------|-------|
| Content-type passthrough                         | present | P1     | `storage_trait.rs` | |
| Turtle ⇄ JSON-LD conversion                      | present | P2     | `ldp::tests::render_container_*` (x3), `interop_jss.rs::jss_get_container_*` (x4) | `render_container_jsonld` / `render_container_turtle`. |
| N-Triples                                        | present | P2     | `ldp::tests::ntriples_roundtrip` | `Graph::to_ntriples` / `Graph::parse_ntriples`. |
| Accept header negotiation                        | present | P2     | `ldp::tests::negotiate_*` (x3) | `ldp::negotiate_format`, q-value aware. |
| RDF/XML                                          | explicitly-deferred | P2+ | — | Format negotiated by `RdfFormat::RdfXml`; full serialisation deferred to consumer crates (ADR-053 §"RDF format coverage"). Sophia/oxigraph drag-in cost not justified for the pod surface. |

## Metadata / Link headers

| Feature                                          | Status  | Target | Tests | Notes |
|--------------------------------------------------|---------|--------|-------|-------|
| `Link: <...>; rel="type"` for LDP types          | present | P1     | `ldp::tests::link_headers_*` (x3), `interop_jss.rs::jss_response_always_has_type_link` | `ldp::link_headers`. |
| `Link: <.acl>; rel="acl"`                        | present | P1     | `ldp::tests::link_headers_skip_acl_on_acl` | |
| `Link: <.meta>; rel="describedby"`               | present | P2     | `interop_jss.rs::jss_meta_sidecar_link_always_present_on_non_meta_resources` | Emitted for every non-meta, non-acl path. |
| `Link: rel="http://www.w3.org/ns/pim/space#storage"` | present | P2 | `ldp::tests::link_headers_root_exposes_pim_storage`, `interop_jss.rs::jss_pod_root_exposes_pim_storage_link` | Emitted for pod root `/`. |
| ETag header on read/write                        | present | P1     | `interop_jss.rs::jss_header_catalog_sanity` | SHA-256 hex. |
| If-Match / If-None-Match conditional requests    | present | P2     | `ldp::tests::preconditions_*` (x6), `parity_close.rs::if_*` (x3), `interop_jss.rs::jss_if_match_preconditions_block_concurrent_update` | `ldp::evaluate_preconditions` → `ConditionalOutcome::{Proceed,PreconditionFailed,NotModified}`. |
| Range requests (RFC 7233)                        | present | P2     | `ldp::tests::range_*` (x5), `parity_close.rs::range_*` (x3), `interop_jss.rs::jss_range_request_returns_slice` | `ldp::parse_range_header` + `slice_range`; supports `start-end`, `start-`, `-suffix` forms; multi-range intentionally rejected. |
| OPTIONS method (Allow/Accept-Post/Accept-Patch/Accept-Ranges) | present | P2 | `ldp::tests::options_*` (x2), `parity_close.rs::options_advertises_*`, `interop_jss.rs::jss_options_*` (x2) | `ldp::options_for` + `ACCEPT_PATCH` constant. |

## PATCH

| Feature                                          | Status  | Target | Tests | Notes |
|--------------------------------------------------|---------|--------|-------|-------|
| JSON Patch (RFC 6902)                            | present | P2     | `ldp::tests::json_patch_*` (x4), `parity_close.rs::json_patch_*` (x3), `interop_jss.rs::jss_json_patch_applies_over_pod_resource`, `jss_patch_dialect_includes_json_patch` | `ldp::apply_json_patch` — `add`/`remove`/`replace`/`test`/`copy`/`move` with JSON Pointer. |
| N3 PATCH (solid-protocol)                        | present | P2     | `ldp::tests::n3_patch_*` (x2) | `ldp::apply_n3_patch`; handles `inserts`/`deletes`/`where`. |
| SPARQL-Update PATCH                              | present | P2     | `ldp::tests::sparql_*` (x2) | `ldp::apply_sparql_patch` via `spargebra`. |

## Notifications

| Feature                                          | Status  | Target | Tests | Notes |
|--------------------------------------------------|---------|--------|-------|-------|
| Subscription trait + in-memory registry          | present | P1     | `notifications::tests::subscribe_unsubscribe_roundtrip` | `notifications::InMemoryNotifications`. |
| WebhookChannel2023 delivery                      | present | P2     | `notifications::tests::webhook_manager_default_retries` | `WebhookChannelManager`: AS2.0 POST, 3× exponential retry on 5xx, drop on 4xx. |
| WebSocketChannel2023 delivery                    | present | P2     | `notifications::tests::websocket_*` (x2) | `WebSocketChannelManager`: broadcast channel feeds per-connection WebSocket writers; 30s heartbeat. |
| Subscription discovery (.notifications)          | present | P2     | `notifications::tests::discovery_lists_both_channels` | `notifications::discovery_document`. |
| Retry + dead-letter                              | present | P2     | `notifications::tests::webhook_manager_default_retries` | Exponential backoff + fatal-drop tracking. |

## Storage backends

| Feature                                          | Status  | Target | Tests | Notes |
|--------------------------------------------------|---------|--------|-------|-------|
| Memory backend                                   | present | P1     | `storage_trait.rs` (Memory) | Includes broadcast watcher. |
| Filesystem backend                               | present | P1     | `storage_trait.rs` (FS) | `.meta.json` sidecars + `notify`-based watcher. |
| Quota enforcement                                | present | P2     | `provision::tests::quota_tracker_*` (x3), `parity_close.rs::quota_rejects_over_limit_writes`, `interop_jss.rs::jss_quota_reserves_and_releases_consistently` | `QuotaTracker` — atomic reserve/release, returns `PreconditionFailed` on overrun. |
| S3 backend                                       | explicitly-deferred | P2+ | — | Feature flag `s3-backend` + `aws-sdk-s3` dep declared; concrete impl lives in consumer crates (VisionClaw pod-worker owns object-store mapping). Rationale: avoid hard-coupling AWS SDK into the portable crate (ADR-053 §"Backend boundary"). |
| R2 / D1 / KV adapters                            | explicitly-deferred | P2+ | — | Consumer-crate concern — Cloudflare Workers bindings only make sense when `wrangler` is already in the tree (ADR-053 §"Backend boundary"). |
| RemoteStorage compatibility                      | explicitly-deferred | P2+ | — | Not on the Solid Protocol spec path; bridged via a separate adapter crate if we ever adopt it (ADR-053 §"Out of scope"). |

## Provisioning

| Feature                                          | Status  | Target | Tests | Notes |
|--------------------------------------------------|---------|--------|-------|-------|
| `.provision` endpoint creating seeded containers | present | P2     | `parity_close.rs::provision_pod_creates_webid_and_containers`, `interop_jss.rs::jss_provision_pod_seeds_profile_and_containers` | `provision::provision_pod` — idempotent container creation + WebID profile. |
| WebID + account scaffolding                      | present | P2     | same as above | Plan carries `pubkey`/`display_name`/`pod_base`; WebID rendered via `webid::generate_webid_html`. |
| Admin override                                   | present | P2     | `provision::tests::admin_override_matches_only_exact`, `parity_close.rs::admin_override_rejects_length_mismatch`, `interop_jss.rs::jss_admin_override_matches_constant_time` | `provision::check_admin_override` — constant-time shared-secret comparison. |

## Interop / discovery

| Feature                                          | Status  | Target | Tests | Notes |
|--------------------------------------------------|---------|--------|-------|-------|
| `.well-known/solid` discovery doc                | present | P2     | `interop::tests::well_known_solid_advertises_*`, `parity_close.rs::well_known_solid_embeds_storage_and_issuer`, `interop_jss.rs::jss_well_known_solid_exposes_storage_and_issuer` | `interop::well_known_solid` — Solid Protocol §4.1.2. |
| WebFinger integration                            | present | P2     | `interop::tests::webfinger_*` (x2), `parity_close.rs::webfinger_acct_lookup_returns_links`, `interop_jss.rs::jss_webfinger_acct_resolves_webid` | `interop::webfinger_response` — RFC 7033 JRD. |
| NIP-05 verification                              | present | P2     | `interop::tests::nip05_*` (x3), `parity_close.rs::nip05_verify_happy_path`, `interop_jss.rs::jss_nip05_lookup_binds_pubkey_to_name` | `interop::verify_nip05` — pubkey shape + `_` fallback. |

## Tests

| Feature                                          | Status  | Target | Tests | Notes |
|--------------------------------------------------|---------|--------|-------|-------|
| Storage trait conformance suite                  | present | P1     | `storage_trait.rs` (15 tests) | Memory + FS both pass. |
| WAC smoke tests                                  | present | P1     | `wac_basic.rs` (6 tests) | |
| ACL inheritance corpus                           | present | P2     | `wac_inheritance.rs` (31 tests) | Derived from WAC §5/§6. |
| JSS interop parity corpus                        | present | P2     | `interop_jss.rs` (42 tests) | Fixture-driven + feature scenarios. |
| Sprint 3 parity-close integration suite          | present | P2     | `parity_close.rs` (20 tests) | Exercises every row flipped in Sprint 3. |
| Schnorr feature test suite                       | present | P2     | `schnorr_nip98.rs` (2 tests) | Feature-gated under `nip98-schnorr`. |
| NIP-98 structural tests                          | present | P1     | `auth::nip98::tests` (inline, 8 tests) | |
| LDP container rendering                          | present | P1     | `ldp::tests` (inline, 32+ tests) | |
| LDP PATCH tests (N3 + SPARQL + JSON Patch)       | present | P2     | `ldp::tests` | Covers insert/delete/where, INSERT DATA, DELETE DATA, JSON Patch add/remove/replace/test/copy/move. |
| OIDC access-token + DPoP tests                   | present | P2     | `oidc::tests` (inline, 9 tests, feature-gated) | |
| Example server (actix-web)                       | present | P1     | `cargo run --example standalone` | |

## Phase 2 coverage by spec clause

| Test                                            | Validates                                |
|-------------------------------------------------|------------------------------------------|
| `link_headers_root_exposes_pim_storage`         | LDP 4.2.1.3 + pim:storage advertisement  |
| `link_headers_include_acl_and_describedby`      | Solid Protocol §4.1.1 (metadata discovery) |
| `prefer_minimal_container_parsed`               | LDP 4.2.2 / 7240 (Prefer)                |
| `prefer_contained_iris_parsed`                  | LDP 4.2.2                                |
| `jss_prefer_compose_include_minimal_and_contained_iris` | RFC 7240 multi-include            |
| `negotiate_prefers_explicit_turtle`             | Solid Protocol §3.1                      |
| `ntriples_roundtrip`                            | RDF 1.1 N-Triples                        |
| `server_managed_triples_include_ldp_contains`   | LDP 5.2.1.4                              |
| `find_illegal_server_managed_flags_ldp_contains`| LDP 5.2.3.1 (server-managed restriction) |
| `n3_patch_insert_and_delete`                    | Solid Protocol §8.2 (N3 Patch)           |
| `n3_patch_where_failure_returns_precondition`   | Solid Protocol §8.2 (precondition)       |
| `sparql_insert_data`, `sparql_delete_data`      | SPARQL 1.1 Update §3.1.1, §3.1.2         |
| `json_patch_add_and_replace`                    | RFC 6902 §4.1, §4.3                      |
| `json_patch_remove`                             | RFC 6902 §4.2                            |
| `json_patch_test_failure_returns_precondition`  | RFC 6902 §4.6                            |
| `json_patch_move_op_reshapes_document`          | RFC 6902 §4.4                            |
| `json_patch_copy_duplicates_value`              | RFC 6902 §4.5                            |
| `json_patch_array_append_with_dash`             | RFC 6902 §4.1 + JSON Pointer `-`          |
| `preconditions_if_match_*`                      | RFC 7232 §3.1                            |
| `preconditions_if_none_match_*`                 | RFC 7232 §3.2                            |
| `range_parses_start_end`                        | RFC 7233 §2.1                            |
| `range_parses_suffix`                           | RFC 7233 §2.1 (suffix-length)            |
| `range_rejects_unsatisfiable`                   | RFC 7233 §4.4                            |
| `options_container_includes_post_and_accept_post` | RFC 7231 §4.3.7                        |
| `options_resource_includes_put_patch_no_post`   | RFC 7231 §4.3.7                          |
| `websocket_manager_broadcasts_events`           | Notifications 0.2 §6 (WebSocketChannel)  |
| `discovery_lists_both_channels`                 | Notifications 0.2 §5                     |
| `change_notification_maps_event_types`          | Activity Streams 2.0                     |
| `access_token_binds_to_dpop_jkt`                | Solid-OIDC §5.2 (DPoP binding)           |
| `access_token_rejects_wrong_jkt`                | Solid-OIDC §5.2 (binding enforcement)    |
| `extract_webid_from_explicit_claim`             | Solid-OIDC §5.4                          |
| `extract_webid_falls_back_to_sub_when_url`      | Solid-OIDC §5.4 fallback                 |
| `dynamic_registration_returns_client_id`        | RFC 7591                                 |
| `discovery_contains_standard_endpoints`         | OpenID Connect Discovery 1.0             |
| `webid_with_issuer_round_trips_issuer`          | Solid-OIDC §4.1 (WebID → OIDC follow-your-nose) |
| `turtle_acl_round_trip_preserves_modes`         | WAC §3 (Turtle serialisation)            |
| `webfinger_acct_lookup_returns_links`           | RFC 7033                                 |
| `nip05_verify_happy_path`                       | NIP-05                                   |
| `well_known_solid_embeds_storage_and_issuer`    | Solid Protocol §4.1.2                    |
| `provision_pod_creates_webid_and_containers`    | ADR-053 §"Provisioning"                  |
| `quota_rejects_over_limit_writes`               | ADR-053 §"Quota"                         |
| `admin_override_rejects_length_mismatch`        | Constant-time admin-secret comparison    |
| `schnorr_verify_rejects_fake_signature`         | NIP-98 Schnorr (BIP-340)                 |

## Phase boundaries

Everything marked `P2` was in-scope for Sprint 2 + Sprint 3. Any
`missing` in a P1/P2 row is a regression. P2+ items are consumer-crate
concerns tracked for informational purposes — they remain
**explicitly-deferred** with an ADR pointer, never `missing`.

## Summary counts

### Sprint 3 close (2026-04-20)

- **67/67 rows present-or-deferred**
- **present**: 62 (was 48)
- **explicitly-deferred** (with ADR + rationale): 5 (was 0)
- **partial**: 0 (was 7)
- **missing**: 0 (was 12)

### Sprint 3 row-flip summary

Every partial or missing row is now resolved. Table below is the
authoritative change-log for this sprint:

| Row                                              | Sprint 2 | Sprint 3 | Landed where |
|--------------------------------------------------|----------|----------|--------------|
| Turtle-serialized ACL documents                  | partial  | present  | `wac::parse_turtle_acl`, `wac::serialize_turtle_acl`, resolver fallback |
| NIP-98 Schnorr signature verification            | partial  | present  | `auth::nip98::verify_schnorr_signature` (feature `nip98-schnorr`) |
| WebID-OIDC discovery (`solid:oidcIssuer`)        | partial  | present  | `webid::generate_webid_html_with_issuer`, `webid::extract_oidc_issuer` |
| RDF/XML                                          | partial  | deferred | Format remains negotiated; serialisation out of scope (ADR-053) |
| If-Match / If-None-Match conditional requests    | partial  | present  | `ldp::evaluate_preconditions` |
| S3 backend                                       | partial  | deferred | Feature flag + dep retained; impl in consumer crate (ADR-053) |
| `.well-known/solid` discovery doc                | partial  | present  | `interop::well_known_solid` |
| WebID-TLS                                        | missing  | deferred | Legacy, superseded by Solid-OIDC (ADR-053) |
| Dev-mode session bypass                          | missing  | present  | `interop::dev_session` |
| Range requests                                   | missing  | present  | `ldp::parse_range_header` + `slice_range` |
| JSON Patch (RFC 6902)                            | missing  | present  | `ldp::apply_json_patch`, `PatchDialect::JsonPatch` |
| R2 / D1 / KV adapters                            | missing  | deferred | Consumer-crate concern (ADR-053) |
| Quota enforcement                                | missing  | present  | `provision::QuotaTracker` |
| `.provision` endpoint                            | missing  | present  | `provision::provision_pod` |
| WebID + account scaffolding                      | missing  | present  | `provision::ProvisionPlan` + `provision_pod` |
| Admin override                                   | missing  | present  | `provision::check_admin_override` |
| WebFinger integration                            | missing  | present  | `interop::webfinger_response` |
| NIP-05 verification                              | missing  | present  | `interop::verify_nip05` |
| RemoteStorage compatibility                      | missing  | deferred | Not on Solid Protocol path (ADR-053) |
| OPTIONS / Accept-Patch / Accept-Ranges           | (new)    | present  | `ldp::options_for` + `ACCEPT_PATCH` |
| Multi-include Prefer composition                 | (new)    | present  | `PreferHeader::parse` tolerates space-separated includes |
| `.meta` sidecar auto-link                        | (new)    | present  | `link_headers` always emits `describedby` |
