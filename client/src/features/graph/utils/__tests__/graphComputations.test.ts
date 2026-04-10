// @ts-ignore - vitest types may not be available in all environments
import { describe, it, expect, vi } from 'vitest';

// Mock the nodeScaling module before importing tested module
vi.mock('../nodeScaling', () => ({
  computeNodeScale: vi.fn(() => 1.0),
}));

import {
  computeEdgePositions,
  computeNodeColor,
  isNodeVisible,
  getDomainColor,
  type NodeTypeVisibility,
} from '../graphComputations';
import { computeNodeScale } from '../nodeScaling';
import type { Node as GraphNode, Edge } from '../../managers/graphDataManager';
import type { GraphVisualMode } from '../../hooks/useGraphVisualState';

// Helper to create a minimal node
function makeNode(id: string, opts: Partial<GraphNode> = {}): GraphNode {
  return {
    id,
    label: opts.label || `Node ${id}`,
    position: opts.position || { x: 0, y: 0, z: 0 },
    metadata: opts.metadata,
  } as GraphNode;
}

// Helper to create a minimal edge
function makeEdge(source: string, target: string, opts: Partial<Edge> = {}): Edge {
  return {
    id: opts.id || `${source}-${target}`,
    source,
    target,
    ...opts,
  } as Edge;
}

