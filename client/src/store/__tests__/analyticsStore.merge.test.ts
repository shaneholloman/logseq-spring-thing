/**
 * analyticsStore.merge.test.ts -- Client unit tests for the analytics
 * side-table merge contract.
 *
 * Pins (PRD-007 §4.2 / ADR-061 §D2 / DDD aggregate `AnalyticsUpdate`):
 *   - Last-wins by `generation` per source.
 *   - Out-of-order generations are dropped (older `generation` after a
 *     higher one MUST NOT clobber the newer value).
 *   - Partial entries from one source do NOT clobber unrelated fields
 *     written by another source on the same node id.
 *   - `byNodeId` is the public read surface keyed by `id` (number).
 */

import { describe, it, expect, beforeEach } from 'vitest';

import { useAnalyticsStore } from '../analyticsStore';
import type { AnalyticsUpdate } from '../analyticsStore';

// ── Helpers ──────────────────────────────────────────────────────────────────

function clusteringUpdate(
  generation: number,
  entries: Array<{ id: number; cluster_id?: number }>,
): AnalyticsUpdate {
  return { type: 'analytics_update', source: 'clustering', generation, entries };
}

function communityUpdate(
  generation: number,
  entries: Array<{ id: number; community_id?: number }>,
): AnalyticsUpdate {
  return { type: 'analytics_update', source: 'community', generation, entries };
}

function anomalyUpdate(
  generation: number,
  entries: Array<{ id: number; anomaly_score?: number }>,
): AnalyticsUpdate {
  return { type: 'analytics_update', source: 'anomaly', generation, entries };
}

function ssspUpdate(
  generation: number,
  entries: Array<{ id: number; sssp_distance?: number; sssp_parent?: number }>,
): AnalyticsUpdate {
  return { type: 'analytics_update', source: 'sssp', generation, entries };
}

// Each test starts from a clean store. The store is a module-level Zustand
// singleton, so `reset()` is the supported isolation hook.
beforeEach(() => {
  useAnalyticsStore.getState().reset();
});

// ── Tests: last-wins by generation ───────────────────────────────────────────

describe('analyticsStore — last-wins by generation per source', () => {
  it('newer generation overwrites older value', () => {
    const store = useAnalyticsStore.getState();
    store.merge(clusteringUpdate(1, [{ id: 1, cluster_id: 10 }]));
    expect(useAnalyticsStore.getState().byNodeId.get(1)?.cluster_id).toBe(10);

    useAnalyticsStore.getState().merge(clusteringUpdate(3, [{ id: 1, cluster_id: 30 }]));
    expect(useAnalyticsStore.getState().byNodeId.get(1)?.cluster_id).toBe(30);
  });

  it('out-of-order older generation is dropped, store unchanged', () => {
    const s = useAnalyticsStore.getState();
    s.merge(clusteringUpdate(1, [{ id: 1, cluster_id: 10 }]));
    s.merge(clusteringUpdate(3, [{ id: 1, cluster_id: 30 }]));

    s.merge(clusteringUpdate(2, [{ id: 1, cluster_id: 20 }]));

    expect(useAnalyticsStore.getState().byNodeId.get(1)?.cluster_id).toBe(30);
  });

  it('repeated same-generation update is idempotent', () => {
    // The merge guard is `generation <= lastGen` so a re-emitted same
    // generation is dropped — but the FIRST emit at gen=5 lands.
    const s = useAnalyticsStore.getState();
    s.merge(clusteringUpdate(5, [{ id: 1, cluster_id: 99 }]));
    s.merge(clusteringUpdate(5, [{ id: 1, cluster_id: 99 }]));

    expect(useAnalyticsStore.getState().byNodeId.get(1)?.cluster_id).toBe(99);
  });
});

// ── Per-source isolation (partial fields don't clobber) ──────────────────────

