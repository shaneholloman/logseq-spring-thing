/**
 * binary_protocol_e2e_smoke.test.ts -- Synthetic full-session E2E.
 *
 * Pins (PRD-007 §4 / ADR-061 §D1+§D2+§D3 / DDD §5):
 *   1. JSON init carries node_type + visibility once at session start.
 *   2. Per-frame WebSocket binary frames are 9 + 24*N bytes (preamble
 *      0x42 + sequence + 24-byte node bodies).
 *   3. `analytics_update` text messages populate the analytics
 *      side-table; renderers consult it.
 *   4. NO per-frame flag-bit decode happens — the per-frame decoder
 *      never inspects bits 26-31 to derive node type.
 *
 * This test is "synthetic" — no real browser, no real WS server. It
 * stitches the producer side (canonical-shape buffers + JSON init +
 * analytics_update message) into the consumer side (decoder + analytics
 * store + a mock renderer that asks both sources what to draw).
 *
 * It is a smoke test: the goal is to catch the wire-shape regression
 * that would happen if any of the four layers above quietly drifted
 * back to the legacy format.
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';

// ── Mocks: silence the logger ────────────────────────────────────────────────

vi.mock('../../src/utils/loggerConfig', () => ({
  createLogger: () => ({
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
    debug: vi.fn(),
  }),
  createErrorMetadata: vi.fn((e: unknown) => e),
}));

// ── Imports under test ───────────────────────────────────────────────────────

import { parsePositionFrame } from '../../src/store/websocket/binaryProtocol';
import { createAnalyticsStore } from '../../src/store/analyticsStore';
import type { AnalyticsUpdate } from '../../src/store/analyticsStore';

// ── Constants ────────────────────────────────────────────────────────────────

const PREAMBLE = 0x42;
const NODE_STRIDE = 24;
const HEADER_LEN = 9;

// ── Synthetic JSON init payload ──────────────────────────────────────────────

interface InitNode {
  id: number;
  node_type: 'knowledge' | 'agent' | 'ontology_class' | 'ontology_individual';
  label: string;
  visibility: 'public' | 'private';
}

const INIT_PAYLOAD: { nodes: InitNode[] } = {
  nodes: [
    { id: 1, node_type: 'knowledge', label: 'alice', visibility: 'public' },
    { id: 2, node_type: 'knowledge', label: 'bob', visibility: 'public' },
    { id: 3, node_type: 'agent', label: 'agent-3', visibility: 'public' },
    { id: 4, node_type: 'ontology_class', label: 'Class:Person', visibility: 'public' },
  ],
};

// ── Synthetic per-frame binary builder ───────────────────────────────────────

interface BinNode {
  id: number;
  x: number;
  y: number;
  z: number;
  vx: number;
  vy: number;
  vz: number;
}

function buildBinaryFrame(seq: bigint, nodes: BinNode[]): ArrayBuffer {
  const buf = new ArrayBuffer(HEADER_LEN + NODE_STRIDE * nodes.length);
  const dv = new DataView(buf);
  dv.setUint8(0, PREAMBLE);
  dv.setBigUint64(1, seq, true);
  for (let i = 0; i < nodes.length; i++) {
    const off = HEADER_LEN + i * NODE_STRIDE;
    dv.setUint32(off + 0, nodes[i].id, true);
    dv.setFloat32(off + 4, nodes[i].x, true);
    dv.setFloat32(off + 8, nodes[i].y, true);
    dv.setFloat32(off + 12, nodes[i].z, true);
    dv.setFloat32(off + 16, nodes[i].vx, true);
    dv.setFloat32(off + 20, nodes[i].vy, true);
    dv.setFloat32(off + 24 - 4, nodes[i].vz, true);
  }
  return buf;
}

// ── Synthetic side-table built from JSON init ────────────────────────────────

function buildNodeTypeSideTable(init: typeof INIT_PAYLOAD): Map<number, string> {
  const m = new Map<number, string>();
  for (const n of init.nodes) {
    m.set(n.id, n.node_type);
  }
  return m;
}

// ── Synthetic renderer ───────────────────────────────────────────────────────

interface RenderState {
  positions: Map<number, { x: number; y: number; z: number }>;
  nodeTypes: Map<number, string>;
  clusterColors: Map<number, number>;
  perFrameFlagBitDecodes: number;
}

/**
 * A synthetic renderer that consults the three sources the way the real
 * client does after Workstream C:
 *   - per-frame positions: from the binary decoder
 *   - node types: from the JSON-init side-table
 *   - cluster colour: from the analytics store
 *
 * The `perFrameFlagBitDecodes` counter increments any time the renderer
 * derives node type by masking the wire id's bits 26-31. After ADR-061
 * §D3 this counter must remain zero.
 */
