//! drawer-fx: ambient flow-field caustic background for the enterprise drawer.
//!
//! Effect anatomy
//! --------------
//! 1. A procedural **value-noise flow field** (2D + a slow z-slice that
//!    advances with time) produces a divergence-free-ish vector field that
//!    advects ~N luminous particles.
//! 2. Each frame we do NOT clear the canvas; instead we paint a translucent
//!    dark rectangle over it. This produces short, self-decaying motion
//!    trails -- cheaper than storing trail history and gives the caustic feel.
//! 3. Particles are drawn as small additive radial gradients in the violet /
//!    cyan palette (cycled by their spawn hue).
//! 4. `pulse(x, y)` injects a transient attractor that decays exponentially.
//!    Particles near it get a radial velocity bias for ~0.8 s.
//!
//! Performance notes
//! -----------------
//! * Canvas2D is used intentionally -- the effect is ambient, the particle
//!   count is low (~100-400), and WebGPU init cost + fallback complexity are
//!   not worth it.
//! * Noise is plain 2D value noise with cosine interpolation, single octave
//!   with a second half-amplitude octave -- this is what you actually see,
//!   not classical Perlin (which needs gradient tables we'd rather avoid to
//!   keep the crate <400 lines).
//! * Particles are stored SoA-ish in a single `Vec<Particle>` for cache
//!   locality; we never allocate per frame after `new()`.

#![allow(clippy::needless_range_loop)]

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

// ------------------------------------------------------------------
// Tiny deterministic RNG (xorshift32) -- no rand crate needed.
// ------------------------------------------------------------------
struct Rng(u32);
impl Rng {
    fn new(seed: u32) -> Self { Self(seed | 1) }
    fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13; x ^= x >> 17; x ^= x << 5;
        self.0 = x; x
    }
    fn f32(&mut self) -> f32 { (self.next_u32() >> 8) as f32 / ((1u32 << 24) as f32) }
    fn range(&mut self, a: f32, b: f32) -> f32 { a + self.f32() * (b - a) }
}

// ------------------------------------------------------------------
// Value noise.
// A 256-entry hash grid; bilinear + cosine interpolation in 2D, linear
// slice interpolation in "time" (used as a z-axis).  Returns values in
// roughly [-1, 1].
// ------------------------------------------------------------------
const HASH_SIZE: usize = 256;
const HASH_MASK: i32 = 255;

struct ValueNoise { hash: [f32; HASH_SIZE] }

impl ValueNoise {
    fn new() -> Self {
        let mut rng = Rng::new(0xC0FFEE);
        let mut hash = [0.0f32; HASH_SIZE];
        for i in 0..HASH_SIZE { hash[i] = rng.range(-1.0, 1.0); }
        Self { hash }
    }

    #[inline]
    fn sample(&self, x: f32, y: f32) -> f32 {
        // Integer cell + fractional offset.
        let xi = x.floor() as i32;
        let yi = y.floor() as i32;
        let xf = x - xi as f32;
        let yf = y - yi as f32;
        // Cosine-smooth the interpolation weights -- gives rounder lobes
        // than raw linear, which is important because we're only running
        // one real octave.
        let sx = (1.0 - (xf * std::f32::consts::PI).cos()) * 0.5;
        let sy = (1.0 - (yf * std::f32::consts::PI).cos()) * 0.5;
        let h = |ix: i32, iy: i32| -> f32 {
            // Cheap 2D hash -> [0, 256)
            let k = ((ix & HASH_MASK) as usize).wrapping_mul(73856093)
                ^ ((iy & HASH_MASK) as usize).wrapping_mul(19349663);
            self.hash[k & (HASH_SIZE - 1)]
        };
        let a = h(xi,     yi);
        let b = h(xi + 1, yi);
        let c = h(xi,     yi + 1);
        let d = h(xi + 1, yi + 1);
        let ab = a + (b - a) * sx;
        let cd = c + (d - c) * sx;
        ab + (cd - ab) * sy
    }

