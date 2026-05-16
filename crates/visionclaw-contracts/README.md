# visionclaw-contracts

Cross-boundary typed contracts for VisionFlow ⇄ agentbox ⇄ forum ⇄ XR client.

Single source of truth for every envelope that crosses a process boundary in
the VisionFlow ecosystem. Rust consumers depend on this crate directly;
JavaScript / TypeScript consumers depend on `@visionflow/contracts` (in
`sdk/visionflow-contracts/`), whose `.d.ts` files are generated from the
types here via the `typescript-export` feature.

## Contracts

| Module             | Envelope                       | Canonical spec               |
|--------------------|--------------------------------|------------------------------|
| `agent_action`     | `AgentActionEnvelope`          | ADR-10 §D3 + T7 resolution   |
| `telemetry`        | `AgentTelemetryEnvelope`       | ADR-10 §D1 (CC-1 ACL added)  |
| `enterprise`       | `EnterpriseEventEnvelope`      | ADR-10 §D5                   |
| `github_adapter`   | `ParsedMarkdown` value object  | ADR-10 §D11 + DDD-08         |
| `version`          | `SCHEMA_VERSION` constants     | ADR-10 §D8                   |

## Building

Default build (no TypeScript export):

```bash
cargo build --manifest-path crates/visionclaw-contracts/Cargo.toml
cargo test  --manifest-path crates/visionclaw-contracts/Cargo.toml
```

With TypeScript export:

```bash
cargo test --manifest-path crates/visionclaw-contracts/Cargo.toml \
  --features typescript-export ts_export
```

Generated bindings land in `crates/visionclaw-contracts/bindings/` and are
mirrored into `sdk/visionflow-contracts/bindings/` for the npm publish.

## Versioning

All envelopes carry `schema_version`. Bump rules (ADR-10 §D8):

- Backwards-compatible field addition → stay at v1.
- Renamed / removed field, changed enum variant, changed transport
  semantics → bump to v2; coordinated deploys required.
- Both sides keep one back-version of support.

Refresh snapshot fixtures after a ratified bump:

```bash
INSTA_UPDATE=always cargo test --manifest-path \
  crates/visionclaw-contracts/Cargo.toml --test schema_stability
```

## Consuming

### Rust

```toml
[dependencies]
visionclaw-contracts = { path = "crates/visionclaw-contracts" }
```

```rust
use visionclaw_contracts::{AgentAction, ActionKind, NodeClass, SCHEMA_VERSION};

let envelope = AgentAction::new(
    uuid::Uuid::new_v4().to_string(),
    chrono::Utc::now().timestamp_millis(),
    user_npub,
    agent_id,
    NodeClass::Agent,
    ActionKind::OpenPanel,
).into_envelope();
let wire_json = serde_json::to_string(&envelope)?;
```

### TypeScript

```ts
import {
  AgentActionEnvelope,
  AGENT_ACTION_CHANNEL,
  AGENT_ACTION_TYPE,
  SCHEMA_VERSION,
} from "@visionflow/contracts";

const channel = new BroadcastChannel(AGENT_ACTION_CHANNEL);
channel.onmessage = (e: MessageEvent<AgentActionEnvelope>) => {
  if (e.data.type !== AGENT_ACTION_TYPE) return;
  if (e.data.schema_version !== SCHEMA_VERSION) return;
  // dispatch on e.data.kind
};
```

## Licence

AGPL-3.0-only.
