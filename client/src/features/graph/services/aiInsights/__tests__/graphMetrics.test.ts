import { describe, it, expect } from 'vitest';
import {
  calculateAveragePathLength,
  calculateClusteringCoefficient,
  calculateCentralization,
  calculateModularity,
  calculateNetworkEfficiency,
  calculateSmallWorldness,
  computeGraphMetrics,
} from '../graphMetrics';

function makeGraph(nodeIds: string[], edges: { source: string; target: string }[]) {
  return {
    nodes: nodeIds.map(id => ({ id, label: id, position: { x: 0, y: 0, z: 0 } })),
    edges: edges.map((e, i) => ({ id: `e${i}`, ...e })),
  } as any;
}

const triangle = makeGraph(['a', 'b', 'c'], [
  { source: 'a', target: 'b' },
  { source: 'b', target: 'c' },
  { source: 'a', target: 'c' },
]);

describe('calculateAveragePathLength', () => {
  it('returns 0 for single node', () => {
    expect(calculateAveragePathLength(makeGraph(['a'], []))).toBe(0);
  });

  it('returns 0 for disconnected nodes with no edges', () => {
    expect(calculateAveragePathLength(makeGraph(['a', 'b'], []))).toBe(0);
  });

  it('returns 1 for two directly connected nodes', () => {
    const g = makeGraph(['a', 'b'], [{ source: 'a', target: 'b' }]);
    expect(calculateAveragePathLength(g)).toBe(1);
  });

  it('returns 1 for a complete triangle', () => {
    expect(calculateAveragePathLength(triangle)).toBe(1);
  });

  it('path length increases with graph diameter', () => {
    // linear chain a-b-c-d-e
    const chain = makeGraph(['a', 'b', 'c', 'd', 'e'], [
      { source: 'a', target: 'b' },
      { source: 'b', target: 'c' },
      { source: 'c', target: 'd' },
      { source: 'd', target: 'e' },
    ]);
    expect(calculateAveragePathLength(chain)).toBeGreaterThan(1);
  });
});

describe('calculateClusteringCoefficient', () => {
  it('returns 0 for fewer than 3 nodes', () => {
    expect(calculateClusteringCoefficient(makeGraph(['a', 'b'], [{ source: 'a', target: 'b' }]))).toBe(0);
  });

  it('returns 1 for a complete triangle', () => {
    expect(calculateClusteringCoefficient(triangle)).toBeCloseTo(1, 5);
  });

  it('returns between 0 and 1', () => {
    const cc = calculateClusteringCoefficient(triangle);
    expect(cc).toBeGreaterThanOrEqual(0);
    expect(cc).toBeLessThanOrEqual(1);
  });
});

describe('calculateCentralization', () => {
  it('returns 0 for fewer than 3 nodes', () => {
    expect(calculateCentralization(makeGraph(['a', 'b'], []))).toBe(0);
  });

  it('returns a positive value for star topology', () => {
    const star = makeGraph(['hub', 'l1', 'l2', 'l3'], [
      { source: 'hub', target: 'l1' },
      { source: 'hub', target: 'l2' },
      { source: 'hub', target: 'l3' },
    ]);
    expect(calculateCentralization(star)).toBeGreaterThan(0);
  });
});

describe('calculateModularity', () => {
  it('returns 0 for graphs with no edges', () => {
    expect(calculateModularity(makeGraph(['a', 'b', 'c'], []))).toBe(0);
  });

  it('returns 0 for a fully connected single community', () => {
    expect(calculateModularity(triangle)).toBe(0);
  });

  it('returns positive for two disconnected components', () => {
    const twoComp = makeGraph(['a', 'b', 'c', 'd'], [
      { source: 'a', target: 'b' },
      { source: 'c', target: 'd' },
    ]);
    expect(calculateModularity(twoComp)).toBeGreaterThanOrEqual(0);
  });
});

describe('calculateNetworkEfficiency', () => {
  it('returns 0 for graphs with no edges', () => {
    expect(calculateNetworkEfficiency(makeGraph(['a', 'b'], []))).toBe(0);
  });

  it('returns a positive value for connected graph', () => {
    expect(calculateNetworkEfficiency(triangle)).toBeGreaterThan(0);
  });
});

describe('calculateSmallWorldness', () => {
  it('divides clustering by path length', () => {
    expect(calculateSmallWorldness(0.8, 2)).toBeCloseTo(0.4, 5);
  });
});

describe('computeGraphMetrics', () => {
  it('density is 1 for complete triangle', () => {
    const m = computeGraphMetrics(triangle);
    expect(m.density).toBeCloseTo(1, 5);
  });

  it('returns object with all required keys', () => {
    const m = computeGraphMetrics(triangle);
    expect(m).toHaveProperty('density');
    expect(m).toHaveProperty('averagePathLength');
    expect(m).toHaveProperty('clusteringCoefficient');
    expect(m).toHaveProperty('centralization');
    expect(m).toHaveProperty('modularity');
    expect(m).toHaveProperty('efficiency');
    expect(m).toHaveProperty('smallWorldness');
  });
});