    /// Fractal sum: octave 1 + half-amplitude octave 2. That's enough for
    /// visual complexity while keeping the inner loop tight.
    #[inline]
    fn fbm(&self, x: f32, y: f32) -> f32 {
        self.sample(x, y) + 0.5 * self.sample(x * 2.17, y * 2.17)
    }
}

// ------------------------------------------------------------------
// Palette -- harmonised with the app's violet/cyan accent.
// base_hue 270deg (violet) drifting through 190deg (cyan).
// Each particle picks its hue once at spawn and keeps it; small lifetime
// adds variety between frames.
// ------------------------------------------------------------------
fn hsl(h: f32, s: f32, l: f32, a: f32) -> String {
    format!("hsla({:.0}, {:.0}%, {:.0}%, {:.3})", h, s * 100.0, l * 100.0, a)
}

// ------------------------------------------------------------------
// Particle.
// ------------------------------------------------------------------
#[derive(Clone, Copy)]
struct Particle {
    x: f32, y: f32,
    vx: f32, vy: f32,
    hue: f32,     // degrees
    life: f32,    // 0..1 -- respawns when < 0
    size: f32,
}

// ------------------------------------------------------------------
// Pulse (cursor attractor).
// ------------------------------------------------------------------
#[derive(Clone, Copy)]
struct Pulse { x: f32, y: f32, t: f32 } // t is remaining seconds (<=0 = dead)

// ------------------------------------------------------------------
// The exposed WASM type.
// ------------------------------------------------------------------
#[wasm_bindgen]
pub struct DrawerFx {
    ctx: CanvasRenderingContext2d,
    width: f32,
    height: f32,
    particles: Vec<Particle>,
    noise: ValueNoise,
    t: f32,              // accumulated seconds
    rng: Rng,
    pulse: Pulse,
    quality: u8,         // 0/1/2
    target_count: usize, // derived from quality + area
}

#[wasm_bindgen]
impl DrawerFx {
    /// Construct, binding to a canvas by DOM id. Returns an error string if
    /// the canvas or its 2d context can't be obtained.
    #[wasm_bindgen(constructor)]
    pub fn new(canvas_id: &str, width: u32, height: u32) -> Result<DrawerFx, JsValue> {
        let window = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
        let document = window.document().ok_or_else(|| JsValue::from_str("no document"))?;
        let canvas = document
            .get_element_by_id(canvas_id)
            .ok_or_else(|| JsValue::from_str("canvas id not found"))?
            .dyn_into::<HtmlCanvasElement>()
            .map_err(|_| JsValue::from_str("element is not a canvas"))?;
        canvas.set_width(width);
        canvas.set_height(height);
        let ctx = canvas
            .get_context("2d")?
            .ok_or_else(|| JsValue::from_str("no 2d context"))?
            .dyn_into::<CanvasRenderingContext2d>()?;

        let mut this = DrawerFx {
            ctx,
            width: width as f32,
            height: height as f32,
            particles: Vec::new(),
            noise: ValueNoise::new(),
            t: 0.0,
            rng: Rng::new(0xA11CE),
            pulse: Pulse { x: 0.0, y: 0.0, t: 0.0 },
            quality: 1,
            target_count: 0,
        };
        this.recalc_target();
        this.seed_particles();
        Ok(this)
    }

    /// Change quality tier. Count is re-scaled on next tick; existing
    /// particles drift out rather than snap.
    pub fn set_quality(&mut self, q: u8) {
        self.quality = q.min(2);
        self.recalc_target();
    }

    /// Update canvas dimensions (call on ResizeObserver).
    pub fn resize(&mut self, w: u32, h: u32) {
        self.width = w as f32;
        self.height = h as f32;
        self.ctx.canvas().unwrap().set_width(w);
        self.ctx.canvas().unwrap().set_height(h);
        self.recalc_target();
    }

