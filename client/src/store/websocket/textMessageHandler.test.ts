import { describe, it, expect, beforeEach, vi } from 'vitest';

vi.mock('../../utils/loggerConfig', () => ({
  createLogger: () => ({
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
    debug: vi.fn(),
  }),
  createErrorMetadata: (e: unknown) => ({ error: e }),
}));

vi.mock('../../utils/clientDebugState', () => ({
  debugState: {
    isEnabled: () => false,
    isDataDebugEnabled: () => false,
  },
}));

const mockSetGraphData = vi.fn().mockResolvedValue(undefined);
vi.mock('../../features/graph/managers/graphDataManager', () => ({
  graphDataManager: {
    setGraphData: mockSetGraphData,
    nodeIdMap: new Map(),
  },
}));

const mockEmit = vi.fn();
const mockNotifyMessageHandlers = vi.fn();
vi.mock('./connectionManager', () => ({
  emit: (...args: unknown[]) => mockEmit(...args),
  notifyMessageHandlers: (...args: unknown[]) => mockNotifyMessageHandlers(...args),
}));

const mockHandleErrorFrame = vi.fn();
vi.mock('./binaryProtocol', () => ({
  handleErrorFrame: (...args: unknown[]) => mockHandleErrorFrame(...args),
}));

const mockMerge = vi.fn();
vi.mock('../analyticsStore', () => ({
  useAnalyticsStore: {
    getState: () => ({ merge: mockMerge }),
  },
}));

import { handleTextMessage } from './textMessageHandler';

describe('textMessageHandler', () => {
  const mockGet = vi.fn(() => ({ forceReconnect: vi.fn() }));
  const mockSet = vi.fn();
  const mockProcessQueue = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('sets isServerReady on connection_established', () => {
    handleTextMessage(
      { type: 'connection_established' } as any,
      mockGet,
      mockSet,
      mockProcessQueue,
    );

    expect(mockSet).toHaveBeenCalledWith({ isServerReady: true });
  });

  it('delegates error messages to handleErrorFrame and returns early', () => {
    const msg = { type: 'error', error: { code: 'E001', message: 'fail' } };

    handleTextMessage(msg as any, mockGet, mockSet, mockProcessQueue);

    expect(mockHandleErrorFrame).toHaveBeenCalledWith(
      { code: 'E001', message: 'fail' },
      mockGet,
      mockProcessQueue,
    );
    // Should NOT reach notifyMessageHandlers
    expect(mockNotifyMessageHandlers).not.toHaveBeenCalled();
  });

  it('emits filterApplied on filter_update_success', () => {
    const msg = {
      type: 'filter_update_success',
      data: { visible_nodes: 42, total_nodes: 100 },
    };

    handleTextMessage(msg as any, mockGet, mockSet, mockProcessQueue);

    expect(mockEmit).toHaveBeenCalledWith('filterApplied', {
      visibleNodes: 42,
      totalNodes: 100,
    });
  });

  it('emits memoryFlash on memory_flash message', () => {
    const payload = { embedding: [1, 2, 3] };
    const msg = { type: 'memory_flash', data: payload };

    handleTextMessage(msg as any, mockGet, mockSet, mockProcessQueue);

    expect(mockEmit).toHaveBeenCalledWith('memoryFlash', payload);
  });

  it('merges analytics_update into analytics store and returns early', () => {
    const msg = { type: 'analytics_update', cluster_id: 5 };

    handleTextMessage(msg as any, mockGet, mockSet, mockProcessQueue);

    expect(mockMerge).toHaveBeenCalledWith(msg);
    expect(mockNotifyMessageHandlers).not.toHaveBeenCalled();
  });

  it('handles analytics_update merge error gracefully', () => {
    mockMerge.mockImplementationOnce(() => {
      throw new Error('merge boom');
    });

    expect(() =>
      handleTextMessage(
        { type: 'analytics_update' } as any,
        mockGet,
        mockSet,
        mockProcessQueue,
      ),
    ).not.toThrow();
  });

  it('calls notifyMessageHandlers for unknown message types', () => {
    const msg = { type: 'custom_event', payload: 'data' };

    handleTextMessage(msg as any, mockGet, mockSet, mockProcessQueue);

    expect(mockNotifyMessageHandlers).toHaveBeenCalledWith(msg);
  });

  it('handles initialGraphLoad when nodeIdMap is empty', () => {
    const msg = {
      type: 'initialGraphLoad',
      nodes: [{ id: 1, label: 'Node 1', node_type: 'page' }],
      edges: [{ id: 'e1', source: '1', target: '2' }],
    };

    handleTextMessage(msg as any, mockGet, mockSet, mockProcessQueue);

    expect(mockSetGraphData).toHaveBeenCalled();
  });
});
