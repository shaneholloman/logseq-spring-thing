/**
 * websocketStore.ts — Thin re-export for backward compatibility
 *
 * All implementation has moved to ./websocket/ modules.
 * Consumers can import from either path:
 *   import { useWebSocketStore } from '../store/websocketStore';
 *   import { useWebSocketStore } from '../store/websocket';
 */
export {
  useWebSocketStore,
  webSocketService,
  WebSocketServiceCompat,
  useWebSocketConnection,
  useWebSocketActions,
} from './websocket';

export type {
  WebSocketAdapter,
  WebSocketErrorFrame,
  QueuedMessage,
  ConnectionState,
  SolidNotification,
  SolidNotificationCallback,
  WebSocketState,
  FilterUpdateParams,
  NodePositionUpdate,
  WebSocketInternals,
  FilterSnapshot,
  LegacyMessageHandler,
  BinaryMessageHandler,
  ConnectionStatusHandler,
  ConnectionStateHandler,
  EventHandler,
} from './websocket';

export { default } from './websocket';
