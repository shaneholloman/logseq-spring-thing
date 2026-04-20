# Comparison vs JSS

JSS — the [Community Solid Server](https://github.com/CommunitySolidServer/CommunitySolidServer)
— is the reference TypeScript implementation of Solid. This page
compares solid-pod-rs to JSS across runtime, protocol support,
operations, and extensibility. It is meant to set expectations for
operators evaluating a migration and to help contributors understand
where the two implementations diverge.

See also [PARITY-CHECKLIST.md](../../PARITY-CHECKLIST.md) for a
feature-level status table.

## At a glance

|                                | JSS (CSS) v7         | solid-pod-rs 0.2.0-alpha |
|--------------------------------|----------------------|---------------------------|
| Language                       | TypeScript (Node 18+)| Rust 2021 / 1.74+         |
| Binary distribution            | `npm install -g`     | Build from source; drop-in crate |
| Memory footprint (idle)        | ~80 MB               | ~8 MB                     |
| Startup time                   | ~1 s                 | < 50 ms                   |
| HTTP framework                 | Koa (bundled)        | Agnostic; actix/axum/hyper|
| Dependency-injection config    | Components.js JSON   | Rust code                 |
| Lines of code (core)           | ~30k                 | ~4k                       |

## Protocol coverage

| Feature                                 | JSS       | solid-pod-rs |
|-----------------------------------------|-----------|---------------|
| LDP RDF + non-RDF GET/PUT/DELETE        | full      | full          |
| LDP container GET                       | full      | full          |
| LDP Slug→child via POST                 | full      | full          |
| `Prefer` header parsing                 | full      | partial (return=representation / include / omit) |
| Content negotiation (Turtle/JSON-LD/N-Triples/RDF-XML) | full | Turtle/JSON-LD/N-Triples full; RDF-XML partial (negotiated, serialisation deferred) |
| `Link`: type, acl, describedby, storage | full      | full          |
| Strong ETag (SHA-256)                   | optional  | always        |
| If-Match enforcement                    | full      | partial (storage returns ETag; HTTP enforcement P2) |
| Range requests                          | full      | not implemented |
| `WAC-Allow` header                      | full      | full          |
| WAC agent / agentClass / agentGroup     | full      | full          |
| `.acl` walk-up resolution               | full      | full          |
| JSON Patch (RFC 6902)                   | full      | **not supported** |
| N3 PATCH (solid-protocol)               | full      | full          |
| SPARQL-Update PATCH                     | full      | subset (INSERT DATA, DELETE DATA, DELETE/INSERT WHERE with ground templates) |
| Notifications 0.2 WebSocketChannel2023  | full      | full          |
| Notifications 0.2 WebhookChannel2023    | full      | full (3× retry, exp. backoff) |
| Solid-OIDC DPoP                         | full      | full (feature `oidc`)         |
| OIDC dynamic client registration        | full      | full          |
| OIDC discovery doc                      | full      | full          |
| Token introspection (RFC 7662)          | full      | full          |
| WebID extraction                        | full      | full          |
| WebID-TLS                               | full      | not supported (legacy) |
| NIP-98                                  | none      | full (structural) |
| `.provision` / account scaffold         | full      | not implemented (consumer concern) |
| `.well-known/solid`                     | full      | partial (OIDC discovery only) |
| WebFinger / NIP-05                      | none / none | not implemented |

## Defaults that differ (gotchas for JSS migrants)

### ACL posture

- **JSS default:** some distributions ship `allow-everything.json`
  for convenience. Production deployments add explicit ACL.
- **solid-pod-rs:** deny-by-default, always. You cannot read a pod
  without an `.acl` being in effect somewhere up the tree.

Mitigation: commit `/.acl` as the first write to any new pod. See
[tutorial 3](../tutorials/03-adding-access-control.md).

### Content-type storage

- **JSS:** stores the intent (e.g. `text/turtle`) and may transcode
  on the fly between RDF serialisations.
- **solid-pod-rs:** stores the body verbatim with its original
  `Content-Type`. No on-the-fly transcoding. If a client `PUT`s
  Turtle and another `GET`s with `Accept: application/ld+json`, we
  serve the stored Turtle with a 200 (client ignores Accept) —
  *unless* you wire up transcoding in your HTTP layer using the
  `Graph` primitives.

### PATCH dialect support

- **JSS:** supports JSON Patch, N3, and SPARQL Update.
- **solid-pod-rs:** N3 + SPARQL Update only. JSON Patch is
  deliberately out of scope (the original pod-worker had it; we
  dropped it to keep surface small and because Solid clients have
  largely converged on N3).

## Operational comparison

### Configuration

- **JSS:** Components.js DI container configured via JSON bundles.
  The `CSS_CONFIG` env var points at a bundle. You compose features
  by selecting or overriding JSON entries.
- **solid-pod-rs:** configured in Rust. You write a ~50-line `main.rs`
  that picks a storage backend, wires auth, and starts your HTTP
  framework. No DI container.

Pros and cons:

- Components.js gives you runtime configurability; you can change the
  storage backend without rebuilding. solid-pod-rs requires a rebuild
  to swap backends — but the build takes <2 s incremental and your
  whole server is a static binary.
- Rust configuration is type-checked. Misconfigurations show up at
  compile time, not on startup.

### Observability

- **JSS:** logs via `winston`. Structured logs supported.
- **solid-pod-rs:** `tracing`. Structured JSON logs + spans.

### Monitoring

- **JSS:** metrics via plugin; Prometheus exporter available.
- **solid-pod-rs:** library does not export metrics itself; you
  instrument at the HTTP framework layer.

### Backup

- **JSS:** filesystem backup of `~/.data/` works.
- **solid-pod-rs:** filesystem backup of `$POD_FS_ROOT` works; include
  the `.meta.json` sidecars.

## Extensibility

### Writing a custom storage backend

- **JSS:** implement `DataAccessor` and wire via Components.js.
- **solid-pod-rs:** implement the `Storage` trait (7 async methods).
  Pass `tests/storage_trait.rs` and you're done.

### Writing custom auth

- **JSS:** implement `Credentials` / `CredentialsExtractor`.
- **solid-pod-rs:** write middleware in your HTTP framework; call
  `auth::nip98::verify` or `oidc::verify_access_token` (or your own
  logic) and populate request-scoped state.

### Writing a notification backend

- **JSS:** implement `NotificationChannelType`.
- **solid-pod-rs:** implement `Notifications` trait (3 async methods)
  and feed it from `Storage::watch()`.

## What you give up moving to solid-pod-rs

- JSON Patch on resources.
- Quota enforcement.
- `.provision` endpoint.
- WebFinger / NIP-05 integration.
- Some Prefer-header nuances (handling=strict vs handling=lenient —
  we always parse leniently).

## What you gain moving to solid-pod-rs

- A single static binary, <20 MB.
- ~10× smaller memory footprint.
- Strong typing at every boundary (`AccessMode`, `PatchDialect`,
  `StorageEvent`, `RdfFormat`).
- First-class NIP-98 authentication (useful for Nostr ecosystems).
- Sub-ms startup time; viable as a serverless function.
- `cargo test` runs the whole conformance suite in a couple of
  seconds.
- Linear cost increase per resource (Rust + tokio scales further per
  core than V8).

## When to stay on JSS

- You rely on JSON Patch.
- You need `.provision` / account management out of the box.
- You have ops tooling built around Components.js bundles.
- You need WebID-TLS.

## When to pick solid-pod-rs

- You are building a NIP-98-authenticated app and want Solid on top.
- You need a tiny, embeddable pod for IoT / edge / serverless.
- You want to add solid-pod-rs as a library inside an existing Rust
  service (VisionClaw, federated forum, etc.).
- You need strict deny-by-default WAC.
- Your deployment demands static binaries (k8s, distroless, single-
  container deployments).

## See also

- [PARITY-CHECKLIST.md](../../PARITY-CHECKLIST.md)
- [how-to/migrate-from-jss.md](../how-to/migrate-from-jss.md)
- [explanation/architecture-decisions.md](architecture-decisions.md)
