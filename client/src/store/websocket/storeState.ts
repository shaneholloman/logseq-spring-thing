/**
 * storeState.ts — initial/reset data for the WebSocket Zustand store
 *
 * Pure data factories shared by the store's initial state and `_reset`.
 * Extracted from index.ts to keep the store factory under the 500-line
 * project limit and to guarantee initial state and reset stay in sync.
 */

import { getUrlFromSettings } from './connectionManager';
import type { WebSocketState, SolidNotificationCallback } from './types';
import type { WebSocketStatistics } from '../../types/websocketTypes';
import type { NodeType } from '../../types/binaryProtocol';

/** The data-only fields shared between initial state and reset. */
type WebSocketDataFields = Pick<
  WebSocketState,
  | 'socket'
  | 'isConnected'
  | 'isServerReady'
  | 'connectionState'
  | 'url'
  | 'solidSocket'
  | 'isSolidConnected'
  | 'solidSubscriptions'
  | 'nodeTypeMap'
  | 'messageQueue'
  | 'statistics'
  | 'reconnectAttempts'
>;

function freshStatistics(): WebSocketStatistics {
  return {
    messagesReceived: 0,
    messagesSent: 0,
    bytesReceived: 0,
    bytesSent: 0,
    connectionTime: 0,
    reconnections: 0,
    averageLatency: 0,
    messagesByType: {},
    errors: 0,
    lastActivity: Date.now(),
  };
}

function freshData(): WebSocketDataFields {
  return {
    socket: null,
    isConnected: false,
    isServerReady: false,
    connectionState: { status: 'disconnected', reconnectAttempts: 0 },
    url: getUrlFromSettings(),
    solidSocket: null,
    isSolidConnected: false,
    solidSubscriptions: new Map<string, Set<SolidNotificationCallback>>(),
    nodeTypeMap: new Map<number, NodeType>(),
    messageQueue: [],
    statistics: freshStatistics(),
    reconnectAttempts: 0,
  };
}

/** Data fields the store carries at creation; spread into the factory return. */
export function createInitialState(): WebSocketDataFields & {
  reconnectInterval: number;
  maxReconnectAttempts: number;
} {
  return {
    ...freshData(),
    reconnectInterval: 1000,
    maxReconnectAttempts: 10,
  };
}

/** Data-only patch applied by `_reset` to return the store to a clean slate. */
export function createResetPatch(): WebSocketDataFields {
  return freshData();
}