    /// Inject a cursor attractor. Decays over ~0.8 s.
    pub fn pulse(&mut self, x: f32, y: f32) {
        self.pulse = Pulse { x, y, t: 0.8 };
    }

    /// Advance one frame. `dt_ms` is the elapsed time since the previous
    /// tick (rAF delta).
    pub fn tick(&mut self, dt_ms: f64) {
        let dt = (dt_ms as f32 / 1000.0).min(0.05); // clamp big frame skips
        self.t += dt;

        self.adjust_population();
        self.paint_trail_fade();
        self.advect_and_draw(dt);

        if self.pulse.t > 0.0 { self.pulse.t -= dt; }
    }
}

// ------------------------------------------------------------------
// Private helpers (kept outside the #[wasm_bindgen] impl to avoid
// inflating the exposed surface).
// ------------------------------------------------------------------
impl DrawerFx {
    fn recalc_target(&mut self) {
        // Density baseline: ~1 particle per 3400 px at medium quality.
        // For a 900x600 drawer that's ~160 particles, well under budget.
        let area = self.width * self.height;
        let base = (area / 3400.0).max(40.0);
        let scale = match self.quality { 0 => 0.5, 2 => 1.75, _ => 1.0 };
        self.target_count = (base * scale).min(420.0) as usize;
    }

    fn seed_particles(&mut self) {
        self.particles.clear();
        for _ in 0..self.target_count {
            let p = Self::spawn_inner(&mut self.rng, self.width, self.height);
            self.particles.push(p);
        }
    }

    /// Free function form: takes rng + dims so callers can spawn while
    /// simultaneously holding &mut self.particles.
    fn spawn_inner(rng: &mut Rng, w: f32, h: f32) -> Particle {
        let x = rng.range(0.0, w);
        let y = rng.range(0.0, h);
        let h0 = rng.range(190.0, 290.0);
        let h1 = rng.range(230.0, 280.0);
        Particle {
            x, y,
            vx: 0.0, vy: 0.0,
            hue: (h0 + h1) * 0.5,
            life: rng.range(0.2, 1.0),
            size: rng.range(0.6, 1.8),
        }
    }

    fn spawn_edge_inner(rng: &mut Rng, w: f32, h: f32) -> Particle {
        let mut p = Self::spawn_inner(rng, w, h);
        match rng.next_u32() & 3 {
            0 => p.x = -4.0,
            1 => p.x = w + 4.0,
            2 => p.y = -4.0,
            _ => p.y = h + 4.0,
        }
        p.life = 1.0;
        p
    }

    fn adjust_population(&mut self) {
        while self.particles.len() < self.target_count {
            let p = Self::spawn_inner(&mut self.rng, self.width, self.height);
            self.particles.push(p);
        }
        if self.particles.len() > self.target_count {
            self.particles.truncate(self.target_count);
        }
    }

    /// Paint a translucent dark rect to fade previous trails.
    /// Alpha controls trail length -- lower = longer trails, higher CPU
    /// perception of motion but more flicker.
    fn paint_trail_fade(&self) {
        self.ctx.set_global_composite_operation("source-over").ok();
        // Deep indigo/near-black with slight violet cast -- matches the
        // frosted-glass drawer backdrop.
        self.ctx.set_fill_style_str("rgba(10, 8, 22, 0.14)");
        self.ctx.fill_rect(0.0, 0.0, self.width as f64, self.height as f64);
    }

