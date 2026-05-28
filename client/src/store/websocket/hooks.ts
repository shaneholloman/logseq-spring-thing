/**
 * hooks.ts — utility React hooks for common WebSocket store selections
 *
 * Extracted from index.ts to keep the store factory under the 500-line
 * project limit. Re-exported from index.ts so the public API is unchanged.
 */

import { useWebSocketStore } from './index';

/** Selects connection status fields only — minimises re-renders. */
export const useWebSocketConnection = () => useWebSocketStore(state => ({
  isConnected: state.isConnected,
  isServerReady: state.isServerReady,
  connectionState: state.connectionState,
}));

/** Selects the common connect/disconnect/send actions. */
export const useWebSocketActions = () => useWebSocketStore(state => ({
  connect: state.connect,
  disconnect: state.disconnect,
  sendMessage: state.sendMessage,
}));
