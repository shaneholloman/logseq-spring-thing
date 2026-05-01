//! ADR-069 D6: 60-frame preset transition ease-in mechanism.
//!
//! When a `ForcePreset` changes mid-run the old `SimParams` are snapshotted
//! and linearly interpolated toward the new values over 60 consecutive frames
//! (~1 s at 60 Hz). During the transition damping is clamped to
//! `min(lerp_damping, 0.9)` to prevent energy spikes.
//!
//! A 1-second debounce window coalesces rapid preset switches so the GPU
//! only receives one smooth ramp per burst of user activity.

use std::time::{Duration, Instant};

use crate::models::simulation_params::SimParams;
use graph_cognition_physics_presets::{PresetConfig, StabilityConfig};

/// Debounce window: any preset change arriving within this duration of the
/// previous change start is queued and coalesced.
const DEBOUNCE_WINDOW: Duration = Duration::from_secs(1);

/// Default number of interpolation frames (~1 s at 60 Hz).
const DEFAULT_TOTAL_FRAMES: u32 = 60;

/// Maximum damping value allowed during a transition to prevent energy spikes.
const TRANSITION_DAMPING_CAP: f32 = 0.9;

// ---------------------------------------------------------------------------
// SimParamsSnapshot — the interpolatable subset of SimParams
// ---------------------------------------------------------------------------

/// Captures the numeric fields of [`SimParams`] that participate in preset
/// interpolation. Non-numeric, integer, and GPU-internal fields (feature_flags,
/// seed, iteration, warmup_iterations, etc.) are excluded because they must
/// snap discretely rather than be linearly blended.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SimParamsSnapshot {
    pub dt: f32,
    pub damping: f32,
    pub cooling_rate: f32,
    pub spring_k: f32,
    pub rest_length: f32,
    pub repel_k: f32,
    pub repulsion_cutoff: f32,
    pub repulsion_softening_epsilon: f32,
    pub center_gravity_k: f32,
    pub max_force: f32,
    pub max_velocity: f32,
    pub grid_cell_size: f32,
    pub separation_radius: f32,
    pub cluster_strength: f32,
    pub alignment_strength: f32,
    pub temperature: f32,
    pub viewport_bounds: f32,
    pub boundary_damping: f32,
    pub gravity: f32,
    pub scaling_ratio: f32,
    pub global_speed: f32,
    pub z_damping: f32,
}

impl SimParamsSnapshot {
    /// Extract the interpolatable fields from a GPU-aligned [`SimParams`].
    pub fn from_sim_params(p: &SimParams) -> Self {
        Self {
            dt: p.dt,
            damping: p.damping,
            cooling_rate: p.cooling_rate,
            spring_k: p.spring_k,
            rest_length: p.rest_length,
            repel_k: p.repel_k,
            repulsion_cutoff: p.repulsion_cutoff,
            repulsion_softening_epsilon: p.repulsion_softening_epsilon,
            center_gravity_k: p.center_gravity_k,
            max_force: p.max_force,
            max_velocity: p.max_velocity,
            grid_cell_size: p.grid_cell_size,
            separation_radius: p.separation_radius,
            cluster_strength: p.cluster_strength,
            alignment_strength: p.alignment_strength,
            temperature: p.temperature,
            viewport_bounds: p.viewport_bounds,
            boundary_damping: p.boundary_damping,
            gravity: p.gravity,
            scaling_ratio: p.scaling_ratio,
            global_speed: p.global_speed,
            z_damping: p.z_damping,
        }
    }

