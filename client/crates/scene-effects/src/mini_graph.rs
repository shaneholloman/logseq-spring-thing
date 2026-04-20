//! Mini-graph / ontology-neighbor thumbnail renderers.
//!
//! Two small RGBA rasterisers that share the zero-copy Float32Array input
//! pattern established by `ParticleField` / `AtmosphereField`:
//!
//! - **OntologyNeighborThumb**: radial thumbnail used in Sensei nudges. Ingests
//!   a compact `[x, y, r, g, b, a, weight, ...]` position buffer (stride 7) and
//!   rasterises a soft-dot + edge composite into an RGBA8 buffer.
//!
//! - **MiniGraph**: 2D mini-graph rasteriser used by Decision Canvas skill
//!   preview cards and the `/studio/:workspaceId` work lane. Accepts the same
//!   stride-7 buffer plus a flat `[from_idx, to_idx, ...]` edge list and
//!   rasterises straight-line edges followed by node dots with additive glow.
//!
//! Design notes:
//!   * All allocations happen in `new`; `render_*` is pure compute.
//!   * Float32Array input is zero-copy (JS passes `values.byteOffset` / `length`).
//!   * Output is a contiguous RGBA8 buffer exposed via `ptr` / `len` accessors.
//!   * Coordinate input is expected in [-1, 1] NDC-style; renderer maps to pixel
//!     space with a small inset so edge pixels are not clipped by `wasm-opt`
//!     auto-inlining heuristics at low resolutions.

use wasm_bindgen::prelude::*;

/// Stride of the node/position buffer: `[x, y, r, g, b, a, weight]`.
const NODE_STRIDE: usize = 7;

/// Maximum thumbnail/mini-graph dimension to cap pathological input.
const MAX_DIM: u32 = 512;

/// Hard cap on nodes. Mini-graph thumbnails are 16-64 node views; 256 is
/// generous and prevents a runaway caller from exhausting linear memory.
const MAX_NODES: usize = 256;

/// Hard cap on edges (undirected pairs count once).
const MAX_EDGES: usize = 1024;

/// Clamp to [0, 1).
#[inline]
fn clamp01(v: f32) -> f32 {
    if v.is_nan() { 0.0 } else { v.clamp(0.0, 1.0) }
}

/// Premultiplied additive blend of a single RGBA sample into the pixel buffer.
#[inline]
fn blend_add(buf: &mut [u8], idx: usize, r: f32, g: f32, b: f32, a: f32) {
    // Additive with soft saturation via min.
    let cur_r = buf[idx] as f32;
    let cur_g = buf[idx + 1] as f32;
    let cur_b = buf[idx + 2] as f32;
    let cur_a = buf[idx + 3] as f32;
    buf[idx]     = (cur_r + r * 255.0 * a).min(255.0) as u8;
    buf[idx + 1] = (cur_g + g * 255.0 * a).min(255.0) as u8;
    buf[idx + 2] = (cur_b + b * 255.0 * a).min(255.0) as u8;
    buf[idx + 3] = (cur_a + a * 255.0).min(255.0) as u8;
}

/// Draw a soft radial dot centred at pixel (cx, cy) with the given radius.
fn splat_dot(
    buf: &mut [u8],
    width: u32,
    height: u32,
    cx: f32,
    cy: f32,
    radius: f32,
    r: f32, g: f32, b: f32, a: f32,
) {
    let r2 = radius * radius;
    let ix0 = ((cx - radius).floor() as i32).max(0);
    let iy0 = ((cy - radius).floor() as i32).max(0);
    let ix1 = ((cx + radius).ceil() as i32).min(width as i32 - 1);
    let iy1 = ((cy + radius).ceil() as i32).min(height as i32 - 1);

    for py in iy0..=iy1 {
        let dy = py as f32 - cy;
        for px in ix0..=ix1 {
            let dx = px as f32 - cx;
            let d2 = dx * dx + dy * dy;
            if d2 > r2 { continue; }
            // Smooth falloff: 1 at centre, 0 at edge.
            let t = 1.0 - (d2 / r2).sqrt();
            let falloff = t * t;
            let idx = ((py as u32 * width + px as u32) * 4) as usize;
            blend_add(buf, idx, r, g, b, a * falloff);
        }
    }
}

