/* tslint:disable */
/* eslint-disable */

/**
 * Atmospheric density field that generates RGBA texture data.
 *
 * The texture evolves over time using 3D fBm noise (2D position + time),
 * producing an organic, gently shifting nebula background.
 */
export class AtmosphereField {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Texture height.
     */
    get_height(): number;
    /**
     * Number of bytes in the pixel buffer.
     */
    get_pixels_len(): number;
    /**
     * Raw pointer to the RGBA pixel buffer for zero-copy access.
     * Layout: [r0, g0, b0, a0, r1, g1, b1, a1, ...] (width * height * 4 bytes)
     */
    get_pixels_ptr(): number;
    /**
     * Texture width.
     */
    get_width(): number;
    /**
     * Create a new atmosphere texture generator.
     *
     * * `width` - Texture width (0 defaults to 128)
     * * `height` - Texture height (0 defaults to 128)
     */
    constructor(width: number, height: number);
    /**
     * Set the noise frequency. Higher values produce finer detail.
     */
    set_frequency(freq: number): void;
    /**
     * Set the animation speed multiplier.
     */
    set_speed(speed: number): void;
    /**
     * Advance the atmosphere by `dt` seconds and regenerate the texture.
     *
     * This is the main per-frame call. It writes RGBA data into the
     * internal pixel buffer which can then be read via `get_pixels_ptr`.
     */
    update(dt: number): void;
}

export class EnergyWisps {
    free(): void;
    [Symbol.dispose](): void;
    get_hues_len(): number;
    get_hues_ptr(): number;
    get_opacities_len(): number;
    get_opacities_ptr(): number;
    get_positions_len(): number;
    get_positions_ptr(): number;
    get_sizes_len(): number;
    get_sizes_ptr(): number;
    /**
     * Create a new wisp field with the given count (clamped to MAX_WISPS).
     */
    constructor(count: number);
    /**
     * Set the drift speed multiplier (default 1.0).
     */
    set_drift_speed(speed: number): void;
    /**
     * Advance simulation by `dt` seconds.
     *
     * Camera position is used for depth-aware opacity, same as ParticleField.
     */
    update(dt: number, camera_x: number, camera_y: number, camera_z: number): void;
    wisp_count(): number;
}

/**
 * Mini-graph rasteriser used by the Decision Canvas skill preview cards and
 * the `/studio/:workspaceId` embedded work-lane graph.
 *
 * Accepts:
 *   * Stride-7 node buffer `[x, y, r, g, b, a, weight]`
 *   * Flat edge buffer `[from, to, from, to, ...]`
 *
 * Renders edges first (linear blend) then nodes (additive glow).
 */
export class MiniGraph {
    free(): void;
    [Symbol.dispose](): void;
    edge_count(): number;
    get_height(): number;
    get_pixels_len(): number;
    get_pixels_ptr(): number;
    get_width(): number;
    constructor(width: number, height: number);
    node_count(): number;
    /**
     * Render the mini-graph into the pixel buffer.
     */
    render(): void;
    /**
     * Convenience: upload both buffers and render in one call.
     */
    render_with(nodes_ptr: number, nodes_len: number, edges_ptr: number, edges_len: number): boolean;
    set_edges(ptr: number, len: number): void;
    set_nodes(ptr: number, len: number): void;
}

/**
 * Ontology-neighbor thumbnail renderer for Sensei nudge cards.
 *
 * Produces a small RGBA image of the neighbourhood around a focus node:
 * a central dot with up to 8 neighbours laid out by their `(x, y)` in NDC
 * plus connecting radial lines tinted by each neighbour's `weight`.
 */
export class OntologyNeighborThumb {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Thumbnail height in pixels.
     */
    get_height(): number;
    /**
     * Number of bytes in the pixel buffer.
     */
    get_pixels_len(): number;
    /**
     * Raw pointer to the RGBA8 pixel buffer.
     */
    get_pixels_ptr(): number;
    /**
     * Thumbnail width in pixels.
     */
    get_width(): number;
    /**
     * Create a new thumbnail renderer with the given dimensions.
     */
    constructor(width: number, height: number);
    /**
     * Render the thumbnail into the pixel buffer. Safe to call repeatedly;
     * clears the buffer first.
     */
    render(): void;
    /**
     * Convenience: upload `nodes` and render in one call. Returns `true` on
     * success. Used by the `renderOntologyNeighborThumb` bridge helper.
     */
    render_with(ptr: number, len: number): boolean;
    /**
     * Set the neighbour buffer. Stride is 7 floats: `[x, y, r, g, b, a, weight]`.
     * Node 0 is the focus (centre). Nodes 1..N are neighbours. Coordinates in
     * [-1, 1] NDC.
     */
    set_nodes(ptr: number, len: number): void;
}

/**
 * Particle field managing positions, velocities, visual properties.
 *
 * All buffers are contiguous f32 arrays suitable for direct Float32Array
 * views from JavaScript without any copying.
 */