    /// Build a snapshot from a [`PresetConfig`]'s `global` block.
    ///
    /// Fields that `GlobalSimConfig` does not carry are set to sensible
    /// defaults matching [`SimParams::new()`].
    pub fn from_preset_config(config: &PresetConfig) -> Self {
        let g = &config.global;
        Self {
            dt: g.dt,
            damping: g.damping,
            cooling_rate: 0.999,
            spring_k: g.spring_k,
            rest_length: g.rest_length,
            repel_k: g.repel_k,
            repulsion_cutoff: g.max_velocity * 10.0,
            repulsion_softening_epsilon: 0.1,
            center_gravity_k: g.central_gravity,
            max_force: g.max_force,
            max_velocity: g.max_velocity,
            grid_cell_size: 40.0,
            separation_radius: 2.0,
            cluster_strength: 0.5,
            alignment_strength: 0.3,
            temperature: 1.0,
            viewport_bounds: 1000.0,
            boundary_damping: 0.9,
            gravity: g.gravity,
            scaling_ratio: 10.0,
            global_speed: g.dt * 10.0,
            z_damping: 0.0,
        }
    }

    /// Element-wise linear interpolation: `self * (1-t) + other * t`.
    pub fn lerp(&self, other: &Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let mix = |a: f32, b: f32| a + (b - a) * t;
        Self {
            dt: mix(self.dt, other.dt),
            damping: mix(self.damping, other.damping),
            cooling_rate: mix(self.cooling_rate, other.cooling_rate),
            spring_k: mix(self.spring_k, other.spring_k),
            rest_length: mix(self.rest_length, other.rest_length),
            repel_k: mix(self.repel_k, other.repel_k),
            repulsion_cutoff: mix(self.repulsion_cutoff, other.repulsion_cutoff),
            repulsion_softening_epsilon: mix(
                self.repulsion_softening_epsilon,
                other.repulsion_softening_epsilon,
            ),
            center_gravity_k: mix(self.center_gravity_k, other.center_gravity_k),
            max_force: mix(self.max_force, other.max_force),
            max_velocity: mix(self.max_velocity, other.max_velocity),
            grid_cell_size: mix(self.grid_cell_size, other.grid_cell_size),
            separation_radius: mix(self.separation_radius, other.separation_radius),
            cluster_strength: mix(self.cluster_strength, other.cluster_strength),
            alignment_strength: mix(self.alignment_strength, other.alignment_strength),
            temperature: mix(self.temperature, other.temperature),
            viewport_bounds: mix(self.viewport_bounds, other.viewport_bounds),
            boundary_damping: mix(self.boundary_damping, other.boundary_damping),
            gravity: mix(self.gravity, other.gravity),
            scaling_ratio: mix(self.scaling_ratio, other.scaling_ratio),
            global_speed: mix(self.global_speed, other.global_speed),
            z_damping: mix(self.z_damping, other.z_damping),
        }
    }

    /// Apply the ADR-069 D6 damping clamp: during transitions, damping must
    /// not exceed `min(old.damping, new.damping, 0.9)`.
    pub fn with_transition_damping_clamp(&self, old: &Self, new: &Self) -> Self {
        let cap = old.damping.min(new.damping).min(TRANSITION_DAMPING_CAP);
        let mut out = *self;
        out.damping = out.damping.min(cap);
        out
    }

    /// Write the snapshot fields back into a mutable [`SimParams`].
    ///
    /// Integer/flag fields on `target` are left untouched.
    pub fn apply_to(&self, target: &mut SimParams) {
        target.dt = self.dt;
        target.damping = self.damping;
        target.cooling_rate = self.cooling_rate;
        target.spring_k = self.spring_k;
        target.rest_length = self.rest_length;
        target.repel_k = self.repel_k;
        target.repulsion_cutoff = self.repulsion_cutoff;
        target.repulsion_softening_epsilon = self.repulsion_softening_epsilon;
        target.center_gravity_k = self.center_gravity_k;
        target.max_force = self.max_force;
        target.max_velocity = self.max_velocity;
        target.grid_cell_size = self.grid_cell_size;
        target.separation_radius = self.separation_radius;
        target.cluster_strength = self.cluster_strength;
        target.alignment_strength = self.alignment_strength;
        target.temperature = self.temperature;
        target.viewport_bounds = self.viewport_bounds;
        target.boundary_damping = self.boundary_damping;
        target.gravity = self.gravity;
        target.scaling_ratio = self.scaling_ratio;
        target.global_speed = self.global_speed;
        target.z_damping = self.z_damping;
    }
}

