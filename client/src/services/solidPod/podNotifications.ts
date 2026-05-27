/**
 * Pod Notifications
 *
 * Manages the WebSocket connection to a Solid/JSS server for the
 * solid-0.1 notification protocol:
 * - Connect / disconnect with exponential-backoff reconnect
 * - Subscribe / unsubscribe to resource URLs
 * - Route incoming pub/ack messages to registered callbacks
 * - Integrates with WebSocketRegistry and WebSocketEventBus
 */

import { createLogger } from '../../utils/loggerConfig';
import { webSocketRegistry } from '../WebSocketRegistry';
import { webSocketEventBus } from '../WebSocketEventBus';

const logger = createLogger('SolidPodService:ws');

export const JSS_WS_URL = import.meta.env.VITE_JSS_WS_URL || null;

const REGISTRY_NAME = 'solid-pod';

export interface SolidNotification {
  type: 'pub' | 'ack';
  url: string;
}

type NotificationCallback = (notification: SolidNotification) => void;

export class PodNotificationManager {
  private wsConnection: WebSocket | null = null;
  private subscriptions: Map<string, Set<NotificationCallback>> = new Map();
  private reconnectAttempts = 0;
  private readonly maxReconnectAttempts = 5;
  private readonly reconnectDelay = 1000;
  private reconnectTimerId: ReturnType<typeof setTimeout> | null = null;
  private isDisconnecting = false;

  /** Connect to JSS WebSocket for real-time notifications. */
  connect(): void {
    if (!JSS_WS_URL) {
      logger.warn('JSS WebSocket URL not configured');
      return;
    }

    if (this.wsConnection?.readyState === WebSocket.OPEN) {
      logger.debug('WebSocket already connected');
      return;
    }

    try {
      const validatedUrl = new URL(JSS_WS_URL);
      if (validatedUrl.protocol !== 'ws:' && validatedUrl.protocol !== 'wss:') {
        logger.error('Invalid WebSocket protocol', { protocol: validatedUrl.protocol });
        return;
      }

      this.wsConnection = new WebSocket(validatedUrl.href);

      this.wsConnection.onopen = () => {
        logger.info('JSS WebSocket connected');
        this.reconnectAttempts = 0;
        webSocketRegistry.register(REGISTRY_NAME, validatedUrl.href, this.wsConnection!);
        webSocketEventBus.emit('connection:open', { name: REGISTRY_NAME, url: validatedUrl.href });
      };

      this.wsConnection.onmessage = (event) => {
        const msg = event.data.toString().trim();
        webSocketEventBus.emit('message:pod', { data: msg });
        this.handleMessage(msg);
      };

      this.wsConnection.onerror = (error) => {
        logger.error('JSS WebSocket error', { error });
        webSocketEventBus.emit('connection:error', { name: REGISTRY_NAME, error });
      };

      this.wsConnection.onclose = (event) => {
        logger.info('JSS WebSocket disconnected');
        webSocketRegistry.unregister(REGISTRY_NAME);
        webSocketEventBus.emit('connection:close', {
          name: REGISTRY_NAME,
          code: event.code,
          reason: event.reason,
        });
        if (this.isDisconnecting) {
          this.isDisconnecting = false;
          return;
        }
        this.handleReconnect();
      };
    } catch (error) {
      logger.error('Failed to connect WebSocket', { error });
    }
  }

  /** Subscribe to notifications for a resource URL. Returns an unsubscribe fn. */
  subscribe(resourceUrl: string, callback: NotificationCallback): () => void {
    if (!this.subscriptions.has(resourceUrl)) {
      this.subscriptions.set(resourceUrl, new Set());
      if (this.wsConnection?.readyState === WebSocket.OPEN) {
        this.wsConnection.send(`sub ${resourceUrl}`);
      }
    }

    this.subscriptions.get(resourceUrl)!.add(callback);

    return () => {
      this.subscriptions.get(resourceUrl)?.delete(callback);
      if (this.subscriptions.get(resourceUrl)?.size === 0) {
        if (this.wsConnection?.readyState === WebSocket.OPEN) {
          this.wsConnection.send(`unsub ${resourceUrl}`);
        }
        this.subscriptions.delete(resourceUrl);
      }
    };
  }

  /** Close the WebSocket and cancel any pending reconnect timer. */
  disconnect(): void {
    if (this.reconnectTimerId !== null) {
      clearTimeout(this.reconnectTimerId);
      this.reconnectTimerId = null;
    }
    this.isDisconnecting = true;
    webSocketRegistry.unregister(REGISTRY_NAME);
    if (this.wsConnection) {
      this.wsConnection.close();
      this.wsConnection = null;
    }
    this.subscriptions.clear();
  }

  /** Whether a WebSocket connection is currently requested. */
  get isConnected(): boolean {
    return this.wsConnection?.readyState === WebSocket.OPEN;
  }

  // -------------------------------------------------------------------------
  // Private
  // -------------------------------------------------------------------------

  private handleMessage(msg: string): void {
    if (msg.startsWith('protocol ')) {
      logger.debug('WebSocket protocol handshake complete');
      for (const url of this.subscriptions.keys()) {
        this.wsConnection?.send(`sub ${url}`);
      }
    } else if (msg.startsWith('ack ')) {
      const url = msg.slice(4);
      logger.debug('Subscription acknowledged', { url });
      this.notifySubscribers(url, { type: 'ack', url });
    } else if (msg.startsWith('pub ')) {
      const url = msg.slice(4);
      logger.debug('Resource changed', { url });
      this.notifySubscribers(url, { type: 'pub', url });
    }
  }

  private notifySubscribers(url: string, notification: SolidNotification): void {
    this.subscriptions.get(url)?.forEach((cb) => cb(notification));

    // Also notify container (parent directory) subscribers
    const containerUrl = url.substring(0, url.lastIndexOf('/') + 1);
    if (containerUrl !== url) {
      this.subscriptions.get(containerUrl)?.forEach((cb) => cb(notification));
    }
  }

  private handleReconnect(): void {
    if (this.reconnectAttempts >= this.maxReconnectAttempts) {
      logger.warn('Max reconnect attempts reached');
      return;
    }

    this.reconnectAttempts++;
    const delay = this.reconnectDelay * Math.pow(2, this.reconnectAttempts - 1);
    logger.info(`Reconnecting in ${delay}ms (attempt ${this.reconnectAttempts})`);

    this.reconnectTimerId = setTimeout(() => {
      this.reconnectTimerId = null;
      this.connect();
    }, delay);
  }
}
