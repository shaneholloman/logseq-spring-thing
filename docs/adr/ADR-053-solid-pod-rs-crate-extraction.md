# ADR-053: solid-pod-rs Crate Extraction

## Status

Ratified 2026-04-19

## Date

2026-04-19

## Related Documents

- ADR-048: Dual-Tier Identity Model — Bridging KG Notes and Ontology Classes
- ADR-028-ext: Optional Authentication (Solid OIDC optionality)
- ADR-052: (predecessor ADR in Wave 4 sequence)
- `src/handlers/solid_proxy_handler.rs` — current JSS proxy
- community-forum-rs: `crates/pod-worker/` (Cloudflare Worker pod implementation)
- Community Solid Server (JSS): https://github.com/CommunitySolidServer/CommunitySolidServer

## Context

VisionClaw currently proxies Pod requests through a Node.js JSS (JavaScript
Solid Server) via `src/handlers/solid_proxy_handler.rs`. The sibling
community-forum-rs project (at `/home/devuser/workspace/project2/community-forum-rs`)
has a Cloudflare-Worker-specific Rust Pod implementation in `crates/pod-worker/`
(44 KB, 13 modules, LDP + WAC + Solid Notifications).

Both projects need a Rust Solid Pod server. Keeping two Pod implementations
(Node JSS + CF pod-worker) is long-term tech debt — every spec change has to
land twice, ops has two runtimes to operate, and neither codebase gets
community hardening.

The user has a personal relationship with Melvin Carvalho (author of Community
Solid Server) and wants to frame a Rust port as a contribution to the Solid
ecosystem rather than competition.

## Decision

Extract pod-worker into a standalone **`solid-pod-rs`** crate, initially housed
as a VisionClaw workspace member at `crates/solid-pod-rs/`, then extracted to
its own GitHub repo post-parity.

### Phase 1 (weeks 1-3) — Extract & Seed

- Convert VisionClaw root to a Cargo workspace; add new member
  `crates/solid-pod-rs/`.
- Lift pod-worker source; strip Cloudflare-isms: `worker` crate, R2 SDK, D1
  calls, KV calls, Durable Objects.
- Introduce a `Storage` trait abstraction with backends:
  - `MemoryBackend` — tests
  - `FsBackend` — VisionClaw default
  - `S3Backend` — feature-gated
  - `R2Backend` — feature-gated for community-forum consumers
- Vendor latest JSS at `references/community-solid-server/` as a git submodule,
  read-only reference for porting.
- `PARITY-CHECKLIST.md` catalogues every JSS feature →
  `present | partial | missing` status.

### Phase 2 (weeks 3-6) — Port Gaps to Parity

Expected gaps (from pod-worker vs JSS):

- **Solid-OIDC flow** — pod-worker bypasses via NIP-98; port behind
  `OIDC_ENABLED` feature flag for ecosystem clients.
- **LDP conformance edge cases** — `Link` headers, `Prefer` headers,
  `Accept-Post`, `PATCH` via SPARQL-Update.
- **Content negotiation matrix** — Turtle, JSON-LD, N-Triples, RDF/XML.
- **Full Solid Notifications** — WebSocketChannel2023 + WebhookChannel2023.
- **ACL inheritance edge cases** — port JSS test corpus (~N tests).
- **`.meta` resources**.
- **Server-managed triples** — `dateModified`, `size`, `contains`, etc.

**Parity gate**: port JSS's own test corpus to Rust; must pass to exit Phase 2.

### Phase 3 (weeks 6-9) — Integrate into VisionClaw

- Embed `solid-pod-rs` as an Actix service in VisionClaw at
  `src/handlers/solid_pod_handler.rs`.
- Feature flag `SOLID_IMPL=jss|native` — both paths coexist during shadow testing.
- Shadow-test in staging for one week: parallel requests to both
  implementations; compare bytes + headers.
- Cutover: flip default to `native`; retain `jss` as rollback for one release.
- Week 10: remove JSS proxy from VisionClaw.

### Post-Parity (week 10) — Extract to Own GitHub Repo

```bash
git subtree split --prefix=crates/solid-pod-rs -b solid-pod-rs-history
# Create dreamlab-ai/solid-pod-rs repo
git push new-origin solid-pod-rs-history:main
# Publish 0.1.0 to crates.io
# VisionClaw Cargo.toml: path = "crates/solid-pod-rs" → version = "0.1.0"
# Delete crates/solid-pod-rs/ from VisionClaw
# community-forum-rs updates to depend on the same published crate
```

### Licensing

Dual **MIT OR Apache-2.0** (Rust ecosystem standard; no conflict with JSS's MIT).

### Melvin Engagement

The user owns this relationship. At the `0.1.0` milestone, open an issue on the
Community Solid Server repo offering the crate as a Rust reference port. Frame
as contribution to the Solid ecosystem, not competition. Invite Melvin as a
named advisor/reviewer on the new repo.

## Consequences

### Positive

- Single Rust Pod server across VisionClaw + community-forum-rs (unified ops,
  one upstream).
- Eliminates Node.js runtime from the VisionClaw stack (single-language ops).
- OSS reference implementation for the Rust Solid ecosystem.
- community-forum-rs gets the R2 backend as a thin adapter over the shared crate.
- Feature flag cutover allows safe rollback.

### Negative

- ~1 month of extraction + porting + parity work (scoped to parallel Sprint B).
- We own Solid spec tracking forever (previously delegated to JSS).
- New crate has no fuzzing / production hardening; staging shadow testing is
  critical.

## Non-Goals (v0.1.0)

- Multi-backend orchestration (one backend per deployment is enough).
- IPFS-based storage (YAGNI; can add later as another backend).
- Solid-OIDC as primary auth for VisionClaw (we use NIP-98 per ADR-028-ext;
  OIDC is feature-flagged for ecosystem interop).
- ActivityPub bridge (future ADR, after 0.1.0).

## Compliance Criteria

- [ ] `crates/solid-pod-rs/` scaffolded as VisionClaw workspace member
- [ ] `Storage` trait with 2+ MVP backends (Memory, FS); Phase 1 exit gate
- [ ] `references/community-solid-server/` vendored; `PARITY-CHECKLIST.md` exists
- [ ] JSS test corpus ported; all green; Phase 2 exit gate
- [ ] Shadow-testing harness in staging; Phase 3 operational
- [ ] `SOLID_IMPL=native` default on; JSS removed by week 10
- [ ] Crate extracted to own GitHub repo; 0.1.0 on crates.io
- [ ] community-forum-rs migrated to depend on published crate
- [ ] Melvin notified per user's preferred channel

## Rollback

- `SOLID_IMPL=jss` preserves Node proxy behaviour across any phase up to week 10.
- Post-extraction: pin crate version in `Cargo.toml` for stability; breaking
  changes gated by 0.2.0.