// ---------------------------------------------------------------------------
// PresetTransition — the 60-frame ease-in state machine
// ---------------------------------------------------------------------------

/// Manages smooth interpolation between two parameter snapshots over a fixed
/// number of frames, with debounce coalescing for rapid preset changes.
#[derive(Debug)]
pub struct PresetTransition {
    old_params: SimParamsSnapshot,
    new_params: SimParamsSnapshot,
    /// Current frame within the transition (0..total_frames).
    frame: u32,
    /// Total interpolation frames (default 60).
    total_frames: u32,
    /// Whether a transition is currently in progress.
    active: bool,
    /// Timestamp of the most recent transition start.
    last_change_at: Instant,
    /// A queued preset change that arrived during the debounce window.
    pending: Option<SimParamsSnapshot>,
    /// Per-preset stability thresholds from the active preset (ADR-069 D7).
    stability: StabilityConfig,
}

impl PresetTransition {
    /// Create a new inactive transition controller.
    pub fn new() -> Self {
        let zero = SimParamsSnapshot::from_sim_params(&SimParams::new());
        Self {
            old_params: zero,
            new_params: zero,
            frame: 0,
            total_frames: DEFAULT_TOTAL_FRAMES,
            active: false,
            last_change_at: Instant::now() - DEBOUNCE_WINDOW * 2,
            pending: None,
            stability: StabilityConfig {
                velocity_epsilon: 0.05,
                force_epsilon: 0.01,
                max_iterations: 2000,
            },
        }
    }

    /// Create with a custom frame count (for testing).
    #[cfg(test)]
    pub fn with_total_frames(total_frames: u32) -> Self {
        let mut t = Self::new();
        t.total_frames = total_frames;
        t
    }

    /// Begin a transition from `old` to `new`, resetting the frame counter.
    pub fn begin(&mut self, old: SimParamsSnapshot, new: SimParamsSnapshot) {
        self.old_params = old;
        self.new_params = new;
        self.frame = 0;
        self.active = true;
        self.last_change_at = Instant::now();
        self.pending = None;
    }

    /// Request a preset change with debounce logic.
    ///
    /// If a transition started within the last [`DEBOUNCE_WINDOW`], the new
    /// target is queued as pending. Once the current transition completes (or
    /// is replaced), the pending target becomes the new endpoint.
    ///
    /// If no transition is active or the debounce window has elapsed, the
    /// transition begins immediately.
    pub fn request_change(
        &mut self,
        current_params: SimParamsSnapshot,
        new_params: SimParamsSnapshot,
    ) {
        let now = Instant::now();
        if self.active && now.duration_since(self.last_change_at) < DEBOUNCE_WINDOW {
            // Within debounce window — coalesce: replace the pending target.
            // The transition will retarget on the next tick or when the current
            // transition completes.
            self.pending = Some(new_params);
        } else {
            // Outside window or no active transition — start immediately.
            let old = if self.active {
                // Capture current interpolated state as the starting point.
                self.interpolated_now()
            } else {
                current_params
            };
            self.begin(old, new_params);
        }
    }

    /// Advance by one frame and return the interpolated snapshot, or `None`
    /// if no transition is active.
    ///
    /// The returned snapshot has the ADR-069 damping clamp applied.
    /// After `total_frames` ticks the transition deactivates and snaps to
    /// `new_params`. If a pending change exists it is promoted at that point.
    pub fn tick(&mut self) -> Option<SimParamsSnapshot> {
        if !self.active {
            return None;
        }

        self.frame += 1;

        if self.frame >= self.total_frames {
            let final_params = self.new_params;

            // Check for coalesced pending change.
            if let Some(pending_new) = self.pending.take() {
                self.begin(final_params, pending_new);
                // Return the first frame of the new ramp.
                return self.tick();
            }

            self.active = false;
            return Some(final_params);
        }

        let t = self.frame as f32 / self.total_frames as f32;
        let interpolated = self.old_params.lerp(&self.new_params, t);
        let clamped =
            interpolated.with_transition_damping_clamp(&self.old_params, &self.new_params);
        Some(clamped)
    }

