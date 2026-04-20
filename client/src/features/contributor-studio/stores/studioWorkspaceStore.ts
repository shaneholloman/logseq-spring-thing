/**
 * studioWorkspaceStore - Zustand + Immer.
 *
 * Holds the list of ContributorWorkspaces, the active workspace id, current
 * focus, pane layout flags, installed skills, and the selected AI partner.
 * Server is authoritative; actions stub API calls using placeholder bridges.
 *
 * Spec: docs/design/2026-04-20-contributor-studio/01-contributor-studio-surface.md §14.
 */

import { create } from 'zustand';
import { produce } from 'immer';
import type {
  ContributorWorkspace,
  PartnerSelection,
  ShareState,
  SkillRow,
  WorkspaceFocus,
} from '../types';

export type PaneLayout = {
  leftWidth: number;
  rightWidth: number;
  leftCollapsed: boolean;
  rightCollapsed: boolean;
  memoryBarExpanded: boolean;
};

export const DEFAULT_LAYOUT: PaneLayout = {
  leftWidth: 320,
  rightWidth: 380,
  leftCollapsed: false,
  rightCollapsed: false,
  memoryBarExpanded: true,
};

export interface StudioWorkspaceState {
  workspaces: ContributorWorkspace[];
  activeId: string | null;
  layout: PaneLayout;
  loading: boolean;
  error: string | null;

  fetchWorkspaces: () => Promise<void>;
  setActive: (id: string | null) => void;
  updateFocus: (workspaceId: string, focus: Partial<WorkspaceFocus>) => void;
  setPartner: (workspaceId: string, partner: PartnerSelection | null) => void;
  setLayout: (patch: Partial<PaneLayout>) => void;
  setShareState: (workspaceId: string, state: ShareState) => void;
  setInstalledSkills: (workspaceId: string, skills: SkillRow[]) => void;
}

export const useStudioWorkspaceStore = create<StudioWorkspaceState>((set) => ({
  workspaces: [],
  activeId: null,
  layout: DEFAULT_LAYOUT,
  loading: false,
  error: null,

  fetchWorkspaces: async () => {
    set({ loading: true, error: null });
    // Stub: backend endpoint `GET /api/studio/workspaces` owned by BC18 (agent C1).
    await new Promise((r) => setTimeout(r, 0));
    set({ loading: false });
  },

  setActive: (id) => set({ activeId: id }),

  updateFocus: (workspaceId, focus) =>
    set(
      produce((draft: StudioWorkspaceState) => {
        const ws = draft.workspaces.find((w) => w.id === workspaceId);
        if (!ws) return;
        ws.focus = { ...ws.focus, ...focus };
      }),
    ),

  setPartner: (workspaceId, partner) =>
    set(
      produce((draft: StudioWorkspaceState) => {
        const ws = draft.workspaces.find((w) => w.id === workspaceId);
        if (!ws) return;
        ws.partnerSelection = partner;
      }),
    ),

  setLayout: (patch) =>
    set(
      produce((draft: StudioWorkspaceState) => {
        draft.layout = { ...draft.layout, ...patch };
      }),
    ),

  setShareState: (workspaceId, state) =>
    set(
      produce((draft: StudioWorkspaceState) => {
        const ws = draft.workspaces.find((w) => w.id === workspaceId);
        if (!ws) return;
        ws.shareState = state;
      }),
    ),

  setInstalledSkills: (workspaceId, skills) =>
    set(
      produce((draft: StudioWorkspaceState) => {
        const ws = draft.workspaces.find((w) => w.id === workspaceId);
        if (!ws) return;
        ws.installedSkills = skills;
      }),
    ),
}));
