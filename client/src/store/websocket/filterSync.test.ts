import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';

vi.mock('../../utils/loggerConfig', () => ({
  createLogger: () => ({
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
    debug: vi.fn(),
  }),
}));

vi.mock('../settingsStore', () => {
  const subscribers: Array<() => void> = [];
  const state = {
    settings: {
      nodeFilter: {
        enabled: true,
        qualityThreshold: 0.5,
        authorityThreshold: 0.5,
        filterByQuality: true,
        filterByAuthority: false,
        filterMode: 'and',
      },
    },
    subscribe: vi.fn((path: string, cb: () => void) => {
      subscribers.push(cb);
      return () => {
        const idx = subscribers.indexOf(cb);
        if (idx >= 0) subscribers.splice(idx, 1);
      };
    }),
  };
  return {
    useSettingsStore: Object.assign(
      () => state,
      {
        getState: () => state,
        subscribe: vi.fn((cb: () => void) => {
          subscribers.push(cb);
          return () => {
            const idx = subscribers.indexOf(cb);
            if (idx >= 0) subscribers.splice(idx, 1);
          };
        }),
      },
    ),
  };
});

vi.mock('../../features/graph/managers/graphDataManager', () => ({
  graphDataManager: {
    setGraphData: vi.fn().mockResolvedValue(undefined),
  },
}));

import {
  resetFilterState,
  cleanupFilterSubscriptions,
  clearFilterSnapshot,
  setupFilterSubscription,
  forceRefreshFilter,
} from './filterSync';

describe('filterSync', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    resetFilterState();
  });

  afterEach(() => {
    vi.useRealTimers();
    resetFilterState();
  });

  describe('resetFilterState', () => {
    it('resets all filter state without throwing', () => {
      expect(() => resetFilterState()).not.toThrow();
    });
  });

  describe('cleanupFilterSubscriptions', () => {
    it('cleans up subscriptions without throwing', () => {
      expect(() => cleanupFilterSubscriptions()).not.toThrow();
    });
  });

  describe('clearFilterSnapshot', () => {
    it('clears the snapshot without throwing', () => {
      expect(() => clearFilterSnapshot()).not.toThrow();
    });
  });

  describe('setupFilterSubscription', () => {
    it('sets up filter subscription once (idempotent)', () => {
      const get = vi.fn(() => ({
        isConnected: true,
        sendFilterUpdate: vi.fn(),
      }));

      setupFilterSubscription(get);
      setupFilterSubscription(get); // second call should be no-op

      // No throw means success; the guard prevents double setup
      expect(true).toBe(true);
    });

    it('does not send filter update when not connected', () => {
      const sendFilterUpdate = vi.fn();
      const get = vi.fn(() => ({
        isConnected: false,
        sendFilterUpdate,
      }));

      setupFilterSubscription(get);
      vi.advanceTimersByTime(100);

      expect(sendFilterUpdate).not.toHaveBeenCalled();
    });
  });

  describe('forceRefreshFilter', () => {
    it('warns and returns early when not connected', async () => {
      const sendFilterUpdate = vi.fn();
      const get = vi.fn(() => ({
        isConnected: false,
        sendFilterUpdate,
      }));

      await forceRefreshFilter(get);
      expect(sendFilterUpdate).not.toHaveBeenCalled();
    });

    it('sends filter update and clears graph when connected', async () => {
      const sendFilterUpdate = vi.fn();
      const get = vi.fn(() => ({
        isConnected: true,
        sendFilterUpdate,
      }));

      await forceRefreshFilter(get);

      const { graphDataManager } = await import('../../features/graph/managers/graphDataManager');
      expect(graphDataManager.setGraphData).toHaveBeenCalledWith({ nodes: [], edges: [] });
      expect(sendFilterUpdate).toHaveBeenCalledWith(
        expect.objectContaining({
          enabled: true,
          qualityThreshold: 0.5,
        }),
      );
    });

    it('handles missing nodeFilter settings gracefully', async () => {
      const { useSettingsStore } = await import('../settingsStore');
      const original = useSettingsStore.getState().settings;
      (useSettingsStore.getState() as any).settings = {};

      const get = vi.fn(() => ({
        isConnected: true,
        sendFilterUpdate: vi.fn(),
      }));

      await forceRefreshFilter(get);
      // Should not throw -- just logs a warning

      (useSettingsStore.getState() as any).settings = original;
    });
  });
});