    fn advect_and_draw(&mut self, dt: f32) {
        // Flow-field params. Spatial frequency tuned so one "cell" spans
        // ~180 px at quality=1.
        let freq = 1.0 / 180.0;
        let time_phase = self.t * 0.25;       // very slow evolution
        let max_speed = 42.0;                  // px/s
        let pulse_active = self.pulse.t > 0.0;

        // Additive blending for the glow dabs.
        self.ctx.set_global_composite_operation("lighter").ok();

        // Pull out &mut refs to avoid re-borrowing self inside the loop.
        let rng = &mut self.rng;
        let noise = &self.noise;
        let pulse = self.pulse;
        let width = self.width;
        let height = self.height;
        let ctx = &self.ctx;

        for p in self.particles.iter_mut() {
            // Sample noise gradient by finite differences to get a 2D flow
            // vector. Using (n(x+h)-n(x-h), n(y+h)-n(y-h)) gives a curl-ish
            // field when we rotate 90deg (below). That's the trick that
            // makes particles swirl instead of flowing straight.
            let nx = p.x * freq;
            let ny = p.y * freq + time_phase;
            let e = 0.6;
            let nx_r = noise.fbm(nx + e, ny);
            let nx_l = noise.fbm(nx - e, ny);
            let ny_u = noise.fbm(nx, ny + e);
            let ny_d = noise.fbm(nx, ny - e);
            // Rotate gradient by 90deg -> divergence-free-ish swirl.
            let fx = (ny_u - ny_d) * 0.5;
            let fy = -(nx_r - nx_l) * 0.5;

            // Target velocity from field, scaled.
            let mut tvx = fx * max_speed;
            let mut tvy = fy * max_speed;

            // Cursor pulse attraction (additive, decays with pulse.t).
            if pulse_active {
                let dx = pulse.x - p.x;
                let dy = pulse.y - p.y;
                let d2 = dx * dx + dy * dy + 1.0;
                let falloff = (-d2 / (140.0 * 140.0)).exp();
                let strength = 55.0 * pulse.t * falloff;
                let inv = 1.0 / d2.sqrt();
                tvx += dx * inv * strength;
                tvy += dy * inv * strength;
            }

            // Smoothly chase the target velocity (first-order lerp).
            let k = 1.0 - (-dt * 4.0).exp();
            p.vx += (tvx - p.vx) * k;
            p.vy += (tvy - p.vy) * k;

            // Integrate.
            p.x += p.vx * dt;
            p.y += p.vy * dt;
            p.life -= dt * 0.08;

            // Respawn on death or wander-off.
            if p.life <= 0.0
                || p.x < -10.0 || p.x > width + 10.0
                || p.y < -10.0 || p.y > height + 10.0
            {
                *p = Self::spawn_edge_inner(rng, width, height);
                continue;
            }

            // Draw: small additive dab. A full createRadialGradient per
            // particle would be expensive (~0.01ms each x hundreds);
            // instead we draw an arc with a hue-shifted fill and let the
            // "lighter" blend mode do the glow.
            let alpha = (p.life * 0.8).min(0.7);
            let core = hsl(p.hue, 0.85, 0.70, alpha);
            ctx.set_fill_style_str(&core);
            ctx.begin_path();
            ctx.arc(
                p.x as f64, p.y as f64,
                (p.size * 1.6) as f64,
                0.0, std::f64::consts::TAU,
            ).ok();
            ctx.fill();
        }

        // Rare highlight: one bright dab at pulse center while fresh.
        if pulse_active && pulse.t > 0.55 {
            let a = (pulse.t - 0.55) / 0.25;
            self.ctx.set_fill_style_str(&hsl(200.0, 0.9, 0.78, a.min(1.0) * 0.55));
            self.ctx.begin_path();
            self.ctx.arc(pulse.x as f64, pulse.y as f64, 12.0, 0.0, std::f64::consts::TAU).ok();
            self.ctx.fill();
        }
    }
}

/// Crate version (diagnostic).
#[wasm_bindgen]
pub fn version() -> String { env!("CARGO_PKG_VERSION").to_string() }

/// WASM init: nicer panics in devtools.
#[wasm_bindgen(start)]
pub fn start() {
    std::panic::set_hook(Box::new(|info| {
        web_sys::console::error_1(&info.to_string().into());
    }));
}