describe('computeEdgePositions', () => {
  const connectionCountMap = new Map<string, number>();
  const graphMode: GraphVisualMode = 'knowledge_graph';
  const nodeSize = 0.5;

  it('should compute edge positions for a simple 2-node graph', () => {
    const nodes = [
      makeNode('0', { position: { x: 0, y: 0, z: 0 } }),
      makeNode('1', { position: { x: 10, y: 0, z: 0 } }),
    ];
    const edges = [makeEdge('0', '1')];
    const positions = new Float32Array([0, 0, 0, 10, 0, 0]);
    const nodeIdToIndexMap = new Map([['0', 0], ['1', 1]]);

    // computeNodeScale mocked to return 1.0
    // sourceRadius = 1.0 * 0.5 = 0.5, targetRadius = 1.0 * 0.5 = 0.5

    const result = computeEdgePositions(
      edges, positions, nodeIdToIndexMap, nodeSize,
      nodes, connectionCountMap, graphMode,
    );

    expect(result).toHaveLength(1);
    // Source offset: x = 0 + 1 * 0.5 = 0.5
    expect(result[0].source.x).toBeCloseTo(0.5);
    expect(result[0].source.y).toBeCloseTo(0);
    expect(result[0].source.z).toBeCloseTo(0);
    // Target offset: x = 10 - 1 * 0.5 = 9.5
    expect(result[0].target.x).toBeCloseTo(9.5);
    expect(result[0].target.y).toBeCloseTo(0);
    expect(result[0].target.z).toBeCloseTo(0);
  });

  it('should skip edges with missing source node', () => {
    const nodes = [makeNode('0')];
    const edges = [makeEdge('0', 'missing')];
    const positions = new Float32Array([0, 0, 0]);
    const nodeIdToIndexMap = new Map([['0', 0]]);

    const result = computeEdgePositions(
      edges, positions, nodeIdToIndexMap, nodeSize,
      nodes, connectionCountMap, graphMode,
    );

    expect(result).toHaveLength(0);
  });

  it('should skip edges with missing target node', () => {
    const nodes = [makeNode('0')];
    const edges = [makeEdge('missing', '0')];
    const positions = new Float32Array([0, 0, 0]);
    const nodeIdToIndexMap = new Map([['0', 0]]);

    const result = computeEdgePositions(
      edges, positions, nodeIdToIndexMap, nodeSize,
      nodes, connectionCountMap, graphMode,
    );

    expect(result).toHaveLength(0);
  });

  it('should skip zero-length edges (nodes at same position)', () => {
    const nodes = [
      makeNode('0', { position: { x: 5, y: 5, z: 5 } }),
      makeNode('1', { position: { x: 5, y: 5, z: 5 } }),
    ];
    const edges = [makeEdge('0', '1')];
    const positions = new Float32Array([5, 5, 5, 5, 5, 5]);
    const nodeIdToIndexMap = new Map([['0', 0], ['1', 1]]);

    const result = computeEdgePositions(
      edges, positions, nodeIdToIndexMap, nodeSize,
      nodes, connectionCountMap, graphMode,
    );

    expect(result).toHaveLength(0);
  });

  it('should handle empty edge list', () => {
    const result = computeEdgePositions(
      [], new Float32Array(0), new Map(), nodeSize,
      [], connectionCountMap, graphMode,
    );

    expect(result).toEqual([]);
  });

  it('should handle empty positions array gracefully', () => {
    const nodes = [makeNode('0'), makeNode('1')];
    const edges = [makeEdge('0', '1')];
    const nodeIdToIndexMap = new Map([['0', 0], ['1', 1]]);

    const result = computeEdgePositions(
      edges, new Float32Array(0), nodeIdToIndexMap, nodeSize,
      nodes, connectionCountMap, graphMode,
    );

    expect(result).toHaveLength(0);
  });

  it('should compute edges for 3D diagonal positions', () => {
    const nodes = [
      makeNode('a', { position: { x: 0, y: 0, z: 0 } }),
      makeNode('b', { position: { x: 10, y: 10, z: 10 } }),
    ];
    const edges = [makeEdge('a', 'b')];
    const positions = new Float32Array([0, 0, 0, 10, 10, 10]);
    const nodeIdToIndexMap = new Map([['a', 0], ['b', 1]]);

    const result = computeEdgePositions(
      edges, positions, nodeIdToIndexMap, nodeSize,
      nodes, connectionCountMap, graphMode,
    );

    expect(result).toHaveLength(1);
    // Edge direction is (10,10,10)/sqrt(300). With radius 0.5:
    const len = Math.sqrt(300);
    const nx = 10 / len;
    expect(result[0].source.x).toBeCloseTo(nx * 0.5, 3);
    expect(result[0].target.x).toBeCloseTo(10 - nx * 0.5, 3);
  });

  it('should use per-node visual mode when provided', () => {
    const nodes = [
      makeNode('0', { position: { x: 0, y: 0, z: 0 } }),
      makeNode('1', { position: { x: 10, y: 0, z: 0 } }),
    ];
    const edges = [makeEdge('0', '1')];
    const positions = new Float32Array([0, 0, 0, 10, 0, 0]);
    const nodeIdToIndexMap = new Map([['0', 0], ['1', 1]]);
    const perNodeMap = new Map<string, GraphVisualMode>([
      ['0', 'ontology'],
      ['1', 'agent'],
    ]);

    const result = computeEdgePositions(
      edges, positions, nodeIdToIndexMap, nodeSize,
      nodes, connectionCountMap, graphMode, undefined, undefined, perNodeMap,
    );

    // computeNodeScale should have been called with the per-node modes
    expect(computeNodeScale).toHaveBeenCalledWith(
      expect.objectContaining({ id: '0' }),
      connectionCountMap,
      'ontology',
      undefined,
      undefined,
    );
    expect(computeNodeScale).toHaveBeenCalledWith(
      expect.objectContaining({ id: '1' }),
      connectionCountMap,
      'agent',
      undefined,
      undefined,
    );
    expect(result).toHaveLength(1);
  });

  it('should skip edges where offset gap is too small', () => {
    // Mock computeNodeScale to return a large radius that causes overlapping surfaces
    // radius = scale * nodeSize = 20 * 0.5 = 10, edge length = 5
    // Source surface at x=10, target surface at 5-10 = -5 => gap = distance(-5, 10) but
    // the code checks gap after offset: srcX = 0+1*10 = 10, tgtX = 5-1*10 = -5
    // gap = sqrt((-5-10)^2) = 15, still passes. Need to use nodeSize=1.0 or bigger scale.
    // With scale=100, radius = 100 * 0.5 = 50. Edge=5. srcX = 0+50 = 50, tgtX = 5-50 = -45
    // gap = |50 - (-45)| = 95 (still large distance). The issue is when edges overlap,
    // the gap can still be large but pointing the wrong direction. Actually the gap
    // check is just the Euclidean distance between the offset points, not direction-aware.
    // To make gap < 0.1, we need the offset points very close together.
    // Nodes at (0,0,0) and (1,0,0), scale=1.0, nodeSize=0.5 => radius=0.5
    // srcX = 0+0.5 = 0.5, tgtX = 1-0.5 = 0.5, gap = 0 <= 0.1 => skip!
    (computeNodeScale as ReturnType<typeof vi.fn>).mockReturnValue(1.0);

    const nodes = [
      makeNode('0', { position: { x: 0, y: 0, z: 0 } }),
      makeNode('1', { position: { x: 1, y: 0, z: 0 } }),
    ];
    const edges = [makeEdge('0', '1')];
    const positions = new Float32Array([0, 0, 0, 1, 0, 0]);
    const nodeIdToIndexMap = new Map([['0', 0], ['1', 1]]);

    const result = computeEdgePositions(
      edges, positions, nodeIdToIndexMap, nodeSize,
      nodes, connectionCountMap, graphMode,
    );

    // sourceRadius = 1.0 * 0.5 = 0.5, targetRadius = 1.0 * 0.5 = 0.5
    // Edge length = 1. srcX = 0 + 0.5 = 0.5, tgtX = 1 - 0.5 = 0.5. Gap = 0 => skip
    expect(result).toHaveLength(0);
  });
});

