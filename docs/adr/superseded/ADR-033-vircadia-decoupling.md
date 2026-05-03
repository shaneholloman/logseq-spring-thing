# ADR-033: Decouple Vircadia SDK from Quest 3 Auto-Detector

> **Status: Superseded by [ADR-071](../ADR-071-godot-rust-xr-replacement.md) and [PRD-008](../../PRD-008-xr-godot-replacement.md). Archived 2026-05-02.**
>
> The entire Vircadia SDK has been removed from VisionClaw. Multi-user XR
> presence is now provided by the Godot Rust substrate (`xr-client/`,
> `crates/visionclaw-xr-presence/`, `/ws/presence` route). Restore the original
> stack from the `feat/preserve-vircadia-stack` branch or the `pre-godot-xr`
> tag if the historical implementation is needed.

## Status

Superseded — see notice above.

## Context

`quest3AutoDetector.ts` (lines 227-256) imports Vircadia's `ClientCore` class and
hardcodes a WebSocket connection to `ws://localhost:3020/world/ws`:

```typescript
import { ClientCore } from '@vircadia/web-sdk';

const client = new ClientCore({
  serverUrl: 'ws://localhost:3020/world/ws',
  // ...
});
await client.connect();
```

This creates three problems:

1. **Silent failure**: If Vircadia is not running, `client.connect()` throws and
   the entire Quest 3 auto-start sequence fails. The error is caught but not
   surfaced to the user -- the XR session simply never starts.

2. **Responsibility violation**: `quest3AutoDetector` is a device detection module.
   Its job is to identify hardware capabilities and prepare session configuration.
   Establishing a multiplayer network connection is a separate concern.

3. **Test isolation**: Any unit test for quest3AutoDetector must either mock the
   entire Vircadia SDK or have a running Vircadia server. Currently neither is
   done, contributing to the 17% XR test coverage.

The Vircadia connection is needed for multiplayer presence in the shared 3D space,
but it is not needed for device detection, session entry, or local XR rendering.

## Decision Drivers

- **Standalone operation**: Quest 3 users should enter XR without a Vircadia server.
- **Testability**: Device detection must be testable without network dependencies.
- **Multiplayer support**: Vircadia integration must remain available when needed.
- **Migration cost**: Minimal changes to existing call sites.

## Considered Options

### Option 1: Dependency injection via XRNetworkAdapter interface

Define an `XRNetworkAdapter` interface:

```typescript
interface XRNetworkAdapter {
  connect(): Promise<void>;
  disconnect(): Promise<void>;
  isConnected(): boolean;
  onStateChange(cb: (connected: boolean) => void): void;
}
```

Provide two implementations:
- `VircadiaAdapter`: wraps `ClientCore` with the existing connection logic.
- `NullAdapter`: no-op implementation; `connect()` resolves immediately.

`quest3AutoDetector` receives an adapter via its constructor. Default is
`NullAdapter`. The application root passes `VircadiaAdapter` when multiplayer
is desired.

**Pros**:
- Clean separation of detection and networking.
- Tests inject `NullAdapter`; no mocking of SDK internals.
- Existing functionality preserved when `VircadiaAdapter` is provided.
- Additional adapters (e.g. for a different multiplayer backend) can be added
  without touching the detector.

**Cons**:
- Requires updating all call sites that construct `quest3AutoDetector`.
- Two new files (interface + VircadiaAdapter) to maintain.

### Option 2: Optional lazy initialization with feature flag

Keep the Vircadia import but gate it behind a feature flag and dynamic import:

```typescript
if (config.enableVircadia) {
  const { ClientCore } = await import('@vircadia/web-sdk');
  // connect...
}
```

**Pros**:
- Minimal code change (add a flag + dynamic import).
- No new interfaces or classes.

**Cons**:
- Detection module still knows about Vircadia's existence.
- Dynamic import complicates bundle splitting and error handling.
- Does not improve testability -- tests must still mock the flag and the import.
- Hardcoded URL remains in the detector module.

### Option 3: Event-based decoupling via EventEmitter

`quest3AutoDetector` emits a `deviceReady` event with capabilities. A separate
`VircadiaConnector` listens for this event and initiates its own connection.

**Pros**:
- Zero coupling between detector and network layer.
- Event-driven architecture aligns with the actor model used elsewhere.

**Cons**:
- Introduces implicit ordering dependency (connector must be registered before
  detector emits).
- Harder to guarantee connection is established before XR scene renders content
  that depends on multiplayer state.
- More complex to debug than explicit injection.

## Decision

**Option 1: Dependency injection via XRNetworkAdapter interface.**

Rationale: DI provides explicit, testable coupling with minimal ceremony. The
interface is small (4 methods), the two implementations are straightforward, and
the pattern is already used elsewhere in the codebase for service abstraction.

Event-based decoupling (Option 3) is more flexible but introduces timing
complexity that is not warranted for a single integration point. Lazy
initialization (Option 2) does not solve the testability problem.

## Implementation Plan

1. Create `src/xr/adapters/XRNetworkAdapter.ts` (interface definition).
2. Create `src/xr/adapters/VircadiaAdapter.ts` (extracts lines 227-256 from
   quest3AutoDetector.ts).
3. Create `src/xr/adapters/NullAdapter.ts` (no-op implementation).
4. Refactor `quest3AutoDetector.ts`:
   - Remove direct `ClientCore` import.
   - Accept `XRNetworkAdapter` as constructor parameter with `NullAdapter` default.
   - Call `adapter.connect()` after session features are configured.
5. Update application root to pass `VircadiaAdapter` when Vircadia is configured.
6. Add unit tests for quest3AutoDetector using `NullAdapter`.
7. Add integration test for `VircadiaAdapter` with a mock WebSocket server.

## Consequences

### Positive

- Quest 3 auto-start works without Vircadia server running.
- quest3AutoDetector is unit-testable with zero network dependencies.
- Vircadia connection logic is isolated in a single adapter file.
- New multiplayer backends require only a new adapter implementation.

### Negative

- All existing call sites for quest3AutoDetector must pass an adapter (or accept
  the NullAdapter default).
- VircadiaAdapter must be kept in sync with `@vircadia/web-sdk` API changes.

### Neutral

- Bundle size: VircadiaAdapter can be lazy-loaded, keeping the detection path
  free of the SDK until multiplayer is activated.
- The hardcoded `ws://localhost:3020/world/ws` moves to VircadiaAdapter's config,
  which should be externalised to environment configuration in a follow-up.

## Links

- [Vircadia Web SDK](https://github.com/vircadia/vircadia-web-sdk)
- PRD: `docs/prd-xr-modernization.md`
- Related: ADR-032 (RATK integration)
