/**
 * analyticsStore.ts — Side-table for sticky GPU outputs (ADR-061 / PRD-007).
 *
 * Sticky analytics (cluster_id, community_id, anomaly_score, sssp_distance,
 * sssp_parent) used to ride the per-physics-tick wire at 60 Hz, even though
 * their values changed at recompute cadence (seconds–minutes). They now
 * arrive on a separate `analytics_update` text-message channel and merge
 * into this store, keyed by node id.
 *
 * Renderers (ClusterHulls, anomaly highlighting, SSSP gradients) read from
 * this store rather than per-frame parsed binary fields.
 *
 * Merge policy: per-source last-wins by `generation`. Out-of-order updates
 * (incoming generation ≤ stored generation) are dropped.
 */

import { create } from 'zustand';

// ── Wire types ─────────────────────────────────────────────────────────

export type AnalyticsSource = 'clustering' | 'community' | 'anomaly' | 'sssp';

export interface AnalyticsEntry {
  id: number;
  cluster_id?: number;
  community_id?: number;
  anomaly_score?: number;
  sssp_distance?: number;
  sssp_parent?: number;
}

export interface AnalyticsUpdate {
  type: 'analytics_update';
  source: AnalyticsSource;
  generation: number;
  entries: AnalyticsEntry[];
}

// ── Store types ────────────────────────────────────────────────────────

export interface AnalyticsRow {
  cluster_id?: number;
  community_id?: number;
  anomaly_score?: number;
  sssp_distance?: number;
  sssp_parent?: number;
}

interface AnalyticsGenerations {
  clustering: number;
  community: number;
  anomaly: number;
  sssp: number;
}

export interface AnalyticsStore {
  /** Per-node analytics row, keyed by raw u32 node id. */
  byNodeId: Map<number, AnalyticsRow>;
  /** Per-source last-seen generation, used for last-wins merge. */
  generations: AnalyticsGenerations;
  /** Merge an `analytics_update` message into the store. */
  merge: (message: AnalyticsUpdate) => void;
  /** Wipe everything (reconnect / disconnect). */
  reset: () => void;
}

/** Whitelist of sources we accept. A bogus value (typo, malicious input,
 * future-protocol message that hasn't shipped to this client) is silently
 * dropped instead of polluting `generations` with stray keys. */
const KNOWN_SOURCES: ReadonlySet<AnalyticsSource> = new Set<AnalyticsSource>([
  'clustering',
  'community',
  'anomaly',
  'sssp',
]);

function isKnownSource(s: unknown): s is AnalyticsSource {
  return typeof s === 'string' && KNOWN_SOURCES.has(s as AnalyticsSource);
}

// ── Per-source field projection ────────────────────────────────────────

/**
 * Map an entry's optional fields into the row, scoped to the message's
 * source. Each source owns a disjoint subset of the AnalyticsRow columns.
 */
function applySourceFields(row: AnalyticsRow, source: AnalyticsSource, entry: AnalyticsEntry) {
  switch (source) {
    case 'clustering':
      if (entry.cluster_id !== undefined) row.cluster_id = entry.cluster_id;
      break;
    case 'community':
      if (entry.community_id !== undefined) row.community_id = entry.community_id;
      break;
    case 'anomaly':
      if (entry.anomaly_score !== undefined) row.anomaly_score = entry.anomaly_score;
      break;
    case 'sssp':
      if (entry.sssp_distance !== undefined) row.sssp_distance = entry.sssp_distance;
      if (entry.sssp_parent !== undefined) row.sssp_parent = entry.sssp_parent;
      break;
  }
}

const initialGenerations: AnalyticsGenerations = {
  clustering: 0,
  community: 0,
  anomaly: 0,
  sssp: 0,
};

// ── Store ──────────────────────────────────────────────────────────────

export const useAnalyticsStore = create<AnalyticsStore>((set, get) => ({
  byNodeId: new Map<number, AnalyticsRow>(),
  generations: { ...initialGenerations },

  merge: (message: AnalyticsUpdate) => {
    if (!message || message.type !== 'analytics_update') return;
    const { source, generation, entries } = message;
    if (!isKnownSource(source) || !Array.isArray(entries)) return;

    const state = get();
    const lastGen = state.generations[source];
    if (typeof generation !== 'number' || generation <= lastGen) {
      // Out-of-order or stale — drop.
      return;
    }

    // Mutate-then-set to keep allocation cost predictable for large entry lists.
    const byNodeId = new Map(state.byNodeId);
    for (const entry of entries) {
      if (typeof entry.id !== 'number') continue;
      const row = byNodeId.get(entry.id) ?? {};
      applySourceFields(row, source, entry);
      byNodeId.set(entry.id, row);
    }

    set({
      byNodeId,
      generations: { ...state.generations, [source]: generation },
    });
  },

  reset: () => {
    set({
      byNodeId: new Map<number, AnalyticsRow>(),
      generations: { ...initialGenerations },
    });
  },
}));