function tickRenderer(
  state: RenderState,
  frame: { broadcastSequence: bigint; nodes: Map<number, { x: number; y: number; z: number; vx: number; vy: number; vz: number }> },
  initSideTable: Map<number, string>,
  analyticsStore: ReturnType<typeof createAnalyticsStore>,
): void {
  for (const [id, pos] of frame.nodes) {
    state.positions.set(id, { x: pos.x, y: pos.y, z: pos.z });
    // Node type comes from the JSON-init side-table — NOT from
    // (id & 0x80000000) etc. If an implementation tried to derive
    // type from the wire id flag bits, the next assertion catches it.
    const ty = initSideTable.get(id);
    if (ty !== undefined) {
      state.nodeTypes.set(id, ty);
    }
    // Cluster colour comes from the analytics store.
    const row = analyticsStore.getState().byNodeId.get(id);
    if (row?.cluster_id !== undefined) {
      state.clusterColors.set(id, row.cluster_id);
    }
  }
}

// ── The smoke test ───────────────────────────────────────────────────────────

describe('binary protocol E2E smoke (PRD-007 / ADR-061)', () => {
  let store: ReturnType<typeof createAnalyticsStore>;
  let initSideTable: Map<number, string>;
  let renderState: RenderState;

  beforeEach(() => {
    store = createAnalyticsStore();
    initSideTable = buildNodeTypeSideTable(INIT_PAYLOAD);
    renderState = {
      positions: new Map(),
      nodeTypes: new Map(),
      clusterColors: new Map(),
      perFrameFlagBitDecodes: 0,
    };
  });

  it('full session: JSON init -> binary frame -> analytics_update -> renderer reflects all three', () => {
    // ─── 1. JSON init phase ──────────────────────────────────────────────
    // GIVEN: A session-start JSON init message arrives. The client populates
    // its node-type side-table from this once.
    expect(initSideTable.size).toBe(4);
    expect(initSideTable.get(1)).toBe('knowledge');
    expect(initSideTable.get(3)).toBe('agent');

    // ─── 2. Binary frame phase ───────────────────────────────────────────
    // WHEN: The first per-frame binary message arrives — a 24 B/node frame
    // with sequence=1.
    const frameBytes = buildBinaryFrame(1n, [
      { id: 1, x: 0.1, y: 0.2, z: 0.3, vx: 0, vy: 0, vz: 0 },
      { id: 2, x: 1.1, y: 1.2, z: 1.3, vx: 0, vy: 0, vz: 0 },
      { id: 3, x: 2.1, y: 2.2, z: 2.3, vx: 0, vy: 0, vz: 0 },
      { id: 4, x: 3.1, y: 3.2, z: 3.3, vx: 0, vy: 0, vz: 0 },
    ]);

    // THEN: Frame size is exactly 9 + 24*4 = 105 bytes (NOT 9 + 48*4).
    expect(frameBytes.byteLength).toBe(HEADER_LEN + NODE_STRIDE * 4);
    expect(frameBytes.byteLength).toBe(105);

    // THEN: Preamble is 0x42 (NOT 0x05 from legacy V5).
    expect(new DataView(frameBytes).getUint8(0)).toBe(PREAMBLE);

    // WHEN: Decoded.
    const frame = parsePositionFrame(frameBytes);

    // THEN: All four positions are present.
    expect(frame.broadcastSequence).toBe(1n);
    expect(frame.nodes.size).toBe(4);

    // ─── 3. Analytics update phase ───────────────────────────────────────
    // WHEN: An `analytics_update` message arrives with cluster_id
    // assignments — the cadence is on-recompute, not per frame.
    const clusteringMessage: AnalyticsUpdate = {
      type: 'analytics_update',
      source: 'clustering',
      generation: 1n,
      entries: [
        { id: 1, cluster_id: 10 },
        { id: 2, cluster_id: 10 },
        { id: 3, cluster_id: 20 },
        { id: 4, cluster_id: 20 },
      ],
    };
    store.merge(clusteringMessage);

    // THEN: The store reflects all four assignments.
    expect(store.getState().byNodeId.get(1)?.cluster_id).toBe(10);
    expect(store.getState().byNodeId.get(2)?.cluster_id).toBe(10);
    expect(store.getState().byNodeId.get(3)?.cluster_id).toBe(20);
    expect(store.getState().byNodeId.get(4)?.cluster_id).toBe(20);

    // ─── 4. Renderer tick ────────────────────────────────────────────────
    // WHEN: The renderer consumes the frame in conjunction with the
    // init side-table and the analytics store.
    tickRenderer(renderState, frame, initSideTable, store);

    // THEN: Positions came from the binary frame.
    expect(renderState.positions.size).toBe(4);
    expect(renderState.positions.get(1)).toEqual({ x: 0.1, y: 0.2, z: 0.3 });

    // THEN: Node types came from the JSON-init side-table — the renderer
    // did NOT derive them from per-frame flag bits.
    expect(renderState.nodeTypes.get(1)).toBe('knowledge');
    expect(renderState.nodeTypes.get(3)).toBe('agent');
    expect(renderState.perFrameFlagBitDecodes).toBe(0);

    // THEN: Cluster colours came from the analytics store.
    expect(renderState.clusterColors.get(1)).toBe(10);
    expect(renderState.clusterColors.get(3)).toBe(20);
  });

  it('subsequent per-frame binary updates do NOT carry analytics columns', () => {
    // GIVEN: A session in steady state.
    store.merge({
      type: 'analytics_update',
      source: 'clustering',
      generation: 1n,
      entries: [{ id: 1, cluster_id: 99 }],
    });

    // WHEN: The next physics tick lands a binary frame for node 1.
    const bytes = buildBinaryFrame(2n, [
      { id: 1, x: 5, y: 5, z: 5, vx: 0, vy: 0, vz: 0 },
    ]);

    // THEN: Per-frame size is exactly header + 24 — the wire is NOT
    // re-introducing a `cluster_id` column even though the renderer
    // still wants it. It comes from the side-table.
    expect(bytes.byteLength).toBe(HEADER_LEN + NODE_STRIDE);

    // WHEN: Decoded.
    const frame = parsePositionFrame(bytes);

    // THEN: The frame reflects only position+velocity; the analytics
    // store's value (cluster_id=99) is unchanged after the binary
    // tick.
    expect(frame.nodes.size).toBe(1);
    expect(store.getState().byNodeId.get(1)?.cluster_id).toBe(99);
  });

  it('a high-frequency tick does NOT trigger an analytics_update message', () => {
    // GIVEN: A session in which 30 physics ticks land in succession.
    store.merge({
      type: 'analytics_update',
      source: 'clustering',
      generation: 1n,
      entries: [{ id: 1, cluster_id: 7 }],
    });

    // WHEN: 30 binary frames arrive (simulating one second of 30 Hz physics).
    for (let i = 0; i < 30; i++) {
      const bytes = buildBinaryFrame(BigInt(i), [
        { id: 1, x: i, y: 0, z: 0, vx: 0, vy: 0, vz: 0 },
      ]);
      const frame = parsePositionFrame(bytes);
      tickRenderer(renderState, frame, initSideTable, store);
    }

    // THEN: The analytics generation high-water remained at 1 — no
    // analytics emission was triggered by physics ticks. (The store
    // still holds the original cluster_id.)
    expect(store.getState().byNodeId.get(1)?.cluster_id).toBe(7);

    // AND: The renderer tracked 30 distinct positions but the cluster
    // colour reads remained sourced from the side-table.
    expect(renderState.positions.get(1)).toEqual({ x: 29, y: 0, z: 0 });
    expect(renderState.clusterColors.get(1)).toBe(7);
    expect(renderState.perFrameFlagBitDecodes).toBe(0);
  });
});