    /// Whether a transition is currently in progress.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Peek at the current interpolated state without advancing the frame.
    pub fn interpolated_now(&self) -> SimParamsSnapshot {
        if !self.active || self.total_frames == 0 {
            return self.new_params;
        }
        let t = self.frame as f32 / self.total_frames as f32;
        self.old_params.lerp(&self.new_params, t)
    }

    /// Update stability thresholds from a preset config (ADR-069 D7).
    pub fn set_stability(&mut self, config: &PresetConfig) {
        self.stability = config.stability.clone();
    }

    /// Returns the active stability thresholds.
    pub fn stability(&self) -> &StabilityConfig {
        &self.stability
    }

    /// Check whether the simulation has settled according to the active
    /// preset's stability thresholds.
    pub fn is_settled(&self, max_velocity: f32, max_force: f32, iteration: u32) -> bool {
        if self.active {
            return false;
        }
        max_velocity < self.stability.velocity_epsilon
            && max_force < self.stability.force_epsilon
            || iteration >= self.stability.max_iterations
    }

    /// Number of frames remaining in the current transition.
    pub fn frames_remaining(&self) -> u32 {
        if !self.active {
            return 0;
        }
        self.total_frames.saturating_sub(self.frame)
    }
}

impl Default for PresetTransition {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    /// Build a snapshot with all fields set to a constant.
    fn uniform_snapshot(v: f32) -> SimParamsSnapshot {
        SimParamsSnapshot {
            dt: v,
            damping: v,
            cooling_rate: v,
            spring_k: v,
            rest_length: v,
            repel_k: v,
            repulsion_cutoff: v,
            repulsion_softening_epsilon: v,
            center_gravity_k: v,
            max_force: v,
            max_velocity: v,
            grid_cell_size: v,
            separation_radius: v,
            cluster_strength: v,
            alignment_strength: v,
            temperature: v,
            viewport_bounds: v,
            boundary_damping: v,
            gravity: v,
            scaling_ratio: v,
            global_speed: v,
            z_damping: v,
        }
    }

