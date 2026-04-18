import { create } from 'zustand';
import { persist, createJSONStorage } from 'zustand/middleware';

/**
 * Zustand slice driving the enterprise drawer prototype.
 *
 * `activeSection` is persisted to localStorage so the user lands back on the
 * section they last inspected when they re-open the drawer. `open` itself is
 * intentionally *not* persisted — a drawer that re-opens on every refresh
 * would be hostile.
 */
export interface DrawerState {
  open: boolean;
  activeSection: string | null;
  openDrawer: (section?: string) => void;
  closeDrawer: () => void;
  toggleDrawer: () => void;
  setActiveSection: (section: string | null) => void;
}

type PersistedSlice = Pick<DrawerState, 'activeSection'>;

export const useDrawerStore = create<DrawerState>()(
  persist(
    (set, get) => ({
      open: false,
      activeSection: null,
      openDrawer: (section) =>
        set((prev) => ({
          open: true,
          activeSection: section ?? prev.activeSection,
        })),
      closeDrawer: () => set({ open: false }),
      toggleDrawer: () => set({ open: !get().open }),
      setActiveSection: (section) => set({ activeSection: section }),
    }),
    {
      name: 'visionflow.enterprise.drawer',
      storage: createJSONStorage(() => localStorage),
      partialize: (state): PersistedSlice => ({
        activeSection: state.activeSection,
      }),
    },
  ),
);
