# ADR-047: WASM-Powered Visualization Components

## Status

Proposed

## Date

2026-04-14

## Context

The enterprise UI surfaces (PRD-002, ADR-046) include three visualization components that benefit from GPU-interpolated or compute-intensive rendering:

1. **KPI Sparklines**: 30-1000 point time series with confidence bands, rendered inline in KPI cards and drill-down views. Must re-render in under 16ms (single frame budget) when new data arrives via WebSocket.
2. **Workflow DAG**: Force-directed mini-graph rendering workflow steps and their connections. Used in the Workflow Studio step editor and the Broker Decision Canvas graph context viewer.
3. **Broker Timeline**: Temporal heatmap showing broker decision density and outcomes over time. Used in the Broker Timeline view.

The existing VisionClaw client has a proven WASM bridge pattern in `client/src/wasm/scene-effects-bridge.ts` that provides:
- Dynamic import of wasm-pack generated glue code
- Typed TypeScript wrappers over raw WASM exports
- Zero-copy `Float32Array` / `Uint8Array` views over WASM linear memory
- Module-level singleton with state machine init guard (idle -> loading -> ready | failed)
- Dispose pattern to prevent use-after-free
- Bounds checking on all pointer+length dereferences
- Graceful degradation: callers catch init failure and fall back to non-WASM rendering

The scene-effects WASM module is compiled from Rust via `wasm-pack` and lives in `client/src/wasm/scene-effects/`. It provides `ParticleField`, `AtmosphereField`, and `EnergyWisps` -- all real-time simulation workloads where WASM acceleration is clearly justified (thousands of particles updated per frame).

### The Question

Should the enterprise visualization components follow the same WASM-accelerated pattern, or should they use pure TypeScript/Canvas2D/SVG rendering?

The enterprise visualizations are simpler than particle fields. A sparkline is 30-1000 points. A workflow DAG is 5-50 nodes. A broker timeline heatmap is a 2D grid of cells. These are not obviously WASM-scale workloads.

However, the PRD specifies:
- WASM-powered sparklines with GPU-interpolated time series as a preferred direction
- Subtle yet exciting graphics elements are valued
- 16ms frame budget for sparkline re-render (during real-time WebSocket updates, new points arriving every few seconds)

The question is not "WASM or nothing" but "when does WASM get added, and how does the architecture support adding it later without rewriting?"

## Decision Drivers

- The existing WASM bridge pattern is proven and well-understood
- Enterprise surfaces must ship with functional visualizations before WASM modules are compiled
- WASM compilation and wasm-pack build pipeline add CI/CD complexity
- The 16ms frame budget for sparklines is achievable in TypeScript for 1000 points (Canvas2D can handle this) but WASM provides headroom for future complexity (interpolation, confidence band computation, animation)
- Workflow DAG layout is a genuine compute problem (force-directed simulation); WASM acceleration is more clearly justified here
- The progressive enhancement pattern (ship TypeScript first, swap in WASM later) is architecturally clean if the interface is designed for it
- User preference for Rust WASM interfaces with performant, subtle graphics

## Considered Options

### Option 1: TypeScript-first with WASM bridge interface, three Rust WASM modules as progressive enhancement (chosen)

Design each visualization component with a renderer interface. Ship with a TypeScript Canvas2D/SVG implementation. Define three Rust WASM modules (`kpi-sparklines`, `workflow-dag`, `broker-timeline`) that implement the same renderer interface via the zero-copy bridge pattern. WASM modules are compiled separately and loaded dynamically. Feature detection at runtime determines which renderer to use.

- **Pros**: Enterprise surfaces ship immediately without waiting for WASM compilation. WASM is a performance upgrade, not a blocker. The bridge interface is tested with the existing scene-effects module. Each WASM module can be developed and shipped independently. Fallback is always available for environments without WASM (rare but possible: some CSPs block `WebAssembly.instantiate`).
- **Cons**: Two renderer implementations per visualization (TypeScript + WASM). The WASM renderers may never justify their maintenance cost if TypeScript performance is sufficient.

### Option 2: WASM-only from day one

