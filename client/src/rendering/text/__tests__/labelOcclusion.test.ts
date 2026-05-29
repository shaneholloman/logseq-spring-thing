import { describe, it, expect } from 'vitest';
import { computeOcclusionMask, type OcclusionCandidate } from '../labelOcclusion';

const c = (
  screenX: number,
  screenY: number,
  screenRadius: number,
  distance: number,
): OcclusionCandidate => ({ screenX, screenY, screenRadius, distance });

describe('computeOcclusionMask', () => {
  it('returns all-visible for a single candidate', () => {
    expect(computeOcclusionMask([c(0, 0, 10, 5)])).toEqual([false]);
  });

  it('hides a far node whose centre falls inside a nearer node disc', () => {
    // index 0 far (distance 20), index 1 near (distance 5) overlapping at same screen point
    const mask = computeOcclusionMask([c(100, 100, 8, 20), c(100, 100, 20, 5)]);
    expect(mask[0]).toBe(true); // far node occluded by the near one
    expect(mask[1]).toBe(false); // near node is never occluded by a farther one
  });

  it('keeps a far node visible when it lies outside every nearer disc', () => {
    const mask = computeOcclusionMask([c(0, 0, 8, 20), c(500, 500, 20, 5)]);
    expect(mask).toEqual([false, false]);
  });

  it('does not occlude co-planar nodes within the depth bias', () => {
    // Both at near-equal distance, fully overlapping — neither should hide the other.
    const mask = computeOcclusionMask([c(100, 100, 30, 10), c(100, 100, 30, 10.2)], 0.5);
    expect(mask).toEqual([false, false]);
  });

  it('respects the depth bias threshold (blocker must be clearly in front)', () => {
    // distance gap of 0.4 < bias 0.5 → not an occluder despite overlap.
    expect(computeOcclusionMask([c(0, 0, 5, 5.0), c(0, 0, 50, 4.6)], 0.5)[0]).toBe(false);
    // distance gap of 1.0 > bias → occludes.
    expect(computeOcclusionMask([c(0, 0, 5, 5.0), c(0, 0, 50, 4.0)], 0.5)[0]).toBe(true);
  });

  it('uses disc edge exclusively (centre exactly on radius is not covered)', () => {
    // A centre is at distance == radius from B; squared compare is strict (<).
    const mask = computeOcclusionMask([c(10, 0, 1, 20), c(0, 0, 10, 5)]);
    expect(mask[0]).toBe(false);
    // just inside the radius → occluded
    const mask2 = computeOcclusionMask([c(9.9, 0, 1, 20), c(0, 0, 10, 5)]);
    expect(mask2[0]).toBe(true);
  });

  it('handles a chain: nearest occludes both middle and far when stacked', () => {
    const mask = computeOcclusionMask([
      c(50, 50, 5, 30), // far
      c(50, 50, 8, 15), // middle
      c(50, 50, 20, 3), // near — covers both
    ]);
    expect(mask).toEqual([true, true, false]);
  });
});
