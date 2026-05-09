import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import React from 'react';
import { renderHook, act } from '@testing-library/react';

vi.mock('../../../utils/loggerConfig', () => ({
  createLogger: () => ({
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
    debug: vi.fn(),
  }),
}));

vi.mock('../services/BotsWebSocketIntegration', () => ({
  botsWebSocketIntegration: {
    on: vi.fn(() => vi.fn()), // returns unsubscribe
  },
}));

vi.mock('../../../types/binaryProtocol', () => ({
  decodePositionFrame: vi.fn(() => ({ nodes: new Map() })),
}));

const mockPollingData = {
  agents: [],
  edges: [],
  metadata: null,
  isPolling: false,
  activityLevel: 'idle' as const,
  lastUpdate: 0,
  error: null,
  pollNow: vi.fn(),
  configure: vi.fn(),
};

vi.mock('../hooks/useAgentPolling', () => ({
  useAgentPolling: () => mockPollingData,
}));

vi.mock('../services/AgentPollingService', () => ({
  agentPollingService: {
    getInstance: vi.fn(),
    start: vi.fn(),
    stop: vi.fn(),
    subscribe: vi.fn(() => vi.fn()),
  },
}));

const mockGetData = vi.fn();
vi.mock('../../../services/api/UnifiedApiClient', () => ({
  unifiedApiClient: {
    getData: (...args: unknown[]) => mockGetData(...args),
  },
}));

import { BotsDataProvider, useBotsData } from './BotsDataContext';

const wrapper = ({ children }: { children: React.ReactNode }) => (
  <BotsDataProvider>{children}</BotsDataProvider>
);

describe('BotsDataContext', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.clearAllMocks();
    mockGetData.mockResolvedValue({ data: { connected: false } });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('provides initial botsData state', () => {
    const { result } = renderHook(() => useBotsData(), { wrapper });

    expect(result.current.botsData).toBeDefined();
    expect(result.current.botsData!.agents).toEqual([]);
    expect(result.current.botsData!.nodeCount).toBe(0);
    expect(result.current.botsData!.dataSource).toBe('live');
  });

  it('exposes updateBotsData function', () => {
    const { result } = renderHook(() => useBotsData(), { wrapper });

    act(() => {
      result.current.updateBotsData({
        nodeCount: 5,
        edgeCount: 3,
        tokenCount: 100,
        mcpConnected: true,
        dataSource: 'live',
        agents: [],
        edges: [],
      });
    });

    expect(result.current.botsData!.nodeCount).toBe(5);
    expect(result.current.botsData!.mcpConnected).toBe(true);
  });

  it('exposes updateFromFullUpdate function', () => {
    const { result } = renderHook(() => useBotsData(), { wrapper });

    act(() => {
      result.current.updateFromFullUpdate({
        agents: [{ id: 'a1', name: 'Agent 1', type: 'researcher', status: 'active' }] as any,
        multiAgentMetrics: {
          totalAgents: 1,
          activeAgents: 1,
          totalTasks: 5,
          completedTasks: 3,
          avgSuccessRate: 0.9,
          totalTokens: 5000,
        },
        timestamp: new Date().toISOString(),
      });
    });

    expect(result.current.botsData!.nodeCount).toBe(1);
    expect(result.current.botsData!.tokenCount).toBe(5000);
  });

  it('exposes polling control functions', () => {
    const { result } = renderHook(() => useBotsData(), { wrapper });

    expect(result.current.pollingStatus).toBeDefined();
    expect(typeof result.current.pollNow).toBe('function');
    expect(typeof result.current.configurePolling).toBe('function');
  });

  it('throws when useBotsData is used outside provider', () => {
    expect(() => {
      renderHook(() => useBotsData());
    }).toThrow('useBotsData must be used within a BotsDataProvider');
  });

  it('polls MCP status periodically', async () => {
    renderHook(() => useBotsData(), { wrapper });

    // First poll is deferred by 3000ms
    await act(async () => {
      vi.advanceTimersByTime(3500);
    });

    expect(mockGetData).toHaveBeenCalledWith('/bots/status');
  });
});