export class ParticleField {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Number of f32 values in the opacities buffer.
     */
    get_opacities_len(): number;
    /**
     * Raw pointer to the opacities buffer.
     * Layout: [o0, o1, o2, ...] (count floats)
     */
    get_opacities_ptr(): number;
    /**
     * Number of f32 values in the positions buffer.
     */
    get_positions_len(): number;
    /**
     * Raw pointer to the positions buffer for zero-copy Float32Array.
     * Layout: [x0, y0, z0, x1, y1, z1, ...] (count * 3 floats)
     */
    get_positions_ptr(): number;
    /**
     * Number of f32 values in the sizes buffer.
     */
    get_sizes_len(): number;
    /**
     * Raw pointer to the sizes buffer.
     */
    get_sizes_ptr(): number;
    /**
     * Create a new particle field with the given number of particles.
     * Clamped to MAX_PARTICLES.
     */
    constructor(count: number);
    /**
     * Current particle count.
     */
    particle_count(): number;
    /**
     * Advance the particle simulation by `dt` seconds.
     *
     * Camera position is used for depth-aware opacity: particles near the
     * camera fade out (to avoid visual clutter) while distant particles
     * have gentle luminosity.
     */
    update(dt: number, camera_x: number, camera_y: number, camera_z: number): void;
}

/**
 * Initialize the WASM module. Call once before using any other exports.
 * Sets up the panic hook for better error messages in the browser console.
 */
export function init(): void;

/**
 * Stateless one-shot renderer exposed as `renderMiniGraph` on the JS side.
 */
export function renderMiniGraph(width: number, height: number, nodes_ptr: number, nodes_len: number, edges_ptr: number, edges_len: number): Uint8Array;

/**
 * Stateless one-shot renderer exposed as `renderOntologyNeighborThumb` on the
 * JS side. Allocates a fresh thumbnail, renders, and returns the pixel
 * buffer as a `Box<[u8]>` (boxed slice, length-prefixed by wasm-bindgen).
 *
 * Note: `Box<[u8]>` is serialised by wasm-bindgen as a `Uint8Array` copy.
 * For zero-copy access, callers should instantiate `OntologyNeighborThumb`
 * directly and use the `get_pixels_ptr` / `get_pixels_len` pair.
 */
export function renderOntologyNeighborThumb(width: number, height: number, nodes_ptr: number, nodes_len: number): Uint8Array;

/**
 * Diagnostic: returns the library version string.
 */
export function version(): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_minigraph_free: (a: number, b: number) => void;
    readonly __wbg_ontologyneighborthumb_free: (a: number, b: number) => void;
    readonly minigraph_edge_count: (a: number) => number;
    readonly minigraph_get_height: (a: number) => number;
    readonly minigraph_get_pixels_len: (a: number) => number;
    readonly minigraph_get_pixels_ptr: (a: number) => number;
    readonly minigraph_get_width: (a: number) => number;
    readonly minigraph_new: (a: number, b: number) => number;
    readonly minigraph_node_count: (a: number) => number;
    readonly minigraph_render: (a: number) => void;
    readonly minigraph_render_with: (a: number, b: number, c: number, d: number, e: number) => number;
    readonly minigraph_set_edges: (a: number, b: number, c: number) => void;
    readonly minigraph_set_nodes: (a: number, b: number, c: number) => void;
    readonly ontologyneighborthumb_get_height: (a: number) => number;
    readonly ontologyneighborthumb_get_pixels_len: (a: number) => number;
    readonly ontologyneighborthumb_get_pixels_ptr: (a: number) => number;
    readonly ontologyneighborthumb_get_width: (a: number) => number;
    readonly ontologyneighborthumb_new: (a: number, b: number) => number;
    readonly ontologyneighborthumb_render: (a: number) => void;
    readonly ontologyneighborthumb_render_with: (a: number, b: number, c: number) => number;
    readonly ontologyneighborthumb_set_nodes: (a: number, b: number, c: number) => void;
    readonly renderMiniGraph: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number];
    readonly renderOntologyNeighborThumb: (a: number, b: number, c: number, d: number) => [number, number];
    readonly __wbg_energywisps_free: (a: number, b: number) => void;
    readonly energywisps_get_hues_len: (a: number) => number;
    readonly energywisps_get_hues_ptr: (a: number) => number;
    readonly energywisps_get_opacities_len: (a: number) => number;
    readonly energywisps_get_opacities_ptr: (a: number) => number;
    readonly energywisps_get_positions_len: (a: number) => number;
    readonly energywisps_get_positions_ptr: (a: number) => number;
    readonly energywisps_get_sizes_len: (a: number) => number;
    readonly energywisps_get_sizes_ptr: (a: number) => number;
    readonly energywisps_new: (a: number) => number;
    readonly energywisps_set_drift_speed: (a: number, b: number) => void;
    readonly energywisps_update: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly energywisps_wisp_count: (a: number) => number;
    readonly __wbg_particlefield_free: (a: number, b: number) => void;
    readonly particlefield_get_opacities_len: (a: number) => number;
    readonly particlefield_get_opacities_ptr: (a: number) => number;
    readonly particlefield_get_positions_len: (a: number) => number;
    readonly particlefield_get_positions_ptr: (a: number) => number;
    readonly particlefield_get_sizes_len: (a: number) => number;
    readonly particlefield_get_sizes_ptr: (a: number) => number;
    readonly particlefield_new: (a: number) => number;
    readonly particlefield_particle_count: (a: number) => number;
    readonly particlefield_update: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly init: () => void;
    readonly version: () => [number, number];
    readonly __wbg_atmospherefield_free: (a: number, b: number) => void;
    readonly atmospherefield_get_height: (a: number) => number;
    readonly atmospherefield_get_pixels_len: (a: number) => number;
    readonly atmospherefield_get_pixels_ptr: (a: number) => number;
    readonly atmospherefield_get_width: (a: number) => number;
    readonly atmospherefield_new: (a: number, b: number) => number;
    readonly atmospherefield_set_frequency: (a: number, b: number) => void;
    readonly atmospherefield_set_speed: (a: number, b: number) => void;
    readonly atmospherefield_update: (a: number, b: number) => void;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
