/**
 * serviceCompat.ts — Backward-compatible service wrapper
 *
 * Wraps the Zustand store in a class-based singleton so existing code that
 * imports `webSocketService` continues to work without changes.
 */

import { useWebSocketStore } from './index';
import type {
  WebSocketState,
  WebSocketErrorFrame,
  FilterUpdateParams,
  SolidNotificationCallback,
} from './types';

class WebSocketServiceCompat {
  private static _instance: WebSocketServiceCompat | null = null;

  public static getInstance(): WebSocketServiceCompat {
    if (!WebSocketServiceCompat._instance) {
      WebSocketServiceCompat._instance = new WebSocketServiceCompat();
    }
    return WebSocketServiceCompat._instance;
  }

  public static resetInstance(): void {
    if (WebSocketServiceCompat._instance) {
      useWebSocketStore.getState()._reset();
    }
    WebSocketServiceCompat._instance = null;
  }

  connect = () => useWebSocketStore.getState().connect();
  disconnect = () => useWebSocketStore.getState().disconnect();
  close = () => useWebSocketStore.getState().close();
  sendMessage = (type: string, data?: unknown) => useWebSocketStore.getState().sendMessage(type, data);
  sendRawBinaryData = (data: ArrayBuffer) => useWebSocketStore.getState().sendRawBinaryData(data);
  sendFilterUpdate = (filter: FilterUpdateParams) => useWebSocketStore.getState().sendFilterUpdate(filter);
  forceRefreshFilter = () => useWebSocketStore.getState().forceRefreshFilter();
  forceReconnect = () => useWebSocketStore.getState().forceReconnect();
  clearMessageQueue = () => useWebSocketStore.getState().clearMessageQueue();
  setCustomBackendUrl = (url: string | null) => useWebSocketStore.getState().setCustomBackendUrl(url);
  sendNodePositionUpdates = (updates: Parameters<WebSocketState['sendNodePositionUpdates']>[0]) => useWebSocketStore.getState().sendNodePositionUpdates(updates);
  flushPositionUpdates = () => useWebSocketStore.getState().flushPositionUpdates();
  getPositionQueueMetrics = () => useWebSocketStore.getState().getPositionQueueMetrics();
  onMessage = (handler: Parameters<WebSocketState['onMessage']>[0]) => useWebSocketStore.getState().onMessage(handler);
  onBinaryMessage = (handler: Parameters<WebSocketState['onBinaryMessage']>[0]) => useWebSocketStore.getState().onBinaryMessage(handler);
  onConnectionStatusChange = (handler: Parameters<WebSocketState['onConnectionStatusChange']>[0]) => useWebSocketStore.getState().onConnectionStatusChange(handler);
  onConnectionStateChange = (handler: Parameters<WebSocketState['onConnectionStateChange']>[0]) => useWebSocketStore.getState().onConnectionStateChange(handler);
  on = (eventName: string, handler: Parameters<WebSocketState['on']>[1]) => useWebSocketStore.getState().on(eventName, handler);
  emit = (eventName: string, data: unknown) => useWebSocketStore.getState().emit(eventName, data);
  connectSolid = () => useWebSocketStore.getState().connectSolid();
  disconnectSolid = () => useWebSocketStore.getState().disconnectSolid();
  subscribeSolidResource = (url: string, cb: SolidNotificationCallback) => useWebSocketStore.getState().subscribeSolidResource(url, cb);
  unsubscribeSolidResource = (url: string) => useWebSocketStore.getState().unsubscribeSolidResource(url);
  isSolidWebSocketConnected = () => useWebSocketStore.getState().isSolidWebSocketConnected();
  getSolidSubscriptions = () => useWebSocketStore.getState().getSolidSubscriptions();
  isReady = () => useWebSocketStore.getState().isReady();
  getConnectionState = () => useWebSocketStore.getState().getConnectionState();
  getQueuedMessageCount = () => useWebSocketStore.getState().getQueuedMessageCount();

  get isConnected(): boolean {
    return useWebSocketStore.getState().isConnected;
  }

  sendErrorFrame = (error: Partial<WebSocketErrorFrame>) => {
    const errorFrame: WebSocketErrorFrame = {
      code: error.code || 'CLIENT_ERROR',
      message: error.message || 'Unknown client error',
      category: error.category || 'protocol',
      retryable: error.retryable ?? false,
      timestamp: Date.now(),
      ...error
    };

    useWebSocketStore.getState().sendMessage('error', { error: errorFrame });
  };

  // WebSocketAdapter interface compatibility
  send = (data: ArrayBuffer) => useWebSocketStore.getState().sendRawBinaryData(data);
}

// Export singleton for backward compatibility
export const webSocketService = WebSocketServiceCompat.getInstance();

// Export class for testing
export { WebSocketServiceCompat };
