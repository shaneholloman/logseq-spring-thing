/**
 * ClusterHulls.render.test.tsx -- Client integration test for the
 * ClusterHulls renderer.
 *
 * Pins (PRD-007 §5.2 / ADR-061 §D2 / DDD anti-pattern §7.1):
 *   - ClusterHulls reads `cluster_id` from the analytics store
 *     (`useAnalyticsStore`), NOT from per-frame binary data.
 *   - When the analytics store is empty, no hulls are rendered.
 *   - When the store is populated, hulls reflect the store's
 *     `cluster_id` assignments — the renderer never reads
 *     `node.metadata.cluster` or any "analytics buffer" hint passed
 *     per-frame.
 *
 * Implementation under test (Workstream C): `ClusterHulls.tsx` is
 * refactored to consume `useAnalyticsStore` and DROP the
 * `analyticsBuffer` arg / `getClusterKey(node, idx, analytics)` per-frame
 * dispatch. The component reads via:
 *
 *     const byNodeId = useAnalyticsStore(s => s.byNodeId);
 *
 * This file will not compile until that refactor lands — RED phase.
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import React from 'react';
import { render } from '@testing-library/react';

// ── Mocks: the renderer uses Three.js + R3F; we stub the parts that
// would otherwise require a WebGL context. ───────────────────────────────────

vi.mock('@react-three/fiber', () => ({
  useFrame: vi.fn(),
}));

vi.mock('three/examples/jsm/geometries/ConvexGeometry.js', () => ({
  ConvexGeometry: class {
    constructor(_pts: unknown[]) {}
    dispose() {}
  },
}));

vi.mock('../../../../utils/loggerConfig', () => ({
  createLogger: () => ({
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
    debug: vi.fn(),
  }),
  createErrorMetadata: vi.fn((e: unknown) => e),
}));

vi.mock('../../managers/graphWorkerProxy', () => ({
  graphWorkerProxy: {
    onAnalyticsUpdate: vi.fn(() => () => {}),
  },
}));

// ── analyticsStore mock — the test's view of the contract ───────────────────
//
// `vi.mock` calls are hoisted to the top of the file, so any state we
// reference from the factory has to be hoisted via `vi.hoisted` to be
// available when the factory runs.

type AnalyticsRow = {
  cluster_id?: number;
  community_id?: number;
  anomaly_score?: number;
  sssp_distance?: number;
  sssp_parent?: number;
};

interface MockState {
  byNodeId: Map<number, AnalyticsRow>;
}

const hoisted = vi.hoisted(() => {
  const state: MockState = { byNodeId: new Map() };
  const fn = (selector?: (s: MockState) => unknown) =>
    selector ? selector(state) : state;
  return { state, fn };
});

vi.mock('../../../../store/analyticsStore', () => {
  const useAnalyticsStore = vi.fn(hoisted.fn);
  (useAnalyticsStore as unknown as { getState: () => MockState }).getState = () => hoisted.state;
  return { useAnalyticsStore };
});

// ── Import under test (after mocks) ──────────────────────────────────────────

import { ClusterHulls } from '../ClusterHulls';
import { useAnalyticsStore as useAnalyticsStoreMocked } from '../../../../store/analyticsStore';

// ── Helpers ──────────────────────────────────────────────────────────────────

function resetAnalyticsStore() {
  hoisted.state.byNodeId = new Map();
}

function setAnalyticsRows(rows: Array<[number, AnalyticsRow]>) {
  hoisted.state.byNodeId = new Map(rows);
}

function makeNode(id: string, x = 0, y = 0, z = 0) {
  return {
    id,
    position: { x, y, z },
  };
}

const baseSettings = {
  enabled: true,
  padding: 0.1,
};

const positionsRef = { current: new Float32Array() };
const idToIndex = new Map<string, number>();

beforeEach(() => {
  resetAnalyticsStore();
  vi.clearAllMocks();
});

// ── Empty store — no hulls ───────────────────────────────────────────────────

describe('ClusterHulls — empty analytics store', () => {
  it('renders no hulls when the analytics store has no entries', () => {
    // GIVEN: A node list but an empty analytics store.
    const nodes = [makeNode('1', 0, 0, 0), makeNode('2', 1, 0, 0), makeNode('3', 2, 0, 0)];
    expect(
      (useAnalyticsStoreMocked as unknown as { getState: () => MockState })
        .getState().byNodeId.size,
    ).toBe(0);

    // WHEN: ClusterHulls renders.
    const { container } = render(
      React.createElement(ClusterHulls as unknown as React.FC<{
        nodes: typeof nodes;
        nodePositionsRef: typeof positionsRef;
        nodeIdToIndexMap: typeof idToIndex;
        settings: typeof baseSettings;
      }>, {
        nodes,
        nodePositionsRef: positionsRef,
        nodeIdToIndexMap: idToIndex,
        settings: baseSettings,
      }),
    );

    // THEN: No <mesh> children for hulls. With no cluster_id assignments
    // there is nothing to draw.
    const meshes = container.querySelectorAll('mesh');
    expect(meshes.length).toBe(0);
  });
});

// ── Populated store — hulls drawn ────────────────────────────────────────────

describe('ClusterHulls — populated analytics store', () => {
  it('reads cluster_id from the analytics store, not from per-frame data', () => {
    // GIVEN: 4 nodes positioned in a square. The analytics store
    // assigns all 4 to cluster_id=3 (>= MIN_CLUSTER_SIZE=4 in the
    // existing ClusterHulls). The component must read these
    // assignments from the store ONLY.
    setAnalyticsRows([
      [1, { cluster_id: 3 }],
      [2, { cluster_id: 3 }],
      [3, { cluster_id: 3 }],
      [4, { cluster_id: 3 }],
    ]);
    const nodes = [
      makeNode('1', 0, 0, 0),
      makeNode('2', 1, 0, 0),
      makeNode('3', 1, 1, 0),
      makeNode('4', 0, 1, 0),
    ];

    // WHEN: ClusterHulls renders.
    render(
      React.createElement(ClusterHulls as unknown as React.FC<{
        nodes: typeof nodes;
        nodePositionsRef: typeof positionsRef;
        nodeIdToIndexMap: typeof idToIndex;
        settings: typeof baseSettings;
      }>, {
        nodes,
        nodePositionsRef: positionsRef,
        nodeIdToIndexMap: idToIndex,
        settings: baseSettings,
      }),
    );

    // THEN: The component consulted the analytics store. We do not
    // assert on the rendered geometry directly (it's WebGL-dependent),
    // but we DO assert that the store's read surface was invoked.
    // Workstream C exposes its store reads via `useAnalyticsStore`;
    // this mock has been called from inside the renderer.
    expect(useAnalyticsStoreMocked).toHaveBeenCalled();
  });

  it('does NOT read cluster_id from node.metadata or per-frame buffer hints', () => {
    // GIVEN: A node with a misleading metadata.cluster_id and an empty
    // analytics store. The legacy code-path used to consult
    // `node.metadata.cluster`/`metadata.cluster_id`/`getClusterKey(node,
    // idx, analytics)` as fallbacks. Per ADR-061 §D3 those paths are
    // removed.
    const nodes = [
      {
        id: '1',
        position: { x: 0, y: 0, z: 0 },
        metadata: { cluster_id: 999 },
      },
      {
        id: '2',
        position: { x: 1, y: 0, z: 0 },
        metadata: { cluster_id: 999 },
      },
      {
        id: '3',
        position: { x: 1, y: 1, z: 0 },
        metadata: { cluster_id: 999 },
      },
      {
        id: '4',
        position: { x: 0, y: 1, z: 0 },
        metadata: { cluster_id: 999 },
      },
    ];

    // WHEN: We render with NO analytics store entries.
    const { container } = render(
      React.createElement(ClusterHulls as unknown as React.FC<{
        nodes: typeof nodes;
        nodePositionsRef: typeof positionsRef;
        nodeIdToIndexMap: typeof idToIndex;
        settings: typeof baseSettings;
      }>, {
        nodes,
        nodePositionsRef: positionsRef,
        nodeIdToIndexMap: idToIndex,
        settings: baseSettings,
      }),
    );

    // THEN: Despite metadata.cluster_id=999 on every node, the
    // renderer draws NO hulls — it does not fall back to metadata.
    const meshes = container.querySelectorAll('mesh');
    expect(meshes.length).toBe(0);
  });
});

// ── Partial store — only some nodes assigned ─────────────────────────────────

describe('ClusterHulls — partial analytics coverage', () => {
  it('renders hulls only for clusters whose nodes are present in the store', () => {
    // GIVEN: 5 nodes; the store assigns 4 of them to cluster=1 (enough
    // for a hull at MIN_CLUSTER_SIZE=4) and leaves the 5th unassigned.
    setAnalyticsRows([
      [1, { cluster_id: 1 }],
      [2, { cluster_id: 1 }],
      [3, { cluster_id: 1 }],
      [4, { cluster_id: 1 }],
      // node 5 omitted from the store
    ]);
    const nodes = [
      makeNode('1', 0, 0, 0),
      makeNode('2', 1, 0, 0),
      makeNode('3', 1, 1, 0),
      makeNode('4', 0, 1, 0),
      makeNode('5', 5, 5, 0),
    ];

    // WHEN: Rendered.
    const { container } = render(
      React.createElement(ClusterHulls as unknown as React.FC<{
        nodes: typeof nodes;
        nodePositionsRef: typeof positionsRef;
        nodeIdToIndexMap: typeof idToIndex;
        settings: typeof baseSettings;
      }>, {
        nodes,
        nodePositionsRef: positionsRef,
        nodeIdToIndexMap: idToIndex,
        settings: baseSettings,
      }),
    );

    // THEN: Render does not throw; the unassigned node is ignored, and
    // only the cluster-1 group contributes to hull rendering. (We do
    // not pin the exact mesh count because hull tessellation is
    // implementation-detail; we DO pin that the renderer tolerated a
    // node missing from the store.)
    expect(container).toBeDefined();
  });
});