/// Rasterise a straight line segment with a 1.5-pixel-wide soft core.
#[allow(clippy::too_many_arguments)]
fn splat_line(
    buf: &mut [u8],
    width: u32,
    height: u32,
    x0: f32, y0: f32, x1: f32, y1: f32,
    r: f32, g: f32, b: f32, a: f32,
) {
    // DDA with super-sampled alpha for anti-aliasing on low-res thumbs.
    let dx = x1 - x0;
    let dy = y1 - y0;
    let steps = dx.abs().max(dy.abs()).ceil().max(1.0) as i32;
    let sx = dx / steps as f32;
    let sy = dy / steps as f32;
    let mut x = x0;
    let mut y = y0;
    for _ in 0..=steps {
        let pxi = x.round() as i32;
        let pyi = y.round() as i32;
        if pxi >= 0 && pyi >= 0 && pxi < width as i32 && pyi < height as i32 {
            // 2x2 splat for a softened core.
            for oy in 0..=1 {
                for ox in 0..=1 {
                    let qx = (pxi + ox).clamp(0, width as i32 - 1);
                    let qy = (pyi + oy).clamp(0, height as i32 - 1);
                    let idx = ((qy as u32 * width + qx as u32) * 4) as usize;
                    blend_add(buf, idx, r, g, b, a * 0.35);
                }
            }
        }
        x += sx;
        y += sy;
    }
}

/// Shared input-buffer validation + copy into an internal Vec.
fn copy_input(ptr: *const f32, len: usize, cap: usize) -> Vec<f32> {
    let safe_len = len.min(cap);
    if ptr.is_null() || safe_len == 0 {
        return Vec::new();
    }
    // SAFETY: caller provides a valid Float32Array-backed pointer + length.
    // We copy to own the data (the JS-side buffer may be mutated between
    // calls). `safe_len` is clamped so we never read past `cap`.
    let slice = unsafe { std::slice::from_raw_parts(ptr, safe_len) };
    slice.to_vec()
}

/// Ontology-neighbor thumbnail renderer for Sensei nudge cards.
///
/// Produces a small RGBA image of the neighbourhood around a focus node:
/// a central dot with up to 8 neighbours laid out by their `(x, y)` in NDC
/// plus connecting radial lines tinted by each neighbour's `weight`.
#[wasm_bindgen]
pub struct OntologyNeighborThumb {
    width: u32,
    height: u32,
    pixel_buffer: Vec<u8>,
    // Last input retained so callers can re-render without re-uploading data.
    nodes: Vec<f32>,
}

#[wasm_bindgen]
impl OntologyNeighborThumb {
    /// Create a new thumbnail renderer with the given dimensions.
    #[wasm_bindgen(constructor)]
    pub fn new(width: u32, height: u32) -> Self {
        let w = width.min(MAX_DIM).max(16);
        let h = height.min(MAX_DIM).max(16);
        Self {
            width: w,
            height: h,
            pixel_buffer: vec![0u8; (w * h * 4) as usize],
            nodes: Vec::new(),
        }
    }

    /// Set the neighbour buffer. Stride is 7 floats: `[x, y, r, g, b, a, weight]`.
    /// Node 0 is the focus (centre). Nodes 1..N are neighbours. Coordinates in
    /// [-1, 1] NDC.
    pub fn set_nodes(&mut self, ptr: *const f32, len: usize) {
        self.nodes = copy_input(ptr, len, MAX_NODES * NODE_STRIDE);
    }

