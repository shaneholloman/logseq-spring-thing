# drawer-fx

Ambient flow-field caustic background for the enterprise sliding drawer.
Compiled to WebAssembly via `wasm-pack`; consumed by
`client/src/features/enterprise/fx/drawerFx.ts`.

## What it does

A value-noise flow field advects ~100-400 luminous particles across a
Canvas2D element. Each frame draws a translucent dark rectangle instead
of clearing, producing short decaying motion trails. Additive blending
plus violet/cyan hues gives the "caustic light dancing on frosted glass"
feel. A cursor `pulse()` creates a brief attractor that perturbs nearby
particles for ~0.8 s.

## How to build

```bash
# one-time prerequisites
rustup target add wasm32-unknown-unknown
curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh

# from client/
npm run wasm:drawer-fx
```

This runs `src/wasm/drawer-fx/build.sh`, which emits JS glue + `.wasm`
into `src/features/enterprise/fx/wasm/`. That folder is gitignored;
commit the Rust source under `src/wasm/drawer-fx/`.

## Quality / CPU

Measured in Chromium 120 on a modest laptop (Intel i5-1135G7, iGPU) at
1440x900 drawer dimensions, idle aside from the effect. Numbers are
approximate -- treat them as orders of magnitude, not guarantees.

| Quality | Particles | Frame time | Main-thread CPU | When to use |
|---------|-----------|------------|-----------------|-------------|
| 0 (low) | ~80       | 0.4-0.7 ms | ~2-3 %          | Low-end, battery, 4K panels |
| 1 (med) | ~180      | 0.8-1.3 ms | ~3-5 %          | **default** |
| 2 (high)| ~360      | 1.6-2.4 ms | ~6-9 %          | Desktops, showcase views |

At default quality the effect sits well under the 5% idle budget on the
main thread. Canvas2D + `lighter` compositing is GPU-accelerated in
Chrome/Safari/Firefox, so paint cost shows up in the compositor, not
here.

## Known limitations

- **Canvas2D, not WebGPU.** Deliberate: init cost is zero and the
  number of primitives is small. If the particle count ever needs to
  10x, port to OffscreenCanvas + WebGL point-sprites.
- **No per-particle radial gradient.** Each particle is a flat hue arc
  blended additively; `createRadialGradient` per particle would burn
  ~0.01 ms/particle and dominate the frame.
- **Single noise octave (+half-amplitude second).** Enough structure
  for motion that reads as "fluid"; adding more octaves degrades Q2
  noticeably for little visual gain.
- **Trail fade uses full-canvas `fillRect`.** This is the single most
  expensive op; it scales with canvas size, not particle count. Very
  large canvases (>2K px) will hit the compositor harder than CPU.
- **No SharedArrayBuffer.** Not needed; no worker offload; no COOP/COEP
  constraints imposed on the host page.

## API surface

```ts
new DrawerFx(canvasId: string, width: number, height: number)
  .tick(dt_ms: number)
  .resize(w: number, h: number)
  .pulse(x: number, y: number)
  .set_quality(q: 0 | 1 | 2)
  .free()
```

The TypeScript wrapper `drawerFx.ts` is the intended consumer.
