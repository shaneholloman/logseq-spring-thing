# scene-effects WASM Module

This directory contains the compiled WebAssembly output for the VisionClaw background scene effects. It is **build output** — do not edit files here directly. The Rust source lives in a `crates/scene-effects/` directory at the project root (not present in this repository; the module ships pre-built).

## Purpose

Three real-time simulation workloads that render behind the 3D knowledge graph:

| Class | What it simulates | Key outputs |
|-------|-------------------|-------------|
| `ParticleField` | Drifting ambient particles responding to camera motion | positions (Float32Array, interleaved xyz), opacities (Float32Array), sizes (Float32Array) |
| `AtmosphereField` | Scrolling noise-based atmosphere texture | pixels (Uint8Array, RGBA), width/height |
| `EnergyWisps` | Ephemeral glowing orbs with lifecycle fade and drift | positions, opacities, sizes, hues (all Float32Array) |

## Zero-Copy Memory Pattern

All output arrays are **views over `WebAssembly.Memory`** — no serialisation, no copies. Each getter (`getPositions()`, `getOpacities()`, etc.) calls the WASM module's pointer+length exports and wraps them in a typed array backed by the same `ArrayBuffer`:

```typescript
// Internally (from scene-effects-bridge.ts):
new Float32Array(this.memory.buffer, ptr, len)
```

Views are reconstructed on each access to handle potential WASM memory growth. Consumers must not hold references across frames.

`AtmosphereField.getPixels()` returns a `Uint8Array` (RGBA bytes). All other getters return `Float32Array`.

## Initialisation — State Machine

`initSceneEffects()` manages a module-level singleton through four states:

```
idle → loading → ready
              ↘ failed → idle  (after 1-second retry backoff)
```

- Safe to call multiple times; returns the cached `SceneEffectsAPI` after first success.
- On failure, the rejected promise is cached for 1 second so concurrent callers receive the same rejection rather than stampeding retries.
- After 1 second the state resets to `idle` and the next caller triggers a fresh attempt.

## Graceful Degradation

The bridge never throws into the render loop. Callers wrap `initSceneEffects()` in a `try/catch` or `.catch()` and continue with non-WASM rendering if the module is unavailable. After `dispose()` all getters return empty zero-length typed arrays instead of throwing.

## Dispose Pattern

Each bridge object (`ParticleFieldBridge`, `AtmosphereFieldBridge`, `WispFieldBridge`) exposes:

```typescript
bridge.isDisposed  // boolean guard
bridge.dispose()   // idempotent; calls WASM free(), sets isDisposed = true
```

Call `dispose()` when the component unmounts to release WASM heap memory.

## Usage

```typescript
import { initSceneEffects } from '../wasm/scene-effects-bridge';

const wasm = await initSceneEffects();                  // singleton init
const particles = wasm.createParticleField(256);        // up to 512
particles.update(0.016, camera.x, camera.y, camera.z); // call each frame
const positions = particles.getPositions();             // Float32Array [x,y,z,...]
// upload positions to GPU, then on unmount:
particles.dispose();
```

For atmosphere texture: `wasm.createAtmosphereField(width, height)` — call `update(dt)` each frame and upload `getPixels()` as an RGBA texture.

For wisps: `wasm.createWispField(count)` — also exposes `getHues()` (Float32Array, 0..1 hue offsets) and `setDriftSpeed(speed)`.

## Building

```bash
wasm-pack build --target web \
  --out-dir ../../client/src/wasm/scene-effects/ \
  --release
```

Run from the `crates/scene-effects/` source directory. The `--release` flag is required; debug builds are not consumed by the frontend.

**Note**: `wasm-opt` is disabled in the build configuration (binaryen validation workaround). The `.wasm` file is not post-processed by `wasm-opt` even in release mode.
