# solid-pod-rs

Rust implementation of a Solid Pod server: WAC (Web Access Control),
LDP (Linked Data Platform) resources and containers, NIP-98 HTTP
authentication, WebID profile documents, and Solid Notifications.

## Status

Phase 1: workspace scaffolding, FS and Memory storage backends, WAC
evaluator, LDP container/resource logic, NIP-98 verifier (structural),
WebID document generator. See `PARITY-CHECKLIST.md` for feature status
against the JavaScript Solid Server reference.

## Features

| Feature | Default |
|---------|---------|
| `fs-backend` | yes |
| `memory-backend` | yes |
| `s3-backend` | no (requires aws-sdk-s3) |

## Architecture

The crate exposes a `Storage` trait and pluggable backends. The backend
is orthogonal to the Solid protocol layer: WAC, LDP, WebID, and
Notifications are implemented against the trait.

```text
  HTTP layer (actix-web, axum, hyper — caller's choice)
        |
        v
  solid-pod-rs
    auth::nip98   ldp::*   wac::*   webid::*
    notifications::* (Phase 2)
        |
        v
  storage::Storage trait
    - storage::memory::MemoryBackend
    - storage::fs::FsBackend
    - (s3-backend, future: r2, ipfs)
```

## Quick Start

```bash
cargo build --release -p solid-pod-rs
cargo test -p solid-pod-rs
cargo run --example standalone -p solid-pod-rs
```

## Attribution

Extracted from `community-forum-rs/crates/pod-worker`. See NOTICE for
full provenance.

## Licence

**AGPL-3.0-only** — inherited from the JavaScriptSolidServer ecosystem
covenant.

This means: if you operate solid-pod-rs as a network-accessible service,
AGPL §13 requires you to make the corresponding source code available to
your users under AGPL-3.0 or later. See `LICENSE` and `NOTICE` for full
terms and provenance.

If AGPL-3.0 is incompatible with your project's licence strategy, consider:
- Contributing upstream rather than hard-forking
- Using the crate in a sidecar architecture where AGPL obligations are
  contained to the sidecar process
- Running JSS itself (same licence; different language)

We welcome issues + PRs asking about specific compatibility scenarios.