describe('analyticsStore — partial entries do not clobber unrelated fields', () => {
  it('clustering write + anomaly write on the same node leaves both fields populated', () => {
    const s = useAnalyticsStore.getState();
    s.merge(clusteringUpdate(1, [{ id: 1, cluster_id: 10 }]));
    s.merge(anomalyUpdate(1, [{ id: 1, anomaly_score: 0.7 }]));

    const row = useAnalyticsStore.getState().byNodeId.get(1)!;
    expect(row.cluster_id).toBe(10);
    expect(row.anomaly_score).toBeCloseTo(0.7, 5);
  });

  it('community update does not clobber cluster_id from a prior clustering update', () => {
    const s = useAnalyticsStore.getState();
    s.merge(clusteringUpdate(1, [{ id: 5, cluster_id: 100 }]));
    s.merge(communityUpdate(1, [{ id: 5, community_id: 200 }]));

    const row = useAnalyticsStore.getState().byNodeId.get(5)!;
    expect(row.cluster_id).toBe(100);
    expect(row.community_id).toBe(200);
  });

  it('SSSP update populates distance + parent without affecting clustering', () => {
    const s = useAnalyticsStore.getState();
    s.merge(clusteringUpdate(1, [{ id: 7, cluster_id: 3 }]));
    s.merge(ssspUpdate(1, [{ id: 7, sssp_distance: 2.5, sssp_parent: 4 }]));

    const row = useAnalyticsStore.getState().byNodeId.get(7)!;
    expect(row.cluster_id).toBe(3);
    expect(row.sssp_distance).toBeCloseTo(2.5, 5);
    expect(row.sssp_parent).toBe(4);
  });

  it('a partial entry that omits a previously-known field does NOT delete it', () => {
    const s = useAnalyticsStore.getState();
    s.merge(clusteringUpdate(1, [{ id: 1, cluster_id: 10 }]));
    s.merge(clusteringUpdate(2, [{ id: 1 }]));

    expect(useAnalyticsStore.getState().byNodeId.get(1)?.cluster_id).toBe(10);
  });
});

// ── Cross-source generation independence ─────────────────────────────────────

describe('analyticsStore — generations are tracked per source', () => {
  it('a high clustering generation does not block a low anomaly generation', () => {
    const s = useAnalyticsStore.getState();
    s.merge(clusteringUpdate(10, [{ id: 1, cluster_id: 5 }]));
    s.merge(anomalyUpdate(1, [{ id: 1, anomaly_score: 0.42 }]));

    expect(useAnalyticsStore.getState().byNodeId.get(1)?.anomaly_score).toBeCloseTo(0.42, 5);
  });
});

// ── Multi-node merges in a single update ─────────────────────────────────────

describe('analyticsStore — multi-entry updates', () => {
  it('a single update writes all of its entries into the side-table', () => {
    useAnalyticsStore.getState().merge(
      clusteringUpdate(1, [
        { id: 1, cluster_id: 10 },
        { id: 2, cluster_id: 20 },
        { id: 3, cluster_id: 30 },
      ]),
    );

    const map = useAnalyticsStore.getState().byNodeId;
    expect(map.get(1)?.cluster_id).toBe(10);
    expect(map.get(2)?.cluster_id).toBe(20);
    expect(map.get(3)?.cluster_id).toBe(30);
  });

  it('an empty entries array advances the generation cursor without writing nodes', () => {
    const s = useAnalyticsStore.getState();
    s.merge(clusteringUpdate(5, []));
    expect(useAnalyticsStore.getState().byNodeId.has(1)).toBe(false);

    s.merge(clusteringUpdate(4, [{ id: 1, cluster_id: 100 }]));
    expect(useAnalyticsStore.getState().byNodeId.has(1)).toBe(false);

    s.merge(clusteringUpdate(6, [{ id: 1, cluster_id: 100 }]));
    expect(useAnalyticsStore.getState().byNodeId.get(1)?.cluster_id).toBe(100);
  });
});

// ── F7 fix: source whitelist (post-consultancy hardening) ────────────────────

describe('analyticsStore — unknown source values are dropped (F7)', () => {
  it('does not pollute generations with bogus source keys', () => {
    const s = useAnalyticsStore.getState();
    // Cast through unknown to bypass TypeScript's union-type fiction —
    // the JSON wire can deliver anything, and the runtime guard must
    // reject it.
    const bogus = {
      type: 'analytics_update',
      source: 'foo',
      generation: 1,
      entries: [{ id: 1, cluster_id: 99 }],
    } as unknown as AnalyticsUpdate;

    s.merge(bogus);
    const state = useAnalyticsStore.getState();
    // No node added.
    expect(state.byNodeId.has(1)).toBe(false);
    // Generations only have the four known keys, no `foo`.
    expect((state.generations as unknown as Record<string, number>).foo).toBeUndefined();
  });
});