Build all three visualizations as Rust WASM modules. No TypeScript fallback.

- **Pros**: Single implementation. Maximum performance. Aligns with the project's Rust-native direction.
- **Cons**: Blocks enterprise UI shipping until WASM modules are compiled and tested. The wasm-pack build pipeline must be extended before any enterprise surface can render a sparkline. Environment without WASM support (CSP restrictions, some WebViews) get broken visualizations instead of fallback rendering.

### Option 3: TypeScript-only, no WASM

Use Canvas2D or SVG for all enterprise visualizations. No WASM.

- **Pros**: Simplest. No build pipeline changes. No Rust compilation for visualizations.
- **Cons**: Forecloses the performance headroom that WASM provides. The workflow DAG force-directed layout in TypeScript will be slower than the Rust equivalent, which matters when the DAG needs to animate during editing. Does not align with the project's direction of Rust WASM interfaces. Misses an opportunity to unify the visualization acceleration approach across the platform.

### Option 4: WebGL/WebGPU shaders instead of WASM

Write sparkline and heatmap rendering as WebGL or WebGPU shaders.

- **Pros**: True GPU acceleration. Excellent for large datasets.
- **Cons**: Shader programming for 2D visualizations is complex and hard to maintain. The existing 3D pipeline uses R3F/Three.js which abstracts WebGL; the enterprise visualizations do not need 3D. WebGPU support is still limited. Overkill for the data volumes involved (1000-point sparklines, 50-node DAGs).

## Decision

**Option 1: TypeScript-first with WASM bridge interface. Three Rust WASM modules as progressive enhancement.**

### Architecture

```
TypeScript Component (Sparkline, DAG, Timeline)
    |
    +-- Renderer Interface (abstract)
    |     |
    |     +-- TypeScriptRenderer (Canvas2D / SVG)   <-- ships first
    |     |
    |     +-- WasmRenderer (bridge to WASM module)  <-- ships later
    |
    +-- Feature detection: try WASM init, fall back to TypeScript
```

### Renderer Interface Pattern

Each visualization defines a renderer interface that abstracts the rendering backend:

```typescript
// client/src/features/kpi/types/sparkline-renderer.ts

export interface SparklineRenderInput {
  /** Time-series values (y-axis). Float32Array for WASM compatibility. */
  values: Float32Array;
  /** Timestamps (x-axis, epoch ms). Float64Array for precision. */
  timestamps: Float64Array;
  /** Upper confidence bound per point. Same length as values. */
  confidenceUpper: Float32Array;
  /** Lower confidence bound per point. Same length as values. */
  confidenceLower: Float32Array;
  /** Canvas width in pixels. */
  width: number;
  /** Canvas height in pixels. */
  height: number;
  /** Device pixel ratio for crisp rendering. */
  dpr: number;
  /** HSL hue for the primary line (0-360). */
  hue: number;
}

export interface SparklineRenderer {
  /** Render the sparkline to a canvas element. */
  render(canvas: HTMLCanvasElement, input: SparklineRenderInput): void;
  /** Release resources (WASM memory, etc.). */
  dispose(): void;
}
```

The `Float32Array` and `Float64Array` input types are deliberate: they enable zero-copy transfer to WASM linear memory. The TypeScript renderer accepts them too (they are standard typed arrays).

### TypeScript Baseline Renderers

Ship with PRD-002 Phase 1-3:

**SparklineCanvasRenderer**:
- Renders to a 2D Canvas context
- Line drawing with `ctx.lineTo()` for the value series
- Filled area with `ctx.globalAlpha` for confidence bands
- Gradient stroke using HSL hue from the VisionClaw crystalline palette
- Anti-aliased via DPR-aware canvas sizing
- Performance: < 2ms for 1000 points on modern hardware (well within 16ms budget)

**DagSvgRenderer**:
- Renders workflow steps as SVG `<rect>` + `<text>` elements
- Connections as SVG `<path>` with cubic bezier curves
- Layout computed by a simple top-down DAG algorithm (Sugiyama-style layering) in TypeScript
- Animated via Framer Motion on the SVG elements
- Performance: < 5ms for 50 nodes

