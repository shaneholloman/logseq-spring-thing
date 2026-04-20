/**
 * studioPartnerStore - Zustand + Immer.
 *
 * Chat transcripts keyed by workspace, active partner handle, and streaming
 * state. Wire-up to `/api/ws/studio` and `studio_run_skill` is deferred to
 * agents C1 and X1.
 *
 * Spec: docs/design/2026-04-20-contributor-studio/01-contributor-studio-surface.md §14, §15.
 */

import { create } from 'zustand';
import { produce } from 'immer';
import type { PartnerMessage } from '../types';

export interface StudioPartnerState {
  transcriptsByWorkspaceId: Record<string, PartnerMessage[]>;
  activeSessionId: string | null;
  streaming: boolean;
  error: string | null;

  appendMessage: (workspaceId: string, message: PartnerMessage) => void;
  setActiveSession: (sessionId: string | null) => void;
  sendMessage: (workspaceId: string, content: string) => Promise<void>;
  cancelStream: () => void;
  clearTranscript: (workspaceId: string) => void;
}

export const useStudioPartnerStore = create<StudioPartnerState>((set) => ({
  transcriptsByWorkspaceId: {},
  activeSessionId: null,
  streaming: false,
  error: null,

  appendMessage: (workspaceId, message) =>
    set(
      produce((draft: StudioPartnerState) => {
        const list = draft.transcriptsByWorkspaceId[workspaceId] ?? [];
        list.push(message);
        draft.transcriptsByWorkspaceId[workspaceId] = list;
      }),
    ),

  setActiveSession: (sessionId) => set({ activeSessionId: sessionId }),

  sendMessage: async (_workspaceId, _content) => {
    // Stub: POST via MCP `partner_message` or WebSocket when agent X1 wires.
    set({ streaming: true });
    await new Promise((r) => setTimeout(r, 0));
    set({ streaming: false });
  },

  cancelStream: () => set({ streaming: false }),

  clearTranscript: (workspaceId) =>
    set(
      produce((draft: StudioPartnerState) => {
        delete draft.transcriptsByWorkspaceId[workspaceId];
      }),
    ),
}));
