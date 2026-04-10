/**
 * solidWebSocket.ts — Solid (JSS) WebSocket connection management
 *
 * Handles: Solid WebSocket connect, disconnect, reconnect, message
 * dispatch, and resource subscription notifications.
 */

import { createLogger, createErrorMetadata } from '../../utils/loggerConfig';
import { webSocketRegistry } from '../../services/WebSocketRegistry';
import { webSocketEventBus } from '../../services/WebSocketEventBus';
import { emit } from './connectionManager';
import type { SolidNotification, SolidNotificationCallback } from './types';

const logger = createLogger('WebSocketStore');

// ── Constants ──────────────────────────────────────────────────────────
const SOLID_MAX_RECONNECT_ATTEMPTS = 10;
const SOLID_RECONNECT_DELAY = 1000;

// ── Encapsulated module-level state ────────────────────────────────────
let solidReconnectTimeout: number | null = null;
let solidReconnectAttempts = 0;

// ── URL helper ─────────────────────────────────────────────────────────

function getSolidWebSocketUrl(): string | null {
  return import.meta.env.VITE_JSS_WS_URL || null;
}

// ── Subscriber notification ────────────────────────────────────────────

function notifySolidSubscribers(
  solidSubscriptions: Map<string, Set<SolidNotificationCallback>>,
  url: string,
  notification: SolidNotification,
) {
  const callbacks = solidSubscriptions.get(url);
  callbacks?.forEach((cb) => {
    try {
      cb(notification);
    } catch (error) {
      logger.error('Error in Solid notification callback', { url, error });
    }
  });

  const containerUrl = url.substring(0, url.lastIndexOf('/') + 1);
  if (containerUrl !== url) {
    const containerCallbacks = solidSubscriptions.get(containerUrl);
    containerCallbacks?.forEach((cb) => {
      try {
        cb(notification);
      } catch (error) {
        logger.error('Error in Solid container notification callback', { containerUrl, error });
      }
    });
  }
}

// ── Message handling ───────────────────────────────────────────────────

function handleSolidMessage(
  get: () => { solidSubscriptions: Map<string, Set<SolidNotificationCallback>>; solidSocket: WebSocket | null },
  msg: string,
) {
  if (msg.startsWith('protocol ')) {
    const protocol = msg.slice(9);
    logger.debug('Solid WebSocket protocol handshake complete', { protocol });

    const state = get();
    for (const url of state.solidSubscriptions.keys()) {
      state.solidSocket?.send(`sub ${url}`);
      logger.debug('Resubscribed to Solid resource', { url });
    }

    emit('solid-protocol', { protocol });
  } else if (msg.startsWith('ack ')) {
    const url = msg.slice(4);
    logger.debug('Solid subscription acknowledged', { url });
    notifySolidSubscribers(get().solidSubscriptions, url, { type: 'ack', url });
  } else if (msg.startsWith('pub ')) {
    const url = msg.slice(4);
    logger.debug('Solid resource changed', { url });
    notifySolidSubscribers(get().solidSubscriptions, url, { type: 'pub', url });
    emit('solid-resource-changed', { url });
  } else if (msg.startsWith('error ')) {
    const errorMsg = msg.slice(6);
    logger.error('Solid WebSocket error message', { error: errorMsg });
    emit('solid-error', { message: errorMsg });
  }
}

// ── Reconnect ──────────────────────────────────────────────────────────

function attemptSolidReconnect(connectSolid: () => void) {
  if (solidReconnectTimeout) {
    window.clearTimeout(solidReconnectTimeout);
    solidReconnectTimeout = null;
  }

  if (solidReconnectAttempts >= SOLID_MAX_RECONNECT_ATTEMPTS) {
    logger.warn('Max Solid WebSocket reconnect attempts reached');
    return;
  }

  solidReconnectAttempts++;
  const delay = SOLID_RECONNECT_DELAY * Math.pow(2, solidReconnectAttempts - 1);

  logger.info(`Solid WebSocket reconnecting in ${delay}ms (attempt ${solidReconnectAttempts})`);

  solidReconnectTimeout = window.setTimeout(() => {
    connectSolid();
  }, delay);
}

export function resetSolidReconnect() {
  solidReconnectAttempts = 0;
  if (solidReconnectTimeout) {
    window.clearTimeout(solidReconnectTimeout);
    solidReconnectTimeout = null;
  }
}

// ── Connect / Disconnect ───────────────────────────────────────────────

export function connectSolidWebSocket(
  get: () => {
    solidSocket: WebSocket | null;
    solidSubscriptions: Map<string, Set<SolidNotificationCallback>>;
  },
  set: (partial: Record<string, unknown>) => void,
  connectSolid: () => void,
) {
  const wsUrl = getSolidWebSocketUrl();

  if (!wsUrl) {
    logger.warn('JSS WebSocket URL not configured (VITE_JSS_WS_URL)');
    return;
  }

  const state = get();
  if (state.solidSocket?.readyState === WebSocket.OPEN) {
    logger.debug('Solid WebSocket already connected');
    return;
  }

  try {
    logger.info(`Connecting to JSS WebSocket at ${wsUrl}`);
    const solidSocket = new WebSocket(wsUrl);

    solidSocket.onopen = () => {
      logger.info('JSS WebSocket connected');
      set({ isSolidConnected: true });
      solidReconnectAttempts = 0;
      webSocketRegistry.register('solid-store', wsUrl!, solidSocket);
      webSocketEventBus.emit('connection:open', { name: 'solid-store', url: wsUrl! });
      emit('solid-connected', { url: wsUrl });
    };

    solidSocket.onmessage = (event) => {
      const msg = event.data.toString().trim();
      handleSolidMessage(get, msg);
    };

    solidSocket.onerror = (error) => {
      logger.error('JSS WebSocket error', { error });
      webSocketEventBus.emit('connection:error', { name: 'solid-store', error });
      emit('solid-error', { error });
    };

    solidSocket.onclose = (event) => {
      logger.info('JSS WebSocket disconnected', { code: event.code, reason: event.reason });
      set({ isSolidConnected: false });
      webSocketRegistry.unregister('solid-store');
      webSocketEventBus.emit('connection:close', {
        name: 'solid-store',
        code: event.code,
        reason: event.reason,
      });
      emit('solid-disconnected', { code: event.code, reason: event.reason });
      attemptSolidReconnect(connectSolid);
    };

    set({ solidSocket });
  } catch (error) {
    logger.error('Failed to connect Solid WebSocket', { error });
  }
}

export function disconnectSolidWebSocket(
  get: () => { solidSocket: WebSocket | null },
  set: (partial: Record<string, unknown>) => void,
) {
  resetSolidReconnect();
  webSocketRegistry.unregister('solid-store');

  const state = get();
  if (state.solidSocket) {
    try {
      state.solidSocket.close(1000, 'Normal closure');
      logger.info('Solid WebSocket disconnected by client');
    } catch (error) {
      logger.error('Error closing Solid WebSocket:', createErrorMetadata(error));
    }
  }

  set({
    solidSocket: null,
    isSolidConnected: false,
    solidSubscriptions: new Map()
  });
}