**TimelineCanvasRenderer**:
- Renders a 2D heatmap grid to Canvas
- Cell colour derived from decision density (count) and outcome distribution
- Time axis (horizontal), category axis (vertical)
- Performance: < 3ms for a 365 x 10 grid

### WASM Modules (Progressive Enhancement)

Ship with PRD-002 Phase 4:

Three Rust crates compiled via `wasm-pack --target web`:

#### 1. `kpi-sparklines`

```rust
// crates/kpi-sparklines/src/lib.rs
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct SparklineEngine {
    width: u32,
    height: u32,
    dpr: f32,
    // Pre-allocated buffers
    pixel_buffer: Vec<u8>,       // RGBA output
    interpolated: Vec<f32>,      // Interpolated y-values at pixel x-coords
}

#[wasm_bindgen]
impl SparklineEngine {
    #[wasm_bindgen(constructor)]
    pub fn new(width: u32, height: u32, dpr: f32) -> Self { /* ... */ }

    /// Compute interpolated sparkline pixels from raw time series.
    /// Input: values_ptr/values_len point to caller-provided Float32Array.
    /// Output: pixel buffer accessible via get_pixels_ptr/get_pixels_len.
    pub fn compute(
        &mut self,
        values_ptr: *const f32,
        values_len: usize,
        conf_upper_ptr: *const f32,
        conf_lower_ptr: *const f32,
        hue: f32,
    ) { /* ... */ }

    pub fn get_pixels_ptr(&self) -> *const u8 { self.pixel_buffer.as_ptr() }
    pub fn get_pixels_len(&self) -> usize { self.pixel_buffer.len() }

    /// Cubic spline interpolation between data points for sub-pixel smoothness.
    fn interpolate_cubic(&self, values: &[f32], x_positions: &[f32]) -> Vec<f32> { /* ... */ }
}
```

Key capabilities beyond the TypeScript baseline:
- Cubic spline interpolation for smooth curves at any zoom level
- SIMD-friendly inner loops (auto-vectorised by LLVM for `wasm32` target)
- Pre-allocated pixel buffer avoids GC pressure on every re-render
- Confidence band rendering with Gaussian-kernel smoothing

#### 2. `workflow-dag`

```rust
// crates/workflow-dag/src/lib.rs
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct DagLayout {
    node_count: usize,
    // Per-node: [x, y] positions after layout
    positions: Vec<f32>,
    // Per-edge: [from_idx, to_idx] pairs
    edges: Vec<u32>,
    // Force-directed simulation state
    velocities: Vec<f32>,
    iteration: u32,
}

#[wasm_bindgen]
impl DagLayout {
    #[wasm_bindgen(constructor)]
    pub fn new(node_count: usize) -> Self { /* ... */ }

    /// Set edge list. edges_ptr points to [from, to, from, to, ...] pairs.
    pub fn set_edges(&mut self, edges_ptr: *const u32, edges_len: usize) { /* ... */ }

    /// Run N iterations of force-directed layout.
    /// Uses Sugiyama layering as initial layout, then refines with
    /// repulsion + attraction forces constrained to layer ordering.
    pub fn step(&mut self, iterations: u32, dt: f32) { /* ... */ }

    pub fn get_positions_ptr(&self) -> *const f32 { self.positions.as_ptr() }
    pub fn get_positions_len(&self) -> usize { self.positions.len() }

    pub fn is_converged(&self) -> bool { /* velocity magnitude below threshold */ }
}
```

Key capabilities:
- Layer-constrained force-directed layout (Sugiyama + spring model)
- Convergence detection
- Incremental re-layout when steps are added/removed (does not restart from scratch)

#### 3. `broker-timeline`

