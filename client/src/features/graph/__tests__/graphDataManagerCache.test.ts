/**
 * T7 — graphDataManager cache + dedup test (ADR-03 D5).
 *
 * Verifies the single delivery path:
 *   1. `_setData(incoming)` computes a cheap topology hash.
 *   2. Identical hash → return without firing subscribers.
 *   3. Different hash → cache update + queueMicrotask delivery.
 *   4. `subscribe(cb)` uses a Set — duplicate registrations coalesce.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';

// Mock all heavyweight collaborators so the cache logic is testable in
// isolation. We only exercise _setData semantics via setGraphData.
vi.mock('../../../store/settingsStore', () => ({
  useSettingsStore: {
    getState: () => ({ settings: { qualityGates: {} } }),
  },
}));

vi.mock('../../../services/api/UnifiedApiClient', () => ({
  unifiedApiClient: { get: vi.fn() },
}));

vi.mock('../../../store/websocketStore', () => ({
  WebSocketAdapter: class {},
}));

vi.mock('../../../store/workerErrorStore', () => ({
  useWorkerErrorStore: {
    getState: () => ({
      setWorkerError: vi.fn(),
      resetTransientErrors: vi.fn(),
      recordTransientError: vi.fn(),
    }),
  },
}));

vi.mock('../../../services/BinaryWebSocketProtocol', () => ({
  binaryProtocol: { setUserInteracting: vi.fn() },
}));

vi.mock('../../../types/binaryProtocol', () => ({
  parseBinaryNodeData: vi.fn(),
  createBinaryNodeData: vi.fn(),
  BINARY_NODE_SIZE: 28,
  PROTOCOL_V4: 4,
  Vec3: class {},
}));

vi.mock('../../../types/idMapping', () => ({
  stringToU32: (s: string) => s.length,
}));

// Stub the worker proxy: never ready, so setGraphTopology is skipped and
// we exercise the cache path purely.
vi.mock('../managers/graphWorkerProxy', () => ({
  graphWorkerProxy: {
    isReady: () => false,
    setGraphTopology: vi.fn().mockResolvedValue(undefined),
    processBinaryFrame: vi.fn().mockResolvedValue(undefined),
    initialize: vi.fn().mockResolvedValue(undefined),
  },
}));

import { graphDataManager } from '../managers/graphDataManager';

const drainMicrotasks = () =>
  new Promise<void>(resolve => queueMicrotask(() => resolve()));

describe('graphDataManager cache + dedup (ADR-03 D5)', () => {
  beforeEach(async () => {
    // Reset internal cache by calling dispose then forcing a fresh setup.
    graphDataManager.dispose();
  });

  it('fires subscribers exactly once for identical re-deliveries', async () => {
    const cb = vi.fn();
    graphDataManager.onGraphDataChange(cb);

    const data = {
      nodes: [{ id: 'a', label: 'A', position: { x: 0, y: 0, z: 0 } }],
      edges: [],
    };

    await graphDataManager.setGraphData(data);
    await drainMicrotasks();
    await drainMicrotasks();

    expect(cb).toHaveBeenCalledTimes(1);

    // Re-deliver exact same structural topology — should be deduped.
    await graphDataManager.setGraphData({
      nodes: [{ id: 'a', label: 'A', position: { x: 0, y: 0, z: 0 } }],
      edges: [],
    });
    await drainMicrotasks();
    await drainMicrotasks();

    expect(cb).toHaveBeenCalledTimes(1);
  });

  it('fires subscribers when topology changes (different node count)', async () => {
    const cb = vi.fn();
    graphDataManager.onGraphDataChange(cb);

    await graphDataManager.setGraphData({
      nodes: [{ id: 'a', label: 'A', position: { x: 0, y: 0, z: 0 } }],
      edges: [],
    });
    await drainMicrotasks();

    await graphDataManager.setGraphData({
      nodes: [
        { id: 'a', label: 'A', position: { x: 0, y: 0, z: 0 } },
        { id: 'b', label: 'B', position: { x: 1, y: 0, z: 0 } },
      ],
      edges: [],
    });
    await drainMicrotasks();
    await drainMicrotasks();

    expect(cb).toHaveBeenCalledTimes(2);
  });

  it('subscribe dedups duplicate registrations (Set semantics)', async () => {
    const cb = vi.fn();
    const unsub1 = graphDataManager.onGraphDataChange(cb);
    const unsub2 = graphDataManager.onGraphDataChange(cb);

    await graphDataManager.setGraphData({
      nodes: [{ id: 'x', label: 'X', position: { x: 0, y: 0, z: 0 } }],
      edges: [],
    });
    await drainMicrotasks();
    await drainMicrotasks();

    // Single fire despite two `add` calls — Set has only one entry.
    expect(cb).toHaveBeenCalledTimes(1);

    unsub1();
    unsub2();
  });

  it('exposes cached topology via getLastGraphData', async () => {
    const data = {
      nodes: [{ id: 'cached', label: 'C', position: { x: 1, y: 2, z: 3 } }],
      edges: [],
    };
    await graphDataManager.setGraphData(data);
    await drainMicrotasks();

    const cached = graphDataManager.getLastGraphData();
    expect(cached).not.toBeNull();
    expect(cached?.nodes[0].id).toBe('cached');
  });
});
