/**
 * T7 — GraphManager identity short-circuit logic (ADR-03 D6).
 *
 * We can't reasonably mount GraphManager.tsx in jsdom (Three.js, R3F,
 * Comlink worker, instanced meshes — too many cross-cutting deps). The
 * short-circuit logic is well-isolated and amenable to a unit-level
 * test of the two refs' interaction.
 *
 * This test extracts the decision tree from handleGraphUpdate and asserts:
 *   1. Reference equality (same object) → skip rebuild.
 *   2. Same topology hash, different reference → adopt new ref, skip rebuild.
 *   3. Different topology hash → trigger rebuild (record call).
 */

import { describe, it, expect, vi } from 'vitest';

interface Shape {
  nodeCount: number;
  edgeCount: number;
  hash: string;
}

interface GraphData {
  nodes: Array<{ id: string }>;
  edges: Array<{ id: string; source: string; target: string }>;
}

function topologyHash(data: GraphData): string {
  const nodes = data.nodes;
  const edges = data.edges;
  const firstId = nodes.length > 0 ? String(nodes[0].id) : '';
  const lastId = nodes.length > 0 ? String(nodes[nodes.length - 1].id) : '';
  return `${nodes.length}-${edges.length}-${firstId}-${lastId}`;
}

/**
 * Mirrors the decision tree from GraphManager.handleGraphUpdate. Returns
 * 'skip-ref' / 'skip-shape' / 'rebuild' depending on which path was taken.
 */
function shortCircuitDecision(
  data: GraphData,
  lastRef: GraphData | null,
  lastShape: Shape | null,
  onRebuild: () => void,
): { decision: 'skip-ref' | 'skip-shape' | 'rebuild'; ref: GraphData; shape: Shape | null } {
  if (data === lastRef) {
    return { decision: 'skip-ref', ref: data, shape: lastShape };
  }
  const shape: Shape = {
    nodeCount: data.nodes.length,
    edgeCount: data.edges.length,
    hash: topologyHash(data),
  };
  if (
    lastShape &&
    lastShape.nodeCount === shape.nodeCount &&
    lastShape.edgeCount === shape.edgeCount &&
    lastShape.hash === shape.hash
  ) {
    return { decision: 'skip-shape', ref: data, shape: lastShape };
  }
  onRebuild();
  return { decision: 'rebuild', ref: data, shape };
}

describe('GraphManager identity short-circuit (ADR-03 D6)', () => {
  it('skips rebuild on reference identity (fast path)', () => {
    const data: GraphData = {
      nodes: [{ id: '1' }],
      edges: [],
    };
    const rebuild = vi.fn();

    const r1 = shortCircuitDecision(data, null, null, rebuild);
    expect(r1.decision).toBe('rebuild');
    expect(rebuild).toHaveBeenCalledTimes(1);

    // Same reference re-delivered.
    const r2 = shortCircuitDecision(data, r1.ref, r1.shape, rebuild);
    expect(r2.decision).toBe('skip-ref');
    expect(rebuild).toHaveBeenCalledTimes(1); // not called again
  });

  it('skips rebuild on topology hash match (slow path)', () => {
    const a: GraphData = {
      nodes: [{ id: '1' }, { id: '2' }],
      edges: [{ id: 'e1', source: '1', target: '2' }],
    };
    const b: GraphData = {
      // Same node count, same edge count, same first/last id — but a
      // different reference. Should hit the shape-equality short-circuit.
      nodes: [{ id: '1' }, { id: '2' }],
      edges: [{ id: 'e1', source: '1', target: '2' }],
    };
    const rebuild = vi.fn();

    const r1 = shortCircuitDecision(a, null, null, rebuild);
    expect(r1.decision).toBe('rebuild');

    const r2 = shortCircuitDecision(b, r1.ref, r1.shape, rebuild);
    expect(r2.decision).toBe('skip-shape');
    expect(rebuild).toHaveBeenCalledTimes(1);
    // The new reference was adopted.
    expect(r2.ref).toBe(b);
  });

  it('rebuilds on genuine topology change (node count delta)', () => {
    const a: GraphData = {
      nodes: [{ id: '1' }],
      edges: [],
    };
    const b: GraphData = {
      nodes: [{ id: '1' }, { id: '2' }],
      edges: [],
    };
    const rebuild = vi.fn();

    const r1 = shortCircuitDecision(a, null, null, rebuild);
    const r2 = shortCircuitDecision(b, r1.ref, r1.shape, rebuild);
    expect(r2.decision).toBe('rebuild');
    expect(rebuild).toHaveBeenCalledTimes(2);
  });

  it('rebuilds on topology change with same node count (different ids)', () => {
    const a: GraphData = {
      nodes: [{ id: '1' }, { id: '2' }],
      edges: [],
    };
    const b: GraphData = {
      // Same node count but different last id → hash differs.
      nodes: [{ id: '1' }, { id: '3' }],
      edges: [],
    };
    const rebuild = vi.fn();

    const r1 = shortCircuitDecision(a, null, null, rebuild);
    const r2 = shortCircuitDecision(b, r1.ref, r1.shape, rebuild);
    expect(r2.decision).toBe('rebuild');
    expect(rebuild).toHaveBeenCalledTimes(2);
  });
});