```rust
// crates/broker-timeline/src/lib.rs
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct TimelineHeatmap {
    cols: u32,        // time buckets
    rows: u32,        // category rows
    pixel_buffer: Vec<u8>,  // RGBA output
    cell_values: Vec<f32>,  // density values per cell
}

#[wasm_bindgen]
impl TimelineHeatmap {
    #[wasm_bindgen(constructor)]
    pub fn new(cols: u32, rows: u32, cell_width: u32, cell_height: u32) -> Self { /* ... */ }

    /// Set cell values. values_ptr points to row-major [cols * rows] floats.
    pub fn set_values(&mut self, values_ptr: *const f32, values_len: usize) { /* ... */ }

    /// Render the heatmap with a colour ramp based on the VisionClaw palette.
    /// Applies Gaussian blur for smooth cell transitions.
    pub fn render(&mut self, hue_start: f32, hue_end: f32) { /* ... */ }

    pub fn get_pixels_ptr(&self) -> *const u8 { self.pixel_buffer.as_ptr() }
    pub fn get_pixels_len(&self) -> usize { self.pixel_buffer.len() }
}
```

Key capabilities:
- Gaussian-smoothed cell transitions (bioluminescent glow effect between cells)
- HSL colour ramp consistent with the VisionClaw aesthetic
- Pre-allocated pixel buffer for zero-GC animation

### WASM Bridge Pattern (Following scene-effects-bridge.ts)

Each WASM module gets a bridge file following the exact pattern of `scene-effects-bridge.ts`:

```
client/src/wasm/
  scene-effects/              # Existing
  scene-effects-bridge.ts     # Existing

  kpi-sparklines/             # New WASM build output
    kpi_sparklines.js
    kpi_sparklines.d.ts
    kpi_sparklines_bg.wasm
    kpi_sparklines_bg.wasm.d.ts
    package.json

  workflow-dag/               # New WASM build output
    workflow_dag.js
    workflow_dag.d.ts
    workflow_dag_bg.wasm
    workflow_dag_bg.wasm.d.ts
    package.json

  broker-timeline/            # New WASM build output
    broker_timeline.js
    broker_timeline.d.ts
    broker_timeline_bg.wasm
    broker_timeline_bg.wasm.d.ts
    package.json

  kpi-sparklines-bridge.ts    # New bridge
  workflow-dag-bridge.ts      # New bridge
  broker-timeline-bridge.ts   # New bridge
```

Each bridge file follows the same structure:
1. Interface definitions mirroring the WASM exports
2. Bridge class with `WebAssembly.Memory` reference for zero-copy views
3. `isDisposed` guard on all methods
4. `dispose()` method calling `.free()` on the WASM handle
5. Module-level singleton with state machine (idle/loading/ready/failed)
6. Async `init` function with dynamic import, cached promise, and 1-second retry backoff on failure
7. Logging via `createLogger`

Example bridge:

```typescript
// client/src/wasm/kpi-sparklines-bridge.ts

export class SparklineEngineBridge {
  private inner: WasmSparklineEngine;
  private memory: WebAssembly.Memory;
  private _disposed = false;

  constructor(inner: WasmSparklineEngine, memory: WebAssembly.Memory) {
    this.inner = inner;
    this.memory = memory;
  }

  get isDisposed(): boolean { return this._disposed; }

  compute(
    values: Float32Array,
    confidenceUpper: Float32Array,
    confidenceLower: Float32Array,
    hue: number,
  ): void {
    if (this._disposed) return;
    // Write input arrays to WASM memory (or pass pointers if already in linear memory)
    this.inner.compute(
      values.byteOffset, values.length,
      confidenceUpper.byteOffset,
      confidenceLower.byteOffset,
      hue,
    );
  }

  /** Zero-copy Uint8Array view of RGBA pixel output. */
  getPixels(): Uint8Array {
    if (this._disposed) return new Uint8Array(0);
    const ptr = this.inner.get_pixels_ptr();
    const len = this.inner.get_pixels_len();
    if (ptr + len > this.memory.buffer.byteLength) {
      throw new Error('WASM pointer out of bounds');
    }
    return new Uint8Array(this.memory.buffer, ptr, len);
  }

  dispose(): void {
    if (this._disposed) return;
    this._disposed = true;
    this.inner.free();
  }
}
```

### Feature Detection and Fallback

Each visualization component uses a hook that attempts WASM init and falls back:

