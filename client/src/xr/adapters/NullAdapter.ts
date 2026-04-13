import type { XRNetworkAdapter } from './XRNetworkAdapter';

/**
 * NullAdapter - ADR-033
 *
 * No-op implementation of XRNetworkAdapter.
 * Used as the default when no network backend is configured,
 * keeping Quest3AutoDetector functional without Vircadia.
 */
export class NullAdapter implements XRNetworkAdapter {
  private listeners: Array<(connected: boolean) => void> = [];

  async connect(): Promise<void> {
    // No-op: resolves immediately
  }

  async disconnect(): Promise<void> {
    // No-op
  }

  isConnected(): boolean {
    return false;
  }

  onStateChange(cb: (connected: boolean) => void): void {
    this.listeners.push(cb);
  }
}