describe('computeNodeColor', () => {
  it('should return default cyan for unknown knowledge graph type', () => {
    const node = makeNode('1');
    expect(computeNodeColor(node, 'knowledge_graph')).toBe('#00ffff');
  });

  it('should return folder color for folder type', () => {
    const node = makeNode('1', { metadata: { type: 'folder' } });
    expect(computeNodeColor(node, 'knowledge_graph')).toBe('#FFD700');
  });

  it('should return file color for file type', () => {
    const node = makeNode('1', { metadata: { type: 'file' } });
    expect(computeNodeColor(node, 'knowledge_graph')).toBe('#00CED1');
  });

  it('should return function color for function type', () => {
    const node = makeNode('1', { metadata: { type: 'function' } });
    expect(computeNodeColor(node, 'knowledge_graph')).toBe('#FF6B6B');
  });

  it('should return class color for class type', () => {
    const node = makeNode('1', { metadata: { type: 'class' } });
    expect(computeNodeColor(node, 'knowledge_graph')).toBe('#4ECDC4');
  });

  // Ontology mode

  it('should return property color for ontology property', () => {
    const node = makeNode('1', { metadata: { type: 'property' } });
    expect(computeNodeColor(node, 'ontology')).toBe('#F38181');
  });

  it('should return property color for datatype_property', () => {
    const node = makeNode('1', { metadata: { type: 'datatype_property' } });
    expect(computeNodeColor(node, 'ontology')).toBe('#F38181');
  });

  it('should return instance color for instance type', () => {
    const node = makeNode('1', { metadata: { type: 'instance' } });
    expect(computeNodeColor(node, 'ontology')).toBe('#B8D4E3');
  });

  it('should return depth-0 color for ontology class at depth 0', () => {
    const node = makeNode('1', { metadata: { type: 'class', depth: 0 } });
    expect(computeNodeColor(node, 'ontology')).toBe('#FF6B6B');
  });

  it('should return depth-3 color for ontology class at depth 3', () => {
    const node = makeNode('1', { metadata: { type: 'class', depth: 3 } });
    expect(computeNodeColor(node, 'ontology')).toBe('#AA96DA');
  });

  it('should clamp depth to max color index for deep hierarchy', () => {
    const node = makeNode('1', { metadata: { type: 'class', depth: 999 } });
    expect(computeNodeColor(node, 'ontology')).toBe('#95E1D3');
  });

  // Agent mode

  it('should return active color for active agent', () => {
    const node = makeNode('1', { metadata: { status: 'active' } });
    expect(computeNodeColor(node, 'agent')).toBe('#2ECC71');
  });

  it('should return busy color for busy agent', () => {
    const node = makeNode('1', { metadata: { status: 'busy' } });
    expect(computeNodeColor(node, 'agent')).toBe('#F39C12');
  });

  it('should return error color for error agent', () => {
    const node = makeNode('1', { metadata: { status: 'error' } });
    expect(computeNodeColor(node, 'agent')).toBe('#E74C3C');
  });

  it('should return queen color for queen agent type', () => {
    const node = makeNode('1', { metadata: { agentType: 'queen' } });
    expect(computeNodeColor(node, 'agent')).toBe('#FFD700');
  });

  it('should return coordinator color for coordinator agent type', () => {
    const node = makeNode('1', { metadata: { agentType: 'coordinator' } });
    expect(computeNodeColor(node, 'agent')).toBe('#E67E22');
  });

  it('should default to active color when no agent status', () => {
    const node = makeNode('1');
    expect(computeNodeColor(node, 'agent')).toBe('#2ECC71');
  });

  // Default mode

  it('should default to knowledge_graph mode when no mode specified', () => {
    const node = makeNode('1');
    expect(computeNodeColor(node)).toBe('#00ffff');
  });
});

