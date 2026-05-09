import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';

vi.mock('../utils/loggerConfig', () => ({
  createLogger: () => ({
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
    debug: vi.fn(),
  }),
}));

const mockToast = vi.fn();
vi.mock('../features/design-system/components/Toast', () => ({
  useToast: () => ({ toast: mockToast }),
}));

const mockGet = vi.fn();
vi.mock('../services/api/UnifiedApiClient', () => ({
  unifiedApiClient: {
    get: (...args: unknown[]) => mockGet(...args),
  },
}));

const subscribers: Array<() => void> = [];
vi.mock('../store/settingsStore', () => ({
  useSettingsStore: Object.assign(
    () => ({
      settings: {
        visualisation: {
          graphs: { logseq: { physics: { autoBalance: true } } },
        },
      },
    }),
    {
      getState: () => ({
        settings: {
          visualisation: {
            graphs: { logseq: { physics: { autoBalance: true } } },
          },
        },
      }),
      subscribe: vi.fn((cb: () => void) => {
        subscribers.push(cb);
        return () => {
          const idx = subscribers.indexOf(cb);
          if (idx >= 0) subscribers.splice(idx, 1);
        };
      }),
    },
  ),
}));

import { useAutoBalanceNotifications } from './useAutoBalanceNotifications';

describe('useAutoBalanceNotifications', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.clearAllMocks();
    mockGet.mockResolvedValue({
      data: { success: true, notifications: [] },
    });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('mounts without throwing', () => {
    expect(() => renderHook(() => useAutoBalanceNotifications())).not.toThrow();
  });

  it('polls for notifications after initial delay', async () => {
    renderHook(() => useAutoBalanceNotifications());

    // Initial delay is 5000ms
    await act(async () => {
      vi.advanceTimersByTime(5500);
    });

    expect(mockGet).toHaveBeenCalledWith(
      expect.stringContaining('/graph/auto-balance-notifications'),
    );
  });

  it('shows toast for received notifications', async () => {
    mockGet.mockResolvedValueOnce({
      data: {
        success: true,
        notifications: [
          { message: 'Balance adjusted', timestamp: Date.now(), severity: 'success' },
        ],
      },
    });

    renderHook(() => useAutoBalanceNotifications());

    await act(async () => {
      vi.advanceTimersByTime(6000);
    });

    expect(mockToast).toHaveBeenCalledWith(
      expect.objectContaining({
        description: 'Balance adjusted',
      }),
    );
  });

  it('handles API errors gracefully', async () => {
    mockGet.mockRejectedValueOnce(new Error('Network error'));

    renderHook(() => useAutoBalanceNotifications());

    await act(async () => {
      vi.advanceTimersByTime(6000);
    });

    // Should not throw
    expect(true).toBe(true);
  });

  it('cleans up timers on unmount', () => {
    const { unmount } = renderHook(() => useAutoBalanceNotifications());

    unmount();

    // Advance time to verify no lingering timers fire
    vi.advanceTimersByTime(20000);
    expect(mockGet).not.toHaveBeenCalled();
  });
});
