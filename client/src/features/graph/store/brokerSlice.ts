/**
 * Broker slice (ADR-051 - Sprint 3).
 *
 * Holds migration candidates surfaced by the Judgment Broker plus the UI state
 * (selected candidate, in-flight action) needed to drive the DecisionCanvas in
 * `BrokerInbox`. Polls `GET /api/bridge/candidates?status=surfaced` on a 30s
 * cadence; individual decisions (promote/reject/defer) optimistically remove
 * the candidate locally and roll back on failure.
 */

import { create } from 'zustand';
import { apiFetch, apiPost, ApiError } from '../../../utils/apiFetch';
import type { MigrationCandidate } from '../types/graphTypes';

export type BrokerDecision = 'promote' | 'reject' | 'defer';

interface CandidatesResponse {
  candidates: MigrationCandidate[];
  total?: number;
}

export interface BrokerState {
  /** Surfaced candidates, freshest first. */
  candidates: MigrationCandidate[];
  /** Candidate currently selected in the DecisionCanvas (by id). */
  selectedId: string | null;
  /** Candidate id whose decision POST is in flight (if any). */
  decidingId: string | null;
  /** Last fetch timestamp (ms). */
  lastFetchedAt: number | null;
  /** Last error from fetch or decision, if any. */
  error: string | null;
  /** Fetch in progress flag. */
  loading: boolean;

  fetchCandidates: () => Promise<void>;
  select: (id: string | null) => void;
  decide: (id: string, decision: BrokerDecision, reason?: string) => Promise<void>;
  clearError: () => void;
}

function decisionPath(id: string, decision: BrokerDecision): string {
  switch (decision) {
    case 'promote':
      return `/api/bridge/${encodeURIComponent(id)}/promote`;
    case 'reject':
      return `/api/bridge/${encodeURIComponent(id)}/reject`;
    case 'defer':
      return `/api/bridge/${encodeURIComponent(id)}/defer`;
  }
}

export const useBrokerStore = create<BrokerState>((set, get) => ({
  candidates: [],
  selectedId: null,
  decidingId: null,
  lastFetchedAt: null,
  error: null,
  loading: false,

  fetchCandidates: async () => {
    if (get().loading) return;
    set({ loading: true, error: null });
    try {
      const data = await apiFetch<CandidatesResponse>(
        '/api/bridge/candidates?status=surfaced',
      );
      set({
        candidates: Array.isArray(data.candidates) ? data.candidates : [],
        lastFetchedAt: Date.now(),
        loading: false,
      });
    } catch (err: unknown) {
      const message =
        err instanceof ApiError ? err.message : 'Failed to load bridge candidates';
      set({ loading: false, error: message });
    }
  },

  select: (id) => set({ selectedId: id }),

  decide: async (id, decision, reason) => {
    const prev = get().candidates;
    // Optimistic remove.
    set({
      decidingId: id,
      error: null,
      candidates: prev.filter((c) => c.id !== id),
      selectedId: get().selectedId === id ? null : get().selectedId,
    });
    try {
      await apiPost(decisionPath(id, decision), {
        reason: reason?.trim() || undefined,
      });
      set({ decidingId: null });
    } catch (err: unknown) {
      const message =
        err instanceof ApiError ? err.message : `Decision (${decision}) failed`;
      // Rollback.
      set({ decidingId: null, error: message, candidates: prev });
    }
  },

  clearError: () => set({ error: null }),
}));