```typescript
// client/src/features/kpi/hooks/useSparklineRenderer.ts

import { useRef, useEffect, useState } from 'react';
import type { SparklineRenderer } from '../types/sparkline-renderer';
import { SparklineCanvasRenderer } from '../renderers/SparklineCanvasRenderer';

export function useSparklineRenderer(): SparklineRenderer {
  const rendererRef = useRef<SparklineRenderer | null>(null);
  const [ready, setReady] = useState(false);

  useEffect(() => {
    let cancelled = false;

    (async () => {
      try {
        // Dynamic import -- only loaded if WASM is available
        const { initKpiSparklines } = await import('../../../wasm/kpi-sparklines-bridge');
        const wasmApi = await initKpiSparklines();
        if (!cancelled) {
          rendererRef.current = new WasmSparklineRenderer(wasmApi);
          setReady(true);
        }
      } catch {
        // WASM unavailable or failed to load -- use TypeScript fallback
        if (!cancelled) {
          rendererRef.current = new SparklineCanvasRenderer();
          setReady(true);
        }
      }
    })();

    return () => {
      cancelled = true;
      rendererRef.current?.dispose();
    };
  }, []);

  // Return TypeScript renderer immediately; replace with WASM when ready
  if (!rendererRef.current) {
    rendererRef.current = new SparklineCanvasRenderer();
  }
  return rendererRef.current;
}
```

This ensures:
- The component renders immediately with the TypeScript renderer
- If WASM loads successfully, subsequent renders use WASM
- If WASM fails, the TypeScript renderer continues indefinitely
- No user-visible loading state for the visualization itself

### Rust Workspace Integration

The three WASM crates are added to the project's Cargo workspace as independent crates. They do not depend on the main `webxr` crate (which is a `cdylib` server binary):

```toml
# Cargo.toml (workspace-level, or new workspace section)
[workspace]
members = [
  ".",                         # webxr server
  "crates/kpi-sparklines",
  "crates/workflow-dag",
  "crates/broker-timeline",
]
```

Each WASM crate has its own `Cargo.toml`:

```toml
# crates/kpi-sparklines/Cargo.toml
[package]
name = "kpi-sparklines"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
wasm-bindgen = "0.2"

[profile.release]
opt-level = "s"        # Size optimisation for WASM
lto = true
```

Build command:

```bash
cd crates/kpi-sparklines && wasm-pack build --target web --out-dir ../../client/src/wasm/kpi-sparklines/
```

A `Makefile` or npm script orchestrates building all WASM modules:

```bash
# client/package.json scripts
"wasm:build": "cd ../crates/kpi-sparklines && wasm-pack build --target web --out-dir ../../client/src/wasm/kpi-sparklines/ && cd ../workflow-dag && wasm-pack build --target web --out-dir ../../client/src/wasm/workflow-dag/ && cd ../broker-timeline && wasm-pack build --target web --out-dir ../../client/src/wasm/broker-timeline/",
"wasm:build:sparklines": "cd ../crates/kpi-sparklines && wasm-pack build --target web --out-dir ../../client/src/wasm/kpi-sparklines/",
```

### Shared Rust Utilities

If the three WASM crates share common code (colour ramps, interpolation algorithms, HSL conversion), extract a `crates/viz-common` crate (non-WASM, pure Rust library) that the WASM crates depend on:

```toml
# crates/viz-common/Cargo.toml
[package]
name = "viz-common"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["lib"]    # Standard Rust library, not cdylib

# No wasm-bindgen dependency
```

This prevents duplicating colour palette logic and interpolation math across three WASM crates.

### Performance Budget

| Component | TypeScript Baseline | WASM Target | Improvement |
|-----------|-------------------|-------------|-------------|
| Sparkline (100 points) | < 1ms | < 0.3ms | 3x (headroom for animation) |
| Sparkline (1000 points) | < 2ms | < 0.5ms | 4x (cubic interpolation adds load) |
| Workflow DAG (50 nodes, 10 iterations) | < 8ms | < 2ms | 4x (force computation is arithmetic-heavy) |
| Timeline heatmap (365 x 10) | < 3ms | < 0.8ms | 4x (pixel buffer fill with colour ramp) |

