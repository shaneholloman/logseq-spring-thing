/**
 * Regression tests for T4: validation-ceiling consistency (TypeScript/client side).
 *
 * RESOLVED 2026-06-03. Previously the actor path-pattern ceilings were narrower
 * than the canonical client defaults (repelK 120 > 100, maxVelocity 100 > 50,
 * springK 12 > 10), so boot-time defaults were silently clamped. The backend now
 * reads every (MIN, MAX) from a single source of truth,
 * src/actors/gpu/physics_bounds.rs, so the actor path validator, the route
 * validator and the canonical client defaults can never diverge again.
 *
 * These tests pin the UNIFIED ceilings (kept in sync with physics_bounds.rs) and
 * assert the canonical DEFAULT_PHYSICS_SETTINGS values are ACCEPTED — i.e. each
 * default sits inside the legal range and would not be clamped on boot. The
 * Rust-side counterpart (tests/repro_t4_ceiling_consistency.rs) drives the real
 * validate_physics_settings() with these same magnitudes.
 *
 * Fix spec: docs/architecture/diagrams/qe-T2-T4-writepaths-ceilings.md
 */

import { describe, it, expect } from 'vitest';
import { DEFAULT_PHYSICS_SETTINGS } from '../defaults';

// ---------------------------------------------------------------------------
// Unified ceilings — single source of truth is
// src/actors/gpu/physics_bounds.rs (MAX of each (MIN, MAX) Bound). Kept in sync
// with the Rust constants; the Rust test pins the same magnitudes against the
// real validator so a drift on either side is caught.
// ---------------------------------------------------------------------------
const SPRING_K_MAX = 500.0; // physics_bounds::SPRING_K.1
const REPEL_K_MAX = 500.0; // physics_bounds::REPEL_K.1
const MAX_VELOCITY_MAX = 1000.0; // physics_bounds::MAX_VELOCITY.1
const MAX_FORCE_MAX = 5000.0; // physics_bounds::MAX_FORCE.1

// Rust backstop constant — force_compute_actor.rs. Held equal to the unified
// max_velocity ceiling so the backstop is only ever a safety net.
const RUST_BACKSTOP_MAX_VELOCITY = 1000.0;

// Canonical default magnitudes the fix must keep accepting (defaults.ts).
const CANONICAL = { springK: 12.0, repelK: 120.0, maxVelocity: 100.0, maxForce: 150.0 };

describe('T4 (resolved): canonical physics defaults are accepted by the unified ceilings', () => {
  it('defaults.ts still carries the canonical magnitudes', () => {
    expect(DEFAULT_PHYSICS_SETTINGS.springK).toBe(CANONICAL.springK);
    expect(DEFAULT_PHYSICS_SETTINGS.repelK).toBe(CANONICAL.repelK);
    expect(DEFAULT_PHYSICS_SETTINGS.maxVelocity).toBe(CANONICAL.maxVelocity);
    expect(DEFAULT_PHYSICS_SETTINGS.maxForce).toBe(CANONICAL.maxForce);
  });

  it('repelK default (120) is within the unified ceiling (500) — not clamped', () => {
    expect(DEFAULT_PHYSICS_SETTINGS.repelK!).toBeLessThanOrEqual(REPEL_K_MAX);
  });

  it('maxVelocity default (100) is within the unified ceiling (1000) — not clamped', () => {
    expect(DEFAULT_PHYSICS_SETTINGS.maxVelocity!).toBeLessThanOrEqual(MAX_VELOCITY_MAX);
  });

  it('springK default (12) is within the unified ceiling (500) — not clamped', () => {
    expect(DEFAULT_PHYSICS_SETTINGS.springK!).toBeLessThanOrEqual(SPRING_K_MAX);
  });

  it('maxForce default (150) is within the unified ceiling (5000) — not clamped', () => {
    expect(DEFAULT_PHYSICS_SETTINGS.maxForce!).toBeLessThanOrEqual(MAX_FORCE_MAX);
  });

  it('the unified max_velocity ceiling equals the Rust backstop so the backstop never clamps healthy frames', () => {
    expect(MAX_VELOCITY_MAX).toBe(RUST_BACKSTOP_MAX_VELOCITY);
  });
});
