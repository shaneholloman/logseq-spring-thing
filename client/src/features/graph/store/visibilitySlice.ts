/**
 * Visibility slice (ADR-049 - Sprint 3).
 *
 * Tracks the in-memory visibility overlay for KG nodes (public/private/
 * tombstone) and recent transitions used by `GraphManager` for the opaque
 * render path and the red-X tombstone fade. Drives `VisibilityControl`
 * publish/unpublish actions against the saga endpoints.
 */

import { create } from 'zustand';
import { apiFetch, apiPost, ApiError } from '../../../utils/apiFetch';
import type {
  NodeVisibility,
  VisibilityTransition,
} from '../types/graphTypes';

/** How long a tombstone marker stays visible before being GC'd. */
export const TOMBSTONE_TTL_MS = 5_000;

export interface VisibilityRecord {
  visibility: NodeVisibility;
  owner_pubkey?: string;
  pod_url?: string;
  /** Set only for `tombstone`; wall-clock ms when the marker should fade out. */
  expiresAt?: number;
}

interface PublishResponse {
  node_id: string;
  visibility: NodeVisibility;
  pod_url?: string;
}

export interface VisibilityState {
  /** Per-node visibility overlay keyed by node id. */
  overlay: Record<string, VisibilityRecord>;
  /** Recent transitions (bounded to 100). */
  transitions: VisibilityTransition[];
  /** Node id whose publish/unpublish POST is in flight. */
  busyId: string | null;
  /** Last error from publish/unpublish, if any. */
  error: string | null;

  /** Apply a transition event from the saga stream. */
  applyTransition: (t: VisibilityTransition) => void;
  /** Publish a node (private -> public). */
  publish: (nodeId: string) => Promise<void>;
  /** Unpublish a node (public -> private), emits a tombstone. */
  unpublish: (nodeId: string) => Promise<void>;
  /** Manually clear any stale tombstone records. */
  gcTombstones: () => void;
  clearError: () => void;
}

const MAX_TRANSITIONS = 100;

function recordFromTransition(t: VisibilityTransition): VisibilityRecord {
  const base: VisibilityRecord = {
    visibility: t.to,
    owner_pubkey: t.owner_pubkey,
    pod_url: t.pod_url,
  };
  if (t.to === 'tombstone') {
    base.expiresAt = Date.now() + TOMBSTONE_TTL_MS;
  }
  return base;
}

export const useVisibilityStore = create<VisibilityState>((set, get) => ({
  overlay: {},
  transitions: [],
  busyId: null,
  error: null,

  applyTransition: (t) => {
    if (!t || !t.node_id) return;
    const state = get();
    const nextOverlay = { ...state.overlay, [t.node_id]: recordFromTransition(t) };
    const nextTransitions = [t, ...state.transitions].slice(0, MAX_TRANSITIONS);
    set({ overlay: nextOverlay, transitions: nextTransitions });
  },

  publish: async (nodeId) => {
    if (!nodeId) return;
    set({ busyId: nodeId, error: null });
    try {
      const res = await apiPost<PublishResponse>(
        `/api/nodes/${encodeURIComponent(nodeId)}/publish`,
        {},
      );
      get().applyTransition({
        id: `local-publish-${nodeId}-${Date.now()}`,
        node_id: nodeId,
        from: get().overlay[nodeId]?.visibility ?? null,
        to: res.visibility,
        pod_url: res.pod_url,
        at: new Date().toISOString(),
      });
      set({ busyId: null });
    } catch (err: unknown) {
      const message = err instanceof ApiError ? err.message : 'Publish failed';
      set({ busyId: null, error: message });
    }
  },

  unpublish: async (nodeId) => {
    if (!nodeId) return;
    set({ busyId: nodeId, error: null });
    try {
      await apiFetch<PublishResponse>(
        `/api/nodes/${encodeURIComponent(nodeId)}/unpublish`,
        { method: 'POST' },
      );
      // Tombstone first, then settle to private. TTL'd record handles the fade.
      const now = new Date().toISOString();
      const from = get().overlay[nodeId]?.visibility ?? 'public';
      get().applyTransition({
        id: `local-tombstone-${nodeId}-${Date.now()}`,
        node_id: nodeId,
        from,
        to: 'tombstone',
        at: now,
      });
      // Schedule settle.
      window.setTimeout(() => {
        get().applyTransition({
          id: `local-private-${nodeId}-${Date.now()}`,
          node_id: nodeId,
          from: 'tombstone',
          to: 'private',
          at: new Date().toISOString(),
        });
      }, TOMBSTONE_TTL_MS);
      set({ busyId: null });
    } catch (err: unknown) {
      const message = err instanceof ApiError ? err.message : 'Unpublish failed';
      set({ busyId: null, error: message });
    }
  },

  gcTombstones: () => {
    const now = Date.now();
    const { overlay } = get();
    let changed = false;
    const next: Record<string, VisibilityRecord> = {};
    for (const [id, rec] of Object.entries(overlay)) {
      if (rec.visibility === 'tombstone' && rec.expiresAt && rec.expiresAt <= now) {
        changed = true;
        // Drop it - GraphManager falls back to the underlying node state.
        continue;
      }
      next[id] = rec;
    }
    if (changed) set({ overlay: next });
  },

  clearError: () => set({ error: null }),
}));
