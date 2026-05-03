import type { XRNetworkAdapter } from './XRNetworkAdapter';

/**
 * NullAdapter - originally ADR-033, now retained under ADR-071.
 *
 * No-op implementation of XRNetworkAdapter. Used as the default when no
 * network backend is configured, keeping Quest3AutoDetector functional
 * without an active multi-user XR backend. Under ADR-071 / PRD-008 the
 * browser path is desktop-only; multi-user presence is delivered through
 * the Godot APK and `crates/visionclaw-xr-presence/`.
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
