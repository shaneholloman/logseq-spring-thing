# How to migrate from JSS (Community Solid Server)

**Goal:** move a production pod from the TypeScript
[Community Solid Server (CSS, "JSS")](https://github.com/CommunitySolidServer/CommunitySolidServer)
to a solid-pod-rs-backed server, with no data loss and a tested
rollback.

This guide is written for operators who already run CSS and want to
replace the runtime while keeping their pod data intact.

## TL;DR

1. Freeze writes to the JSS pod.
2. Copy `~/.data/<pod>/` to your new pod's storage root.
3. Rewrite `.acl` paths if the layout differs (it usually doesn't).
4. Stand up solid-pod-rs pointing at the same directory.
5. Cut the DNS / load balancer over.
6. Keep JSS running read-only for 24 h as rollback.

## 1. Semantic differences you must know

| Concern | JSS default | solid-pod-rs |
|---|---|---|
| WAC default | public-read on new pods (`config/util/auth/authorizers/allow-everything.json`) | **deny-by-default**; no ACL = no access |
| Authentication | Solid-OIDC is primary; WebID-TLS supported | NIP-98 is first-class; Solid-OIDC behind the `oidc` feature |
| Storage layout | `.internal/` + `<pod>/` under a root directory | `<root>/<path>` mirroring the pod URI directly |
| Quota | `config/storage/backend/quota/` | not implemented (P2 item) |
| Provisioning endpoint | `config/setup/main/entry/register.json` | not implemented (consumer-crate concern) |
| HTTP framework | Koa-based | framework-agnostic; examples use actix-web |
| JSON Patch (RFC 6902) | supported | not supported (by design); use N3 Patch or SPARQL-Update |

The most important difference is the WAC default. **Do not start
solid-pod-rs in production without first installing a `/.acl`
document**, or every request will 401. See
[tutorial 3](../tutorials/03-adding-access-control.md).

## 2. Configuration mapping

### Environment variables

| JSS env var                              | solid-pod-rs equivalent             | Notes |
|------------------------------------------|-------------------------------------|-------|
| `CSS_PORT`                               | bind address in your HTTP wiring    | solid-pod-rs is framework-agnostic; configure your actix/axum server directly |
| `CSS_BASE_URL`                           | passed to `discovery_for(issuer)` + `ChangeNotification::from_storage_event(_, pod_base)` | — |
| `CSS_ROOT_FILE_PATH`                     | `FsBackend::new(path)` argument     | — |
| `CSS_CONFIG`                             | — (no monolithic config file)       | assemble your pod in Rust |
| `CSS_LOG_LEVEL`                          | `RUST_LOG=solid_pod_rs=info`        | uses `tracing` |
| `CSS_ALLOW_ROOT_ACL`                     | always-on; install `/.acl` manually | — |

### Config-file mapping

JSS is configured via Components.js JSON bundles. solid-pod-rs is
configured in Rust code. The rough equivalence:

| JSS config identity                                  | solid-pod-rs equivalent                                   |
|------------------------------------------------------|-----------------------------------------------------------|
| `urn:solid-server:default:FileDataAccessor`          | `storage::fs::FsBackend`                                  |
| `urn:solid-server:default:MemoryDataAccessor`        | `storage::memory::MemoryBackend`                          |
| `urn:solid-server:default:WebAclReader`              | `wac::StorageAclResolver` + `wac::evaluate_access`        |
| `urn:solid-server:default:LinkTypeParser`            | `ldp::link_headers`                                       |
| `urn:solid-server:default:PreferenceSupport`         | `ldp::PreferHeader::parse`                                |
| `urn:solid-server:default:NotificationsSubscription` | `notifications::WebSocketChannelManager` + `WebhookChannelManager` |
| `urn:solid-server:default:ClientCredentialsHandler`  | `oidc::register_client`                                   |
| `urn:solid-server:default:DPoPValidator`             | `oidc::verify_dpop_proof`                                 |

If a mapping is missing, it is either in the [parity
checklist](../../PARITY-CHECKLIST.md) as a P2 item or deliberately out
of scope (consumer-crate concern).

## 3. Data migration

### Layout translation

JSS data layout (with the default `file` accessor):

```
/var/lib/css/
├── .internal/          (server metadata — discard)
├── <pod-name>/
│   ├── profile/card$.ttl
│   ├── profile/card.acl$.ttl
│   └── ...
└── ...
```

solid-pod-rs layout:

```
/var/lib/mypod/
├── profile/
│   ├── card                  (body)
│   ├── card.meta.json        ({"content_type":"text/turtle","links":[]})
│   ├── card.acl              (body)
│   └── card.acl.meta.json
└── ...
```

### Migration script (rsync + meta-sidecar generation)

```bash
#!/usr/bin/env bash
set -euo pipefail
SRC=/var/lib/css/mypod
DST=/var/lib/mypod

# 1. Copy bodies, dropping JSS's `$.ttl` / `$.json` suffixes.
rsync -av --exclude='.internal' "$SRC/" "$DST.tmp/"

# 2. For each `foo$.ttl` file, rename to `foo` and write meta-sidecar.
find "$DST.tmp" -type f | while read -r f; do
  case "$f" in
    *\$.ttl)  target="${f%\$.ttl}"; ct="text/turtle";;
    *\$.jsonld) target="${f%\$.jsonld}"; ct="application/ld+json";;
    *\$.nt)   target="${f%\$.nt}"; ct="application/n-triples";;
    *)        continue;;
  esac
  mv "$f" "$target"
  printf '{"content_type":"%s","links":[]}\n' "$ct" > "$target.meta.json"
done

mv "$DST.tmp" "$DST"
```

Verify counts:

```bash
find /var/lib/css/mypod -type f ! -path '*/.internal/*' | wc -l
find /var/lib/mypod     -type f ! -name '*.meta.json' | wc -l
# Numbers should match.
```

### ACL translation

JSS stores ACL as Turtle; solid-pod-rs reads JSON-LD ACLs. Both are
semantically equivalent; convert with any tool (rdf-pipe, riot,
rdflib). Minimum worked example — CSS ACL:

```turtle
@prefix acl: <http://www.w3.org/ns/auth/acl#> .
@prefix foaf: <http://xmlns.com/foaf/0.1/> .

<#owner>
  a acl:Authorization ;
  acl:agent <https://me.example/profile/card#me> ;
  acl:accessTo <./> ;
  acl:default <./> ;
  acl:mode acl:Read, acl:Write, acl:Control .
```

...becomes the JSON-LD equivalent:

```json
{
  "@context": {
    "acl": "http://www.w3.org/ns/auth/acl#"
  },
  "@graph": [{
    "@id": "#owner",
    "@type": "acl:Authorization",
    "acl:agent":    { "@id": "https://me.example/profile/card#me" },
    "acl:accessTo": { "@id": "./" },
    "acl:default":  { "@id": "./" },
    "acl:mode":     [
      { "@id": "acl:Read" },
      { "@id": "acl:Write" },
      { "@id": "acl:Control" }
    ]
  }]
}
```

Store at the same filename the JSS server used (e.g. `/profile/.acl`).

## 4. Standing up solid-pod-rs

Minimal production wire-up:

```rust
use std::sync::Arc;
use solid_pod_rs::storage::{fs::FsBackend, Storage};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let storage: Arc<dyn Storage> =
        Arc::new(FsBackend::new("/var/lib/mypod").await?);
    // Wire your HTTP framework's handlers against `storage`.
    // See examples/standalone.rs for actix-web.
    Ok(())
}
```

Do not forget:

- Install `/.acl` before flipping traffic (deny-by-default).
- Register a NIP-98 verifier if your JSS was OIDC-only — see
  [how-to/configure-nip98-auth.md](configure-nip98-auth.md). Or turn
  on the `oidc` feature to continue accepting Solid-OIDC tokens.
- Expose `.well-known/openid-configuration` if you are keeping OIDC
  clients.

## 5. Cutover

1. Drop write permissions on the JSS pod (make the data directory
   read-only, or throw 503 at the load balancer for mutating methods).
2. Run the migration script.
3. Start solid-pod-rs pointing at the new directory.
4. Smoke test:
   ```bash
   curl -sI https://pod.example/profile/card | grep -iE '^(link|wac-allow):'
   ```
5. Cut DNS / the load balancer to solid-pod-rs.

## 6. Rollback strategy

Keep JSS running **read-only** for 24 h on a different port. If
anything breaks:

1. Stop solid-pod-rs.
2. Point the load balancer back at JSS.
3. Writes made to solid-pod-rs during the cutover window need to be
   re-applied to JSS (rsync in reverse, with the sidecar-strip logic
   inverted).

Because both servers address the same filesystem layout format once
translated, forward and backward migration are symmetric.

## 7. What you lose

- **JSON Patch** — use N3 Patch or SPARQL-Update instead. See
  [reference/patch-semantics.md](../reference/patch-semantics.md).
- **Quota enforcement** — a P2 item; track storage yourself if you
  need limits.
- **`.provision` endpoint** — build your own at the HTTP layer.
- **WebID-TLS** — legacy; deliberately not ported.

## 8. What you gain

- Single static binary, no Node runtime, no Components.js.
- `cargo test` against the storage trait passes on memory + FS
  today, so regression tests in CI are cheap.
- Strong typing at every API boundary — `AccessMode`, `RdfFormat`,
  `PatchDialect`, `StorageEvent`.
- Straight path to S3, R2, IPFS backends via the `Storage` trait.

## See also

- [explanation/comparison-vs-jss.md](../explanation/comparison-vs-jss.md)
- [PARITY-CHECKLIST.md](../../PARITY-CHECKLIST.md)
- [reference/env-vars.md](../reference/env-vars.md)
