import { describe, it, expect } from 'vitest';
import { selectOptimalClusteringAlgorithm, applyClustering, calculateClusterQuality, generateClusterRecommendations } from '../clusterDetector';

function makeGraph(nodeIds: string[], edges: { source: string; target: string }[]) {
  return {
    nodes: nodeIds.map(id => ({ id, label: id, position: { x: 0, y: 0, z: 0 }, metadata: { type: 'file' } })),
    edges: edges.map((e, i) => ({ id: `e${i}`, ...e })),
  } as any;
}

describe('selectOptimalClusteringAlgorithm', () => {
  it('selects modularity for small graphs (<50 nodes)', () => {
    const g = makeGraph(['a', 'b'], [{ source: 'a', target: 'b' }]);
    expect(selectOptimalClusteringAlgorithm(g)).toBe('modularity');
  });

  it('selects density for dense graphs (edge/node ratio >3)', () => {
    // 4 nodes, 13 edges → ratio 3.25
    const ids = ['a', 'b', 'c', 'd'];
    const edges: { source: string; target: string }[] = [];
    // fully connected = 6 edges — still only 1.5, need more nodes
    // instead: 60 nodes, 250 edges
    const big = makeGraph(
      Array.from({ length: 60 }, (_, i) => `n${i}`),
      Array.from({ length: 250 }, (_, i) => ({ source: `n${i % 60}`, target: `n${(i + 1) % 60}` }))
    );
    expect(selectOptimalClusteringAlgorithm(big)).toBe('density');
  });

  it('returns spectral or hierarchical for medium graphs', () => {
    const medium = makeGraph(
      Array.from({ length: 100 }, (_, i) => `n${i}`),
      Array.from({ length: 50 }, (_, i) => ({ source: `n${i}`, target: `n${(i + 1) % 100}` }))
    );
    const algo = selectOptimalClusteringAlgorithm(medium);
    expect(['spectral', 'hierarchical']).toContain(algo);
  });
});

describe('applyClustering', () => {
  it('returns empty array for empty graph', async () => {
    const clusters = await applyClustering(makeGraph([], []), 'modularity', {});
    expect(clusters).toHaveLength(0);
  });

  it('groups connected nodes into clusters', async () => {
    const g = makeGraph(['a', 'b', 'c', 'd'], [
      { source: 'a', target: 'b' },
      { source: 'c', target: 'd' },
    ]);
    const clusters = await applyClustering(g, 'modularity', { minClusterSize: 2 });
    expect(clusters.length).toBe(2);
    const allNodes = clusters.flatMap(c => c.nodes);
    expect(allNodes.sort()).toEqual(['a', 'b', 'c', 'd']);
  });

  it('respects minClusterSize', async () => {
    const g = makeGraph(['a', 'b', 'c'], [{ source: 'a', target: 'b' }]);
    // 'c' is isolated (cluster size 1), should be excluded
    const clusters = await applyClustering(g, 'modularity', { minClusterSize: 2 });
    expect(clusters.every(c => c.nodes.length >= 2)).toBe(true);
  });

  it('cluster has positive density for interconnected nodes', async () => {
    const g = makeGraph(['a', 'b', 'c'], [
      { source: 'a', target: 'b' },
      { source: 'b', target: 'c' },
      { source: 'a', target: 'c' },
    ]);
    const clusters = await applyClustering(g, 'modularity', { minClusterSize: 2 });
    if (clusters.length > 0) {
      expect(clusters[0].density).toBeGreaterThan(0);
    }
  });
});

describe('calculateClusterQuality', () => {
  it('returns zeros for empty clusters', () => {
    const q = calculateClusterQuality(makeGraph([], []), []);
    expect(q).toEqual({ modularity: 0, silhouette: 0, cohesion: 0 });
  });
});

describe('generateClusterRecommendations', () => {
  it('returns no-cluster message when empty', () => {
    expect(generateClusterRecommendations([], { modularity: 0, silhouette: 0, cohesion: 0 })).toEqual(['No clusters detected']);
  });

  it('returns one recommendation per cluster', async () => {
    const g = makeGraph(['a', 'b', 'c', 'd'], [
      { source: 'a', target: 'b' },
      { source: 'c', target: 'd' },
    ]);
    const clusters = await applyClustering(g, 'modularity', { minClusterSize: 2 });
    const recs = generateClusterRecommendations(clusters, { modularity: 0.5, silhouette: 0.4, cohesion: 0.45 });
    expect(recs.length).toBe(clusters.length);
    recs.forEach(r => expect(typeof r).toBe('string'));
  });
});