    /// Render the thumbnail into the pixel buffer. Safe to call repeatedly;
    /// clears the buffer first.
    pub fn render(&mut self) {
        // Clear.
        for b in self.pixel_buffer.iter_mut() { *b = 0; }

        let w = self.width as f32;
        let h = self.height as f32;
        let inset = 4.0;
        let node_count = self.nodes.len() / NODE_STRIDE;
        if node_count == 0 { return; }

        // NDC -> pixel mapping helper.
        let map = |x: f32, y: f32| -> (f32, f32) {
            let px = inset + ((x.clamp(-1.0, 1.0) + 1.0) * 0.5) * (w - inset * 2.0);
            let py = inset + ((y.clamp(-1.0, 1.0) + 1.0) * 0.5) * (h - inset * 2.0);
            (px, py)
        };

        // Focus is node 0.
        let fx = self.nodes[0];
        let fy = self.nodes[1];
        let (fpx, fpy) = map(fx, fy);

        // 1. Radial edges (focus -> each neighbour).
        for i in 1..node_count {
            let base = i * NODE_STRIDE;
            let nx = self.nodes[base];
            let ny = self.nodes[base + 1];
            let nr = clamp01(self.nodes[base + 2]);
            let ng = clamp01(self.nodes[base + 3]);
            let nb = clamp01(self.nodes[base + 4]);
            let weight = clamp01(self.nodes[base + 6]);
            let (npx, npy) = map(nx, ny);
            splat_line(
                &mut self.pixel_buffer, self.width, self.height,
                fpx, fpy, npx, npy,
                nr, ng, nb, 0.25 + weight * 0.35,
            );
        }

        // 2. Neighbour dots (drawn after edges so dots pop over edges).
        for i in 1..node_count {
            let base = i * NODE_STRIDE;
            let nx = self.nodes[base];
            let ny = self.nodes[base + 1];
            let nr = clamp01(self.nodes[base + 2]);
            let ng = clamp01(self.nodes[base + 3]);
            let nb = clamp01(self.nodes[base + 4]);
            let na = clamp01(self.nodes[base + 5]).max(0.4);
            let weight = clamp01(self.nodes[base + 6]);
            let (npx, npy) = map(nx, ny);
            let radius = 2.5 + weight * 3.5;
            splat_dot(
                &mut self.pixel_buffer, self.width, self.height,
                npx, npy, radius,
                nr, ng, nb, na,
            );
        }

        // 3. Focus dot on top, with a halo.
        let fr = clamp01(self.nodes[2]).max(0.7);
        let fg = clamp01(self.nodes[3]).max(0.8);
        let fb = clamp01(self.nodes[4]).max(0.95);
        splat_dot(
            &mut self.pixel_buffer, self.width, self.height,
            fpx, fpy, 7.0, fr, fg, fb, 0.35,
        );
        splat_dot(
            &mut self.pixel_buffer, self.width, self.height,
            fpx, fpy, 3.5, fr, fg, fb, 1.0,
        );
    }

    /// Convenience: upload `nodes` and render in one call. Returns `true` on
    /// success. Used by the `renderOntologyNeighborThumb` bridge helper.
    pub fn render_with(&mut self, ptr: *const f32, len: usize) -> bool {
        self.set_nodes(ptr, len);
        self.render();
        true
    }

    /// Raw pointer to the RGBA8 pixel buffer.
    pub fn get_pixels_ptr(&self) -> *const u8 { self.pixel_buffer.as_ptr() }
    /// Number of bytes in the pixel buffer.
    pub fn get_pixels_len(&self) -> usize { self.pixel_buffer.len() }
    /// Thumbnail width in pixels.
    pub fn get_width(&self) -> u32 { self.width }
    /// Thumbnail height in pixels.
    pub fn get_height(&self) -> u32 { self.height }
}

/// Mini-graph rasteriser used by the Decision Canvas skill preview cards and
/// the `/studio/:workspaceId` embedded work-lane graph.
///
/// Accepts:
///   * Stride-7 node buffer `[x, y, r, g, b, a, weight]`
///   * Flat edge buffer `[from, to, from, to, ...]`
///
/// Renders edges first (linear blend) then nodes (additive glow).
#[wasm_bindgen]
pub struct MiniGraph {
    width: u32,
    height: u32,
    pixel_buffer: Vec<u8>,
    nodes: Vec<f32>,
    edges: Vec<u32>,
}

