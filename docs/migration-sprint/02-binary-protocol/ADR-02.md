# ADR-02 — Binary Protocol & Broadcast

Status   : Proposed
Date     : 2026-05-16
Related  : ADR-01 (Physics events), ADR-03 (Client State), ADR-06 (Auth)

## Context

Position broadcasting was the largest contributor to the client freeze
regression and the most-churned subsystem between baseline and main. The
churn includes V4 delta protocol attempts, hand-tuned broadcast rates,
backpressure half-measures, delta-filter starvation bugs, and
client-side frame coalescer drain-loop starvation. The capability is
simple — get positions from the GPU to the canvas — but the implementation
has grown a high accidental complexity surface.

## Decision

### D1. V3 full-sync, single wire format

The protocol carries one frame type. Header (`magic = 0xV3F0`, `frame_id`),
per-node payload (28 bytes), trailer (`node_count`). V4 delta encoding,
attempted in `8a7610a00`, is rejected. Reasons:

- Deltas require client to maintain authoritative state matching the
  server, which it cannot guarantee under reconnect / packet drop.
- Full-state at 5k nodes is 140KB / frame; well within bandwidth at
  realistic cadences.
- Delta logic introduced two regressions (client silently rejected V4,
  then BroadcastOptimizer starved converged clients).

### D2. Settlement-gated cadence (no fixed FPS)

The broadcast actor subscribes to physics domain events:

- On `LayoutDestabilised`: enter ACTIVE state, broadcast up to 10 Hz.
- On `LayoutSettled`: enter SETTLED state, broadcast on `LayoutHeartbeat`
  only (every 5s by default).
- On `PhysicsClamped`: log only; no protocol change.

There is no `broadcast_interval_ms` knob. Cadence is event-driven.

### D3. Backpressure: drop, never queue

For each connected client, the broadcast actor inspects the per-client
WebSocket send buffer fill. If `buffered_amount > 64 * 1024`, the current
frame is dropped *for that client*. Frame dropping is silent (counter
metric only). No queueing. No retry. The next broadcast tick handles the
next attempt.

This is a hard policy: clients that fall behind will receive less data
until they catch up. They are guaranteed to receive a full frame within
the next heartbeat regardless of their lag.

### D4. Single source-of-truth: `GraphStateActor`

The WebSocket broadcast and the REST `GET /graph/positions` endpoint both
read from `GraphStateActor::current_snapshot()`. They do not synthesize
positions; they cannot read from the GPU directly; they cannot disagree.
The physics actor periodically pushes its current positions into
`GraphStateActor` via `UpdateNodePositions` messages. This is the only
write path for the snapshot.

This eliminates the polling-vs-broadcast divergence reported in prior
freeze investigations (BroadcastOptimizer filtering nodes from broadcast
while polling returned stale positions).

### D5. BroadcastOptimizer eliminated

The optimiser layer that filtered "nodes that haven't moved" is removed.
It existed to reduce bandwidth; under D1 + D2 the bandwidth concern is
gone (heartbeat at 0.2Hz on the SETTLED state is the bandwidth floor).
The optimiser's delta-filter was the proximate cause of the freeze.

### D6. No reactor / coalescer pipelines on the client

The client-side `BinaryFrameCoalescer` (`348d23c62`,
`17c0f913a`) is removed. With D2's bounded cadence (max 10 Hz), there are
not enough frames per second to justify coalescing. The client processes
each frame as it arrives, single-flight (see ADR-03 D2).

### D7. Frame ID for client-side drop detection

`frame_id` in the header is monotonic per connection. The client can
detect lost / dropped frames (`current_frame_id - last_frame_id > 1`)
and surface a metric. Lost frames are not retransmitted; the next frame
is full state anyway. The metric is for observability, not correction.

### D8. Auth model

WebSocket upgrade handshake requires a `?token=<nostr_jwt>` query param in
production. In dev mode (`?skipAuth=true` request to the HTML shell), the
client emits no token; the server, if running with `--allow-skip-auth`,
accepts. There is no third mode. The `--allow-skip-auth` flag is gated by
`build = debug || env(VISIONFLOW_DEV_MODE)`. Release builds reject it
outright. (Section 6 owns the broader auth posture.)

## Options considered

### O1. Bring V3 + V4 + BroadcastOptimizer + delta-filter forward

Rejected. This is the state that produced the freeze.

### O2. Move to a streaming protocol (gRPC server-streaming, WebTransport)

Rejected for this sprint. WebSocket works; the bandwidth at 5k nodes is
not the bottleneck. WebTransport would be a separate ADR with its own
client and infra implications.

### O3. V3 full-sync + settlement gate + drop-on-pressure (this ADR)

Adopted. Eliminates V4 complexity, BroadcastOptimizer surface, and the
delta-filter starvation class.

## Risks

- **R1**: 10Hz at 5k nodes is 1.4 MB/s. Adequate for LAN; may stress mobile
  cellular. Mitigation: tier-down to 5Hz max on connections whose
  measured RTT exceeds 100ms (deferred to a follow-up ADR; not blocking).
- **R2**: Drop-on-pressure means a slow client sees lag. Acceptable per A4
  (next heartbeat guaranteed). Make the drop metric visible in the
  client status overlay so users can diagnose.
- **R3**: Removing the coalescer might surprise a future load test that
  pumps frames faster than D2 allows. Mitigation: D2 enforced server-side,
  so this can't happen unless someone bypasses the broadcast actor.

## Rejected from main as buggy / unjustified

- `c09f8725a fix: eliminate client freeze on 4.5k-node graphs via comms-rate
  fix` — symptom-level fix at 1 FPS. Replaced by D2 settlement gating.
- `4c126cffc fix: eliminate non-SAB double Comlink round-trip in
  processBinaryData` — client-side workaround; addressed cleanly by
  ADR-03's zero-copy contract.
- `348d23c62 fix: break drain loop event loop starvation in binary frame
  coalescer` — coalescer eliminated entirely (D6).
- `695be6d6d fix: guard map-clearing with graphDataLoaded flag to prevent
  binary frame race` — addressed by ADR-03 single-flight discipline.

## Bugs and smells at the reset point (41979d33e)

- The baseline has only V3, no V4. Good. Migration is "keep V3, refine
  semantics", not "remove V4".
- BroadcastOptimizer exists at baseline as `src/utils/broadcast_optimizer.rs`
  or similar (verify location during implementation). At baseline its
  delta-filter is naïve; removing it is straightforward.
- The baseline uses `client_coordinator_actor.rs` with `broadcast_interval`
  fields. These become vestigial under D2 — remove rather than re-tune.
