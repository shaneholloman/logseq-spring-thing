import { describe, it, expect } from 'vitest';
import { clamp, validatePhysicsSettings, validateConstraintSettings } from '../validators';

describe('clamp', () => {
  it('returns value when within range', () => {
    expect(clamp(5, 0, 10)).toBe(5);
  });

  it('clamps to min', () => {
    expect(clamp(-5, 0, 10)).toBe(0);
  });

  it('clamps to max', () => {
    expect(clamp(15, 0, 10)).toBe(10);
  });

  it('handles equal min and max', () => {
    expect(clamp(7, 5, 5)).toBe(5);
  });

  it('handles floating point values', () => {
    expect(clamp(0.5, 0.0, 1.0)).toBeCloseTo(0.5);
    expect(clamp(1.5, 0.0, 1.0)).toBeCloseTo(1.0);
  });

  it('handles NaN by returning NaN (pass-through)', () => {
    expect(isNaN(clamp(NaN, 0, 10))).toBe(true);
  });
});

describe('validatePhysicsSettings', () => {
  it('returns null for valid settings', () => {
    expect(validatePhysicsSettings({ damping: 0.5, boundsSize: 100, maxVelocity: 10 })).toBeNull();
  });

  it('returns null for empty settings', () => {
    expect(validatePhysicsSettings({})).toBeNull();
  });

  it('rejects damping < 0', () => {
    expect(validatePhysicsSettings({ damping: -0.1 })).toMatch(/damping/i);
  });

  it('rejects damping > 1', () => {
    expect(validatePhysicsSettings({ damping: 1.1 })).toMatch(/damping/i);
  });

  it('accepts damping at boundary values 0 and 1', () => {
    expect(validatePhysicsSettings({ damping: 0 })).toBeNull();
    expect(validatePhysicsSettings({ damping: 1 })).toBeNull();
  });

  it('rejects boundsSize <= 0', () => {
    expect(validatePhysicsSettings({ boundsSize: 0 })).toMatch(/bounds/i);
    expect(validatePhysicsSettings({ boundsSize: -1 })).toMatch(/bounds/i);
  });

  it('rejects maxVelocity <= 0', () => {
    expect(validatePhysicsSettings({ maxVelocity: 0 })).toMatch(/velocity/i);
    expect(validatePhysicsSettings({ maxVelocity: -10 })).toMatch(/velocity/i);
  });
});

describe('validateConstraintSettings', () => {
  it('returns null for valid settings', () => {
    expect(validateConstraintSettings({ activationFrames: 60, farThreshold: 100, mediumThreshold: 50, nearThreshold: 10 })).toBeNull();
  });

  it('rejects activationFrames < 1', () => {
    expect(validateConstraintSettings({ activationFrames: 0 })).toMatch(/activation/i);
  });

  it('rejects activationFrames > 600', () => {
    expect(validateConstraintSettings({ activationFrames: 601 })).toMatch(/activation/i);
  });

  it('accepts boundary values 1 and 600', () => {
    expect(validateConstraintSettings({ activationFrames: 1 })).toBeNull();
    expect(validateConstraintSettings({ activationFrames: 600 })).toBeNull();
  });

  it('rejects negative threshold values', () => {
    expect(validateConstraintSettings({ farThreshold: -1 })).toMatch(/far/i);
    expect(validateConstraintSettings({ mediumThreshold: -1 })).toMatch(/medium/i);
    expect(validateConstraintSettings({ nearThreshold: -1 })).toMatch(/near/i);
  });

  it('accepts zero thresholds', () => {
    expect(validateConstraintSettings({ farThreshold: 0 })).toBeNull();
    expect(validateConstraintSettings({ nearThreshold: 0 })).toBeNull();
  });
});
