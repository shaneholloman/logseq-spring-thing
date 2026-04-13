/**
 * XRNetworkAdapter - ADR-033
 *
 * Strategy interface for XR network connectivity.
 * Implementations handle specific backends (Vircadia, null/no-op, etc.)
 * while Quest3AutoDetector remains backend-agnostic.
 */
export interface XRNetworkAdapter {
  connect(): Promise<void>;
  disconnect(): Promise<void>;
  isConnected(): boolean;
  onStateChange(cb: (connected: boolean) => void): void;
}