These targets are measured on a mid-range 2024 laptop. The TypeScript baselines already meet the 16ms frame budget. WASM provides headroom for:
- Higher-fidelity rendering (cubic interpolation, Gaussian smoothing)
- More data points (zoom-to-detail on KPI drill-down)
- Animated transitions (sparkline point slide-in, DAG re-layout)
- Simultaneous rendering of multiple components (four KPI cards on the dashboard)

### Build Pipeline

WASM modules are not rebuilt on every `npm run dev`. They are pre-built artefacts:

1. **Development**: WASM modules are built once and committed to the repo (or built in CI and cached). The TypeScript fallback is the primary development renderer.
2. **CI**: GitHub Actions workflow runs `wasm:build` and includes WASM output in the client build artefact.
3. **Production**: `npm run build` includes pre-built WASM files via Vite's static asset handling. Vite treats `.wasm` files as assets and serves them with correct MIME type.

The WASM build step is explicitly separated from the TypeScript build to avoid requiring `wasm-pack` and a Rust toolchain for frontend developers.

## Consequences

### Positive

- Enterprise surfaces ship immediately with TypeScript renderers; no WASM compilation blocking
- The renderer interface pattern cleanly separates visualization logic from rendering backend
- WASM modules follow the proven scene-effects bridge pattern, reducing implementation risk
- Zero-copy Float32Array transfers eliminate serialisation overhead between TypeScript and WASM
- The fallback chain (try WASM, fall back to TypeScript) handles all environments gracefully
- Three independent WASM crates can be developed, tested, and deployed independently
- Shared `viz-common` crate prevents code duplication across WASM modules
- The cosmic aesthetic (crystalline palette, bioluminescent glow) is implemented in Rust with precise colour control

### Negative

- Two renderer implementations per visualization doubles the rendering code surface. Mitigation: the TypeScript renderers are simple Canvas2D/SVG implementations (~100-200 lines each). The cost is low relative to the resilience benefit.
- Three WASM crates add build pipeline complexity. Mitigation: builds are separated from the TypeScript build; WASM artifacts are pre-built. Frontend developers do not need `wasm-pack` locally.
- WASM module sizes add to the download budget. Estimated: 30-50KB per module (gzipped). Mitigation: modules are lazy-loaded only when the enterprise surface is accessed; they are not in the critical path for graph visualisation.
- The WASM modules may never provide perceptible performance improvement for small datasets (30-point sparklines). The justification is headroom for future complexity and aesthetic fidelity, not current-scale performance. This is an acceptable trade-off given the user preference for WASM Rust interfaces.

### Neutral

- The existing scene-effects WASM module and bridge are not modified
- The existing 3D rendering pipeline (R3F, Three.js, post-processing) is not affected
- The WASM modules produce pixel buffers or position arrays, not DOM elements; they compose with any React rendering approach
- The WASM crate workspace does not affect the main `webxr` server crate compilation

## Related Decisions

- ADR-046: Enterprise UI Architecture (defines the feature modules that consume these visualization components)
- ADR-043: KPI Lineage Model (defines the data model behind sparkline time series)
- ADR-041: Judgment Broker Workbench (defines the broker timeline and decision canvas that use DAG and timeline visualizations)
- ADR-042: Workflow Proposal Object Model (defines the workflow step model that the DAG visualizes)
- ADR-013: Render Performance (established performance budgeting approach for the client)
- PRD-002: Enterprise Control Plane UI (product requirements for visualization fidelity and performance)

## References

- `client/src/wasm/scene-effects-bridge.ts` (existing WASM bridge pattern -- the template for all new bridges)
- `client/src/wasm/scene-effects/` (existing WASM build output directory structure)
- `client/src/features/design-system/animations.ts` (Framer Motion presets used alongside WASM renderers)
- wasm-pack documentation: https://rustwasm.github.io/wasm-pack/
- wasm-bindgen documentation: https://rustwasm.github.io/wasm-bindgen/
- Vite WASM integration: https://vite.dev/guide/features#webassembly
