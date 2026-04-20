/**
 * studioContextStore - Zustand + Immer.
 *
 * Per-workspace focus assembly returned by `ContextAssemblyActor`. Populated
 * via `studio_context_assemble` MCP bridge (agent C1 owns backend).
 *
 * Spec: docs/design/2026-04-20-contributor-studio/01-contributor-studio-surface.md §14.
 */

import { create } from 'zustand';
import { produce } from 'immer';
import type { WorkspaceFocus } from '../types';

export interface StudioContextState {
  byWorkspaceId: Record<string, WorkspaceFocus>;
  loading: boolean;
  error: string | null;

  assembleContext: (workspaceId: string) => Promise<void>;
  invalidate: (workspaceId: string) => void;
}

export const useStudioContextStore = create<StudioContextState>((set) => ({
  byWorkspaceId: {},
  loading: false,
  error: null,

  assembleContext: async (workspaceId) => {
    set({ loading: true, error: null });
    // Stub: MCP tool `studio_context_assemble` wired by agent X1.
    await new Promise((r) => setTimeout(r, 0));
    set(
      produce((draft: StudioContextState) => {
        draft.byWorkspaceId[workspaceId] = {
          nodeRef: null,
          label: '',
          lastUpdatedAt: null,
        };
        draft.loading = false;
      }),
    );
  },

  invalidate: (workspaceId) =>
    set(
      produce((draft: StudioContextState) => {
        delete draft.byWorkspaceId[workspaceId];
      }),
    ),
}));
