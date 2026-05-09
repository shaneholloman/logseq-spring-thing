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

const mockApiGet = vi.fn();
const mockApiPost = vi.fn();
vi.mock('../services/api/UnifiedApiClient', () => ({
  unifiedApiClient: {
    get: (...args: unknown[]) => mockApiGet(...args),
    post: (...args: unknown[]) => mockApiPost(...args),
  },
}));

import { useHybridSystemStatus } from './useHybridSystemStatus';

describe('useHybridSystemStatus', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockApiGet.mockResolvedValue({
      data: {
        dockerHealth: 'healthy',
        mcpHealth: 'connected',
        activeSessions: [],
        systemStatus: 'healthy',
        failoverActive: false,
        performance: {
          totalRequests: 0,
          successfulRequests: 0,
          failedRequests: 0,
          averageResponseTimeMs: 0,
          cacheHitRatio: 0,
          connectionPoolUtilization: 0,
          memoryUsageMb: 0,
          activeOptimizations: [],
        },
      },
    });
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('returns initial status with unknown health states', () => {
    const { result } = renderHook(() =>
      useHybridSystemStatus({ enableWebSocket: false }),
    );

    expect(result.current.status.dockerHealth).toBeDefined();
    expect(result.current.status.mcpHealth).toBeDefined();
    expect(result.current.isLoading).toBeDefined();
  });

  it('provides computed boolean helpers', () => {
    const { result } = renderHook(() =>
      useHybridSystemStatus({ enableWebSocket: false }),
    );

    expect(typeof result.current.isSystemHealthy).toBe('boolean');
    expect(typeof result.current.isSystemDegraded).toBe('boolean');
    expect(typeof result.current.isSystemCritical).toBe('boolean');
    expect(typeof result.current.isDockerAvailable).toBe('boolean');
    expect(typeof result.current.isMcpAvailable).toBe('boolean');
  });

  it('exposes action functions', () => {
    const { result } = renderHook(() =>
      useHybridSystemStatus({ enableWebSocket: false }),
    );

    expect(typeof result.current.refresh).toBe('function');
    expect(typeof result.current.reconnect).toBe('function');
    expect(typeof result.current.spawnSwarm).toBe('function');
    expect(typeof result.current.stopSwarm).toBe('function');
    expect(typeof result.current.getPerformanceReport).toBe('function');
  });

  it('fetches status on mount when polling (no WebSocket)', async () => {
    renderHook(() =>
      useHybridSystemStatus({ enableWebSocket: false }),
    );

    // Polling starts immediately
    await vi.waitFor(() => {
      expect(mockApiGet).toHaveBeenCalledWith('/hybrid/status');
    });
  });

  it('handles fetch errors gracefully', async () => {
    mockApiGet.mockRejectedValueOnce(new Error('server down'));

    const { result } = renderHook(() =>
      useHybridSystemStatus({ enableWebSocket: false }),
    );

    await vi.waitFor(() => {
      expect(result.current.error).toBeTruthy();
    });
  });

  it('spawnSwarm posts to the API', async () => {
    mockApiPost.mockResolvedValueOnce({ data: { sessionId: 's1' } });

    const { result } = renderHook(() =>
      useHybridSystemStatus({ enableWebSocket: false }),
    );

    await act(async () => {
      await result.current.spawnSwarm('test task', { priority: 'high' });
    });

    expect(mockApiPost).toHaveBeenCalledWith(
      '/hybrid/spawn-swarm',
      expect.objectContaining({ task: 'test task', priority: 'high' }),
    );
  });
});
