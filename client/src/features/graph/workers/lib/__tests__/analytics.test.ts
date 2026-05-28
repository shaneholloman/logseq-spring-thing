import { describe, it, expect } from 'vitest';
import { computeAnomalyScores, computeCommunities, recomputeAnalytics } from '../analytics';
import type { GraphData } from '../types';

function makeGraph(nodeIds: string[], edges: { source: string; target: string }[]): GraphData {
  return {
    nodes: nodeIds.map(id => ({ id, label: id, position: { x: 0, y: 0, z: 0 } })),
    edges: edges.map((e, i) => ({ id: `e${i}`, ...e })),
  };
}

describe('computeAnomalyScores', () => {
  it('does nothing on empty graph', () => {
    const buf = new Float32Array(3);
    computeAnomalyScores(buf, makeGraph([], []));
    expect(buf[1]).toBe(0);
  });

  it('single isolated node gets anomaly score 0', () => {
    const buf = new Float32Array(3);
    computeAnomalyScores(buf, makeGraph(['a'], []));
    expect(buf[1]).toBe(0);
  });

  it('hub node (many connections) gets higher anomaly score than leaf', () => {
    // hub connects to 5 leaves; leaves connect only to hub
    const graph = makeGraph(['hub', 'l1', 'l2', 'l3', 'l4', 'l5'], [
      { source: 'hub', target: 'l1' },
      { source: 'hub', target: 'l2' },
      { source: 'hub', target: 'l3' },
      { source: 'hub', target: 'l4' },
      { source: 'hub', target: 'l5' },
    ]);
    const buf = new Float32Array(6 * 3);
    computeAnomalyScores(buf, graph);
    const hubScore = buf[0 * 3 + 1];
    const leafScore = buf[1 * 3 + 1];
    expect(hubScore).toBeGreaterThan(leafScore);
  });

  it('all scores are in range [0, 1]', () => {
    const graph = makeGraph(['a', 'b', 'c', 'd'], [
      { source: 'a', target: 'b' }, { source: 'a', target: 'c' }, { source: 'a', target: 'd' }
    ]);
    const buf = new Float32Array(4 * 3);
    computeAnomalyScores(buf, graph);
    for (let i = 0; i < 4; i++) {
      const score = buf[i * 3 + 1];
      expect(score).toBeGreaterThanOrEqual(0);
      expect(score).toBeLessThanOrEqual(1);
    }
  });
});

describe('computeCommunities', () => {
  it('does nothing on empty graph', () => {
    const buf = new Float32Array(3);
    computeCommunities(buf, makeGraph([], []));
    expect(buf[2]).toBe(0);
  });

  it('isolated nodes each get their own community', () => {
    const graph = makeGraph(['a', 'b', 'c'], []);
    const buf = new Float32Array(3 * 3);
    computeCommunities(buf, graph);
    const ids = [buf[0 * 3 + 2], buf[1 * 3 + 2], buf[2 * 3 + 2]];
    // All different, all >= 1
    expect(new Set(ids).size).toBe(3);
    ids.forEach(id => expect(id).toBeGreaterThanOrEqual(1));
  });

  it('two connected components produce different community IDs', () => {
    const graph = makeGraph(['a', 'b', 'c', 'd'], [
      { source: 'a', target: 'b' },
      { source: 'c', target: 'd' },
    ]);
    const buf = new Float32Array(4 * 3);
    computeCommunities(buf, graph);
    const ca = buf[0 * 3 + 2];
    const cb = buf[1 * 3 + 2];
    const cc = buf[2 * 3 + 2];
    const cd = buf[3 * 3 + 2];
    expect(ca).toBe(cb);
    expect(cc).toBe(cd);
    expect(ca).not.toBe(cc);
  });

  it('community IDs are 1-based positive integers', () => {
    const graph = makeGraph(['a', 'b'], [{ source: 'a', target: 'b' }]);
    const buf = new Float32Array(2 * 3);
    computeCommunities(buf, graph);
    expect(buf[0 * 3 + 2]).toBeGreaterThanOrEqual(1);
    expect(buf[1 * 3 + 2]).toBeGreaterThanOrEqual(1);
  });
});

describe('recomputeAnalytics', () => {
  it('populates buffer when all zeros (no server data)', () => {
    const graph = makeGraph(['a', 'b'], [{ source: 'a', target: 'b' }]);
    const buf = new Float32Array(2 * 3); // all zeros
    recomputeAnalytics(buf, graph);
    // At least community IDs should now be non-zero
    expect(buf[0 * 3 + 2]).toBeGreaterThan(0);
  });

  it('skips recompute when server data already present', () => {
    const graph = makeGraph(['a', 'b'], []);
    const buf = new Float32Array(2 * 3);
    buf[1] = 0.5; // server anomaly score present
    const before = buf.slice();
    recomputeAnalytics(buf, graph);
    // Buffer must be unchanged because hasServerData was true
    expect(buf).toEqual(before);
  });
});
