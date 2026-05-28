import { describe, it, expect } from 'vitest';
import { Vector3 } from 'three';
import { generateCacheKey, hasHierarchicalStructure, doEdgesCross, calculateAverageNodeDistance } from '../utils';

// Minimal GraphData shape matching what the helpers need
type MinGraph = { edges: { source: string; target: string }[]; nodes: any[] };

describe('generateCacheKey', () => {
  it('returns deterministic JSON string', () => {
    expect(generateCacheKey('a', 1, { x: 2 })).toBe(JSON.stringify(['a', 1, { x: 2 }]));
  });

  it('produces different keys for different args', () => {
    expect(generateCacheKey('a')).not.toBe(generateCacheKey('b'));
  });

  it('handles no args', () => {
    expect(generateCacheKey()).toBe('[]');
  });
});

describe('hasHierarchicalStructure', () => {
  it('returns false on empty graph', () => {
    expect(hasHierarchicalStructure({ edges: [], nodes: [] } as any)).toBe(false);
  });

  it('returns true when a hub has >3× the average degree', () => {
    // hub connects to 10 leaves; average degree ≈ 10*2/11 ≈ 1.8; hub degree = 10 > 5.4
    const nodes = Array.from({ length: 11 }, (_, i) => ({ id: `n${i}` }));
    const edges = Array.from({ length: 10 }, (_, i) => ({ source: 'n0', target: `n${i + 1}` }));
    expect(hasHierarchicalStructure({ nodes, edges } as any)).toBe(true);
  });

  it('returns false for a ring graph (uniform degree)', () => {
    const n = 6;
    const nodes = Array.from({ length: n }, (_, i) => ({ id: `n${i}` }));
    const edges = Array.from({ length: n }, (_, i) => ({ source: `n${i}`, target: `n${(i + 1) % n}` }));
    expect(hasHierarchicalStructure({ nodes, edges } as any)).toBe(false);
  });
});

describe('doEdgesCross', () => {
  const e1 = { start: new Vector3(0, 0, 0), end: new Vector3(2, 0, 2) };
  const e2 = { start: new Vector3(0, 0, 2), end: new Vector3(2, 0, 0) };
  const e3 = { start: new Vector3(5, 0, 5), end: new Vector3(6, 0, 6) };

  it('detects crossing X-shaped edges', () => {
    expect(doEdgesCross(e1, e2)).toBe(true);
  });

  it('returns false for clearly separated edges', () => {
    expect(doEdgesCross(e1, e3)).toBe(false);
  });

  it('returns false for parallel horizontal edges', () => {
    const a = { start: new Vector3(0, 0, 0), end: new Vector3(4, 0, 0) };
    const b = { start: new Vector3(0, 0, 2), end: new Vector3(4, 0, 2) };
    expect(doEdgesCross(a, b)).toBe(false);
  });
});

describe('calculateAverageNodeDistance', () => {
  it('returns 0 for empty map', () => {
    expect(calculateAverageNodeDistance(new Map())).toBe(0);
  });

  it('returns 0 for single-node map', () => {
    const m = new Map([['a', new Vector3(1, 2, 3)]]);
    expect(calculateAverageNodeDistance(m)).toBe(0);
  });

  it('returns correct distance for two nodes', () => {
    const m = new Map<string, Vector3>([
      ['a', new Vector3(0, 0, 0)],
      ['b', new Vector3(3, 4, 0)],
    ]);
    expect(calculateAverageNodeDistance(m)).toBeCloseTo(5, 5);
  });

  it('returns average of all pairwise distances', () => {
    const m = new Map<string, Vector3>([
      ['a', new Vector3(0, 0, 0)],
      ['b', new Vector3(1, 0, 0)],
      ['c', new Vector3(2, 0, 0)],
    ]);
    // pairs: a-b=1, a-c=2, b-c=1 → avg = 4/3
    expect(calculateAverageNodeDistance(m)).toBeCloseTo(4 / 3, 5);
  });
});