    fn approx_eq(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    // ── Basic interpolation ─────────────────────────────────────────────

    #[test]
    fn frame_0_returns_old_params() {
        let old = uniform_snapshot(1.0);
        let new = uniform_snapshot(2.0);
        let mut t = PresetTransition::with_total_frames(60);
        t.begin(old, new);

        // First tick is frame 1/60, so t = 1/60. Frame 0 is "before any tick".
        // The tick at frame=1 should be very close to old.
        let snap = t.tick().unwrap();
        // At t=1/60 ≈ 0.0167, spring_k should be ~1.0167
        assert!(approx_eq(snap.spring_k, 1.0 + 1.0 / 60.0));
    }

    #[test]
    fn frame_60_returns_new_params() {
        let old = uniform_snapshot(1.0);
        let new = uniform_snapshot(2.0);
        let mut t = PresetTransition::with_total_frames(60);
        t.begin(old, new);

        let mut last = None;
        for _ in 0..60 {
            last = t.tick();
        }
        let snap = last.unwrap();
        // Final frame snaps to new params.
        assert!(approx_eq(snap.spring_k, 2.0));
        assert!(approx_eq(snap.repel_k, 2.0));
        assert!(approx_eq(snap.dt, 2.0));
    }

    #[test]
    fn midpoint_interpolation() {
        let old = uniform_snapshot(0.0);
        let new = uniform_snapshot(100.0);
        let mut t = PresetTransition::with_total_frames(60);
        t.begin(old, new);

        // Advance to frame 30 (midpoint).
        let mut snap = None;
        for _ in 0..30 {
            snap = t.tick();
        }
        let s = snap.unwrap();
        // t = 30/60 = 0.5, so each field should be 50.0.
        assert!(approx_eq(s.spring_k, 50.0));
        assert!(approx_eq(s.repel_k, 50.0));
        assert!(approx_eq(s.max_velocity, 50.0));
    }

    // ── Damping clamp ───────────────────────────────────────────────────

    #[test]
    fn damping_clamped_during_transition() {
        // old.damping = 0.95, new.damping = 0.98
        // Midpoint lerp would be 0.965 — but cap is min(0.95, 0.98, 0.9) = 0.9
        let mut old = uniform_snapshot(0.5);
        old.damping = 0.95;
        let mut new = uniform_snapshot(0.5);
        new.damping = 0.98;

        let mut t = PresetTransition::with_total_frames(60);
        t.begin(old, new);

        for _ in 0..30 {
            let snap = t.tick().unwrap();
            assert!(
                snap.damping <= TRANSITION_DAMPING_CAP + 1e-6,
                "damping {} exceeds cap {}",
                snap.damping,
                TRANSITION_DAMPING_CAP,
            );
        }
    }

    #[test]
    fn damping_below_cap_passes_through() {
        // Both dampings below 0.9 — cap is min(0.5, 0.6, 0.9) = 0.5
        let mut old = uniform_snapshot(0.5);
        old.damping = 0.5;
        let mut new = uniform_snapshot(0.5);
        new.damping = 0.6;

        let mut t = PresetTransition::with_total_frames(60);
        t.begin(old, new);

        let snap = t.tick().unwrap(); // frame 1/60
                                      // lerp damping = 0.5 + 0.1*(1/60) ≈ 0.5017
                                      // cap = min(0.5, 0.6, 0.9) = 0.5
                                      // So damping should be clamped to 0.5.
        assert!(
            snap.damping <= 0.5 + 1e-6,
            "damping {} exceeds min(old, new) = 0.5",
            snap.damping,
        );
    }

    // ── Inactive state ──────────────────────────────────────────────────

    #[test]
    fn inactive_before_begin() {
        let t = PresetTransition::new();
        assert!(!t.is_active());
    }

    #[test]
    fn tick_returns_none_when_inactive() {
        let mut t = PresetTransition::new();
        assert!(t.tick().is_none());
    }

    #[test]
    fn deactivates_after_completion() {
        let mut t = PresetTransition::with_total_frames(10);
        t.begin(uniform_snapshot(0.0), uniform_snapshot(1.0));
        for _ in 0..10 {
            t.tick();
        }
        assert!(!t.is_active());
        assert!(t.tick().is_none());
    }

    // ── Debounce coalescing ─────────────────────────────────────────────

    #[test]
    fn rapid_changes_coalesce_to_pending() {
        let mut t = PresetTransition::with_total_frames(60);
        let current = uniform_snapshot(0.0);
        let first = uniform_snapshot(1.0);
        let second = uniform_snapshot(2.0);
        let third = uniform_snapshot(3.0);

        // First change starts immediately.
        t.request_change(current, first);
        assert!(t.is_active());

        // Rapid follow-ups within 1s should queue as pending.
        t.request_change(current, second);
        assert!(t.pending.is_some());

        // Another rapid change replaces the pending target.
        t.request_change(current, third);
        let pending = t.pending.unwrap();
        assert!(approx_eq(pending.spring_k, 3.0));
    }

    #[test]
    fn pending_promoted_after_transition_completes() {
        let mut t = PresetTransition::with_total_frames(5);
        let current = uniform_snapshot(0.0);
        let first = uniform_snapshot(1.0);
        let second = uniform_snapshot(2.0);

        t.request_change(current, first);
        t.request_change(current, second); // queued as pending

        // Drain the first transition.
        for _ in 0..5 {
            t.tick();
        }

        // The pending should have been promoted — transition should be active
        // again, targeting second.
        assert!(t.is_active());

        // Drain second transition.
        let mut last = None;
        for _ in 0..5 {
            last = t.tick();
        }
        let final_snap = last.unwrap();
        assert!(approx_eq(final_snap.spring_k, 2.0));
        assert!(!t.is_active());
    }

    #[test]
    fn change_after_debounce_window_starts_immediately() {
        let mut t = PresetTransition::with_total_frames(5);
        let current = uniform_snapshot(0.0);
        let first = uniform_snapshot(1.0);

        t.request_change(current, first);
        // Drain the transition.
        for _ in 0..5 {
            t.tick();
        }
        assert!(!t.is_active());

        // Sleep past the debounce window.
        thread::sleep(Duration::from_millis(1100));

        let second = uniform_snapshot(5.0);
        let snapshot_now = uniform_snapshot(1.0);
        t.request_change(snapshot_now, second);
        assert!(t.is_active());
        assert!(t.pending.is_none());
    }

    // ── Snapshot conversion round-trips ─────────────────────────────────

    #[test]
    fn from_sim_params_roundtrip() {
        let params = SimParams::new();
        let snap = SimParamsSnapshot::from_sim_params(&params);
        let mut target = SimParams::new();
        snap.apply_to(&mut target);

        assert!(approx_eq(target.dt, params.dt));
        assert!(approx_eq(target.damping, params.damping));
        assert!(approx_eq(target.spring_k, params.spring_k));
        assert!(approx_eq(target.repel_k, params.repel_k));
        assert!(approx_eq(target.max_force, params.max_force));
        assert!(approx_eq(target.max_velocity, params.max_velocity));
        assert!(approx_eq(target.gravity, params.gravity));
        assert!(approx_eq(target.z_damping, params.z_damping));
    }

    #[test]
    fn from_preset_config_extracts_globals() {
        let cfg = PresetConfig::default_preset();
        let snap = SimParamsSnapshot::from_preset_config(&cfg);
        assert!(approx_eq(snap.dt, cfg.global.dt));
        assert!(approx_eq(snap.damping, cfg.global.damping));
        assert!(approx_eq(snap.spring_k, cfg.global.spring_k));
        assert!(approx_eq(snap.repel_k, cfg.global.repel_k));
        assert!(approx_eq(snap.max_force, cfg.global.max_force));
        assert!(approx_eq(snap.max_velocity, cfg.global.max_velocity));
        assert!(approx_eq(snap.gravity, cfg.global.gravity));
        assert!(approx_eq(snap.center_gravity_k, cfg.global.central_gravity));
    }

    // ── Lerp edge cases ─────────────────────────────────────────────────

    #[test]
    fn lerp_t_zero_returns_self() {
        let a = uniform_snapshot(10.0);
        let b = uniform_snapshot(20.0);
        let result = a.lerp(&b, 0.0);
        assert!(approx_eq(result.spring_k, 10.0));
    }

    #[test]
    fn lerp_t_one_returns_other() {
        let a = uniform_snapshot(10.0);
        let b = uniform_snapshot(20.0);
        let result = a.lerp(&b, 1.0);
        assert!(approx_eq(result.spring_k, 20.0));
    }

    #[test]
    fn lerp_clamps_out_of_range_t() {
        let a = uniform_snapshot(0.0);
        let b = uniform_snapshot(100.0);
        // t > 1.0 should clamp to 1.0
        let result = a.lerp(&b, 2.0);
        assert!(approx_eq(result.spring_k, 100.0));
        // t < 0.0 should clamp to 0.0
        let result = a.lerp(&b, -1.0);
        assert!(approx_eq(result.spring_k, 0.0));
    }

    // ── frames_remaining ────────────────────────────────────────────────

    #[test]
    fn frames_remaining_tracks_progress() {
        let mut t = PresetTransition::with_total_frames(10);
        t.begin(uniform_snapshot(0.0), uniform_snapshot(1.0));
        assert_eq!(t.frames_remaining(), 10);
        t.tick();
        assert_eq!(t.frames_remaining(), 9);
        for _ in 0..9 {
            t.tick();
        }
        assert_eq!(t.frames_remaining(), 0);
    }

    #[test]
    fn frames_remaining_zero_when_inactive() {
        let t = PresetTransition::new();
        assert_eq!(t.frames_remaining(), 0);
    }
}
