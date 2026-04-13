import { ClientCore } from '../../services/vircadia/VircadiaClientCore';
import { createLogger } from '../../utils/loggerConfig';
import type { XRNetworkAdapter } from './XRNetworkAdapter';

const logger = createLogger('VircadiaAdapter');

export interface VircadiaAdapterConfig {
  serverUrl?: string;
  authToken?: string;
  authProvider?: string;
  reconnectAttempts?: number;
  reconnectDelay?: number;
  debug?: boolean;
}

/**
 * VircadiaAdapter - ADR-033
 *
 * Extracts Vircadia connection logic from Quest3AutoDetector into a
 * standalone XRNetworkAdapter implementation.
 */
export class VircadiaAdapter implements XRNetworkAdapter {
  private client: ClientCore | null = null;
  private connected = false;
  private listeners: Array<(connected: boolean) => void> = [];
  private readonly config: Required<VircadiaAdapterConfig>;

  constructor(config: VircadiaAdapterConfig = {}) {
    this.config = {
      serverUrl:
        config.serverUrl ??
        (typeof import.meta !== 'undefined'
          ? (import.meta as any).env?.VITE_VIRCADIA_SERVER_URL
          : undefined) ??
        'ws://localhost:3020/world/ws',
      authToken:
        config.authToken ??
        (typeof import.meta !== 'undefined'
          ? (import.meta as any).env?.VITE_VIRCADIA_AUTH_TOKEN
          : undefined) ??
        'system-token',
      authProvider:
        config.authProvider ??
        (typeof import.meta !== 'undefined'
          ? (import.meta as any).env?.VITE_VIRCADIA_AUTH_PROVIDER
          : undefined) ??
        'system',
      reconnectAttempts: config.reconnectAttempts ?? 5,
      reconnectDelay: config.reconnectDelay ?? 5000,
      debug:
        config.debug ??
        (typeof import.meta !== 'undefined'
          ? (import.meta as any).env?.DEV
          : false) ??
        false,
    };
  }

  async connect(): Promise<void> {
    try {
      logger.info('Initializing Vircadia connection for Quest 3 XR...');

      this.client = new ClientCore({
        serverUrl: this.config.serverUrl,
        authToken: this.config.authToken,
        authProvider: this.config.authProvider,
        reconnectAttempts: this.config.reconnectAttempts,
        reconnectDelay: this.config.reconnectDelay,
        debug: this.config.debug,
        suppress: false,
      });

      const connectionInfo =
        await this.client.Utilities.Connection.connect({
          timeoutMs: 10000,
        });

      this.connected = true;
      this.notifyListeners(true);

      logger.info('Vircadia connected for Quest 3 XR', {
        agentId: connectionInfo.agentId,
        sessionId: connectionInfo.sessionId,
      });
    } catch (error) {
      logger.error('Failed to initialize Vircadia connection:', error);
      // Non-fatal: mirror original behaviour
    }
  }

  async disconnect(): Promise<void> {
    if (this.client) {
      logger.info('Disconnecting Vircadia client');
      this.client.dispose();
      this.client = null;
      this.connected = false;
      this.notifyListeners(false);
    }
  }

  isConnected(): boolean {
    return this.connected;
  }

  onStateChange(cb: (connected: boolean) => void): void {
    this.listeners.push(cb);
  }

  /** Expose the underlying ClientCore for callers that need direct access. */
  getClient(): ClientCore | null {
    return this.client;
  }

  private notifyListeners(connected: boolean): void {
    for (const cb of this.listeners) {
      try {
        cb(connected);
      } catch (err) {
        logger.error('Error in state-change listener:', err);
      }
    }
  }
}
