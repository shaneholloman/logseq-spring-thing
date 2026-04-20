/**
 * studioInboxStore - Zustand + Immer.
 *
 * Headless automation output review queue. Powers the sidebar badge and the
 * /studio/inbox view. Server state arrives via `/api/ws/studio` `inbox_new`
 * frames (agent C5 owns backend).
 *
 * Spec: docs/design/2026-04-20-contributor-studio/01-contributor-studio-surface.md §8.4, §14.
 */

import { create } from 'zustand';
import { produce } from 'immer';
import type { InboxItem } from '../types';

export interface StudioInboxState {
  items: InboxItem[];
  recentArtifactIds: string[];
  loading: boolean;
  error: string | null;

  fetchInbox: () => Promise<void>;
  markRead: (id: string) => void;
  ack: (id: string) => Promise<void>;
  pushRecentArtifact: (artifactId: string) => void;
}

function unreadCount(items: InboxItem[]): number {
  return items.filter((i) => i.disposition === 'pending').length;
}

export const useStudioInboxStore = create<StudioInboxState>((set) => ({
  items: [],
  recentArtifactIds: [],
  loading: false,
  error: null,

  fetchInbox: async () => {
    set({ loading: true, error: null });
    // Stub: backend endpoint owned by agent C5.
    await new Promise((r) => setTimeout(r, 0));
    set({ loading: false });
  },

  markRead: (id) =>
    set(
      produce((draft: StudioInboxState) => {
        const item = draft.items.find((i) => i.id === id);
        if (item && item.disposition === 'pending') {
          item.disposition = 'accepted';
        }
      }),
    ),

  ack: async (id) => {
    // Stub: MCP `inbox_ack`.
    set(
      produce((draft: StudioInboxState) => {
        draft.items = draft.items.filter((i) => i.id !== id);
      }),
    );
  },

  pushRecentArtifact: (artifactId) =>
    set(
      produce((draft: StudioInboxState) => {
        draft.recentArtifactIds = [
          artifactId,
          ...draft.recentArtifactIds.filter((id) => id !== artifactId),
        ].slice(0, 10);
      }),
    ),
}));

export const useStudioInboxUnreadCount = (): number =>
  useStudioInboxStore((s) => unreadCount(s.items));