describe('isNodeVisible', () => {
  it('should return true when visibility is null', () => {
    const node = makeNode('1');
    expect(isNodeVisible(node, null)).toBe(true);
  });

  it('should return true when visibility is undefined', () => {
    const node = makeNode('1');
    expect(isNodeVisible(node, undefined)).toBe(true);
  });

  it('should return true when all types visible', () => {
    const node = makeNode('1');
    const vis: NodeTypeVisibility = { knowledge: true, ontology: true, agent: true };
    expect(isNodeVisible(node, vis)).toBe(true);
  });

  it('should hide knowledge_graph node when knowledge is false', () => {
    const node = makeNode('1');
    const vis: NodeTypeVisibility = { knowledge: false, ontology: true, agent: true };
    expect(isNodeVisible(node, vis, 'knowledge_graph')).toBe(false);
  });

  it('should hide ontology node when ontology is false', () => {
    const node = makeNode('1');
    const vis: NodeTypeVisibility = { knowledge: true, ontology: false, agent: true };
    expect(isNodeVisible(node, vis, 'ontology')).toBe(false);
  });

  it('should hide agent node when agent is false', () => {
    const node = makeNode('1');
    const vis: NodeTypeVisibility = { knowledge: true, ontology: true, agent: false };
    expect(isNodeVisible(node, vis, 'agent')).toBe(false);
  });

  it('should use per-node visual mode from map', () => {
    const node = makeNode('1');
    const vis: NodeTypeVisibility = { knowledge: true, ontology: false, agent: true };
    const perNodeMap = new Map<string, GraphVisualMode>([['1', 'ontology']]);

    // Node is classified as ontology via perNodeMap, and ontology is hidden
    expect(isNodeVisible(node, vis, 'knowledge_graph', perNodeMap)).toBe(false);
  });

  it('should fall back to graphMode when node not in perNodeMap', () => {
    const node = makeNode('1');
    const vis: NodeTypeVisibility = { knowledge: false, ontology: true, agent: true };
    const perNodeMap = new Map<string, GraphVisualMode>(); // empty map

    // Falls back to graphMode='knowledge_graph', which is hidden
    expect(isNodeVisible(node, vis, 'knowledge_graph', perNodeMap)).toBe(false);
  });

  it('should return true for unknown mode', () => {
    const node = makeNode('1');
    const vis: NodeTypeVisibility = { knowledge: false, ontology: false, agent: false };
    const perNodeMap = new Map<string, GraphVisualMode>([['1', 'unknown_mode' as GraphVisualMode]]);

    // Unknown mode falls through to the default return true
    expect(isNodeVisible(node, vis, 'knowledge_graph', perNodeMap)).toBe(true);
  });
});

describe('getDomainColor', () => {
  it('should return correct color for AI domain', () => {
    expect(getDomainColor('AI')).toBe('#4FC3F7');
  });

  it('should return correct color for BC domain', () => {
    expect(getDomainColor('BC')).toBe('#81C784');
  });

  it('should return default grey for unknown domain', () => {
    expect(getDomainColor('UNKNOWN')).toBe('#90A4AE');
  });

  it('should return default grey for undefined domain', () => {
    expect(getDomainColor(undefined)).toBe('#90A4AE');
  });

  it('should return default grey for empty string domain', () => {
    expect(getDomainColor('')).toBe('#90A4AE');
  });
});