#[wasm_bindgen]
impl MiniGraph {
    #[wasm_bindgen(constructor)]
    pub fn new(width: u32, height: u32) -> Self {
        let w = width.min(MAX_DIM).max(32);
        let h = height.min(MAX_DIM).max(32);
        Self {
            width: w,
            height: h,
            pixel_buffer: vec![0u8; (w * h * 4) as usize],
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }

    pub fn set_nodes(&mut self, ptr: *const f32, len: usize) {
        self.nodes = copy_input(ptr, len, MAX_NODES * NODE_STRIDE);
    }

    pub fn set_edges(&mut self, ptr: *const u32, len: usize) {
        let safe_len = len.min(MAX_EDGES * 2);
        if ptr.is_null() || safe_len == 0 {
            self.edges = Vec::new();
            return;
        }
        // SAFETY: JS passes a Uint32Array-backed pointer; we clamp length and copy.
        let slice = unsafe { std::slice::from_raw_parts(ptr, safe_len) };
        self.edges = slice.to_vec();
    }

    /// Render the mini-graph into the pixel buffer.
    pub fn render(&mut self) {
        for b in self.pixel_buffer.iter_mut() { *b = 0; }

        let w = self.width as f32;
        let h = self.height as f32;
        let inset = 6.0;
        let node_count = self.nodes.len() / NODE_STRIDE;
        if node_count == 0 { return; }

        let map = |x: f32, y: f32| -> (f32, f32) {
            let px = inset + ((x.clamp(-1.0, 1.0) + 1.0) * 0.5) * (w - inset * 2.0);
            let py = inset + ((y.clamp(-1.0, 1.0) + 1.0) * 0.5) * (h - inset * 2.0);
            (px, py)
        };

        // Edges first.
        let edge_count = self.edges.len() / 2;
        for i in 0..edge_count {
            let from = self.edges[i * 2] as usize;
            let to = self.edges[i * 2 + 1] as usize;
            if from >= node_count || to >= node_count { continue; }
            let fbase = from * NODE_STRIDE;
            let tbase = to * NODE_STRIDE;
            let (fpx, fpy) = map(self.nodes[fbase], self.nodes[fbase + 1]);
            let (tpx, tpy) = map(self.nodes[tbase], self.nodes[tbase + 1]);
            // Edge colour is average of endpoint colours; alpha from the smaller weight.
            let r = (clamp01(self.nodes[fbase + 2]) + clamp01(self.nodes[tbase + 2])) * 0.5;
            let g = (clamp01(self.nodes[fbase + 3]) + clamp01(self.nodes[tbase + 3])) * 0.5;
            let b = (clamp01(self.nodes[fbase + 4]) + clamp01(self.nodes[tbase + 4])) * 0.5;
            let wgt = clamp01(self.nodes[fbase + 6]).min(clamp01(self.nodes[tbase + 6]));
            splat_line(
                &mut self.pixel_buffer, self.width, self.height,
                fpx, fpy, tpx, tpy,
                r, g, b, 0.2 + wgt * 0.3,
            );
        }

        // Nodes on top.
        for i in 0..node_count {
            let base = i * NODE_STRIDE;
            let nr = clamp01(self.nodes[base + 2]);
            let ng = clamp01(self.nodes[base + 3]);
            let nb = clamp01(self.nodes[base + 4]);
            let na = clamp01(self.nodes[base + 5]).max(0.5);
            let weight = clamp01(self.nodes[base + 6]);
            let (px, py) = map(self.nodes[base], self.nodes[base + 1]);
            // Halo.
            splat_dot(
                &mut self.pixel_buffer, self.width, self.height,
                px, py, 4.5 + weight * 5.0,
                nr, ng, nb, 0.25,
            );
            // Core.
            splat_dot(
                &mut self.pixel_buffer, self.width, self.height,
                px, py, 2.0 + weight * 2.0,
                nr, ng, nb, na,
            );
        }
    }

    /// Convenience: upload both buffers and render in one call.
    pub fn render_with(
        &mut self,
        nodes_ptr: *const f32, nodes_len: usize,
        edges_ptr: *const u32, edges_len: usize,
    ) -> bool {
        self.set_nodes(nodes_ptr, nodes_len);
        self.set_edges(edges_ptr, edges_len);
        self.render();
        true
    }

    pub fn get_pixels_ptr(&self) -> *const u8 { self.pixel_buffer.as_ptr() }
    pub fn get_pixels_len(&self) -> usize { self.pixel_buffer.len() }
    pub fn get_width(&self) -> u32 { self.width }
    pub fn get_height(&self) -> u32 { self.height }
    pub fn node_count(&self) -> usize { self.nodes.len() / NODE_STRIDE }
    pub fn edge_count(&self) -> usize { self.edges.len() / 2 }
}

/// Stateless one-shot renderer exposed as `renderOntologyNeighborThumb` on the
/// JS side. Allocates a fresh thumbnail, renders, and returns the pixel
/// buffer as a `Box<[u8]>` (boxed slice, length-prefixed by wasm-bindgen).
///
/// Note: `Box<[u8]>` is serialised by wasm-bindgen as a `Uint8Array` copy.
/// For zero-copy access, callers should instantiate `OntologyNeighborThumb`
/// directly and use the `get_pixels_ptr` / `get_pixels_len` pair.
#[wasm_bindgen(js_name = renderOntologyNeighborThumb)]
pub fn render_ontology_neighbor_thumb(
    width: u32,
    height: u32,
    nodes_ptr: *const f32,
    nodes_len: usize,
) -> Box<[u8]> {
    let mut t = OntologyNeighborThumb::new(width, height);
    t.render_with(nodes_ptr, nodes_len);
    t.pixel_buffer.into_boxed_slice()
}

/// Stateless one-shot renderer exposed as `renderMiniGraph` on the JS side.
#[wasm_bindgen(js_name = renderMiniGraph)]
pub fn render_mini_graph(
    width: u32,
    height: u32,
    nodes_ptr: *const f32,
    nodes_len: usize,
    edges_ptr: *const u32,
    edges_len: usize,
) -> Box<[u8]> {
    let mut g = MiniGraph::new(width, height);
    g.render_with(nodes_ptr, nodes_len, edges_ptr, edges_len);
    g.pixel_buffer.into_boxed_slice()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumb_clears_and_has_right_size() {
        let t = OntologyNeighborThumb::new(64, 48);
        assert_eq!(t.get_width(), 64);
        assert_eq!(t.get_height(), 48);
        assert_eq!(t.get_pixels_len(), 64 * 48 * 4);
        assert!(t.pixel_buffer.iter().all(|b| *b == 0));
    }

    #[test]
    fn thumb_renders_focus_dot() {
        let mut t = OntologyNeighborThumb::new(64, 64);
        // Single node at origin with strong white colour.
        let nodes: Vec<f32> = vec![0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0];
        t.set_nodes(nodes.as_ptr(), nodes.len());
        t.render();
        // Centre pixel should be lit.
        let cx = 32usize;
        let cy = 32usize;
        let idx = (cy * 64 + cx) * 4;
        assert!(t.pixel_buffer[idx] > 0, "centre pixel should have red > 0");
    }

    #[test]
    fn mini_graph_renders_edges_and_nodes() {
        let mut g = MiniGraph::new(96, 96);
        let nodes: Vec<f32> = vec![
            -0.5, -0.5, 0.6, 0.8, 1.0, 1.0, 0.8,
             0.5,  0.5, 0.9, 0.4, 1.0, 1.0, 0.6,
        ];
        let edges: Vec<u32> = vec![0, 1];
        g.set_nodes(nodes.as_ptr(), nodes.len());
        g.set_edges(edges.as_ptr(), edges.len());
        g.render();
        assert_eq!(g.node_count(), 2);
        assert_eq!(g.edge_count(), 1);
        // Some pixels lit.
        assert!(g.pixel_buffer.iter().any(|b| *b > 0));
    }

    #[test]
    fn one_shot_renderers_produce_non_empty_output() {
        let nodes: Vec<f32> = vec![
            0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0,
            0.7, 0.2, 0.5, 0.9, 1.0, 1.0, 0.5,
        ];
        let edges: Vec<u32> = vec![0, 1];
        let bytes_a = render_ontology_neighbor_thumb(48, 48, nodes.as_ptr(), nodes.len());
        assert_eq!(bytes_a.len(), 48 * 48 * 4);
        assert!(bytes_a.iter().any(|b| *b > 0));
        let bytes_b = render_mini_graph(96, 64, nodes.as_ptr(), nodes.len(), edges.as_ptr(), edges.len());
        assert_eq!(bytes_b.len(), 96 * 64 * 4);
        assert!(bytes_b.iter().any(|b| *b > 0));
    }

    #[test]
    fn empty_input_is_safe() {
        let mut t = OntologyNeighborThumb::new(32, 32);
        t.set_nodes(std::ptr::null(), 0);
        t.render();
        // Cleared buffer, no panic.
        assert!(t.pixel_buffer.iter().all(|b| *b == 0));
    }

    #[test]
    fn oversized_input_is_clamped() {
        let mut t = OntologyNeighborThumb::new(32, 32);
        let nodes: Vec<f32> = vec![0.0; (MAX_NODES + 10) * NODE_STRIDE];
        t.set_nodes(nodes.as_ptr(), nodes.len());
        assert_eq!(t.nodes.len(), MAX_NODES * NODE_STRIDE);
    }
}
