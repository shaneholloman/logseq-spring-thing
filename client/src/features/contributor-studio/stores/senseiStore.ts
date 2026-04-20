/**
 * senseiStore - Zustand + Immer.
 *
 * Ontology-guide suggestions (terms / concepts / policies) per workspace plus
 * the focus trace for /studio/:id/sensei. Backed by `sensei_nudge` MCP tool
 * (agent X1).
 *
 * Spec: docs/design/2026-04-20-contributor-studio/01-contributor-studio-surface.md §6, §14.
 */

import { create } from 'zustand';
import { produce } from 'immer';
import type { SenseiNudges, SenseiSuggestion } from '../types';

const emptyNudges = (): SenseiNudges => ({
  terms: [],
  concepts: [],
  policies: [],
});

export type TraceEvent = {
  id: string;
  at: string;
  label: string;
  detail: string;
};

export interface SenseiState {
  nudgesByWorkspaceId: Record<string, SenseiNudges>;
  trace: TraceEvent[];
  loading: boolean;
  error: string | null;

  loadNudges: (workspaceId: string) => Promise<void>;
  accept: (workspaceId: string, suggestion: SenseiSuggestion) => void;
  dismiss: (workspaceId: string, suggestionId: string, reason: string) => void;
  appendTrace: (event: TraceEvent) => void;
}

export const useSenseiStore = create<SenseiState>((set) => ({
  nudgesByWorkspaceId: {},
  trace: [],
  loading: false,
  error: null,

  loadNudges: async (workspaceId) => {
    set({ loading: true, error: null });
    // Stub: MCP tool `sensei_nudge` wired by agent X1.
    await new Promise((r) => setTimeout(r, 0));
    set(
      produce((draft: SenseiState) => {
        if (!draft.nudgesByWorkspaceId[workspaceId]) {
          draft.nudgesByWorkspaceId[workspaceId] = emptyNudges();
        }
        draft.loading = false;
      }),
    );
  },

  accept: (workspaceId, suggestion) =>
    set(
      produce((draft: SenseiState) => {
        const bucket = draft.nudgesByWorkspaceId[workspaceId];
        if (!bucket) return;
        const remove = (arr: SenseiSuggestion[]) =>
          arr.filter((s) => s.id !== suggestion.id);
        bucket.terms = remove(bucket.terms);
        bucket.concepts = remove(bucket.concepts);
        bucket.policies = remove(bucket.policies);
        draft.trace.unshift({
          id: `accept-${suggestion.id}-${Date.now()}`,
          at: new Date().toISOString(),
          label: `Accepted ${suggestion.label}`,
          detail: suggestion.rationale,
        });
      }),
    ),

  dismiss: (workspaceId, suggestionId, reason) =>
    set(
      produce((draft: SenseiState) => {
        const bucket = draft.nudgesByWorkspaceId[workspaceId];
        if (!bucket) return;
        const remove = (arr: SenseiSuggestion[]) =>
          arr.filter((s) => s.id !== suggestionId);
        bucket.terms = remove(bucket.terms);
        bucket.concepts = remove(bucket.concepts);
        bucket.policies = remove(bucket.policies);
        draft.trace.unshift({
          id: `dismiss-${suggestionId}-${Date.now()}`,
          at: new Date().toISOString(),
          label: `Dismissed suggestion`,
          detail: reason,
        });
      }),
    ),

  appendTrace: (event) =>
    set(
      produce((draft: SenseiState) => {
        draft.trace.unshift(event);
        if (draft.trace.length > 200) draft.trace.pop();
      }),
    ),
}));
