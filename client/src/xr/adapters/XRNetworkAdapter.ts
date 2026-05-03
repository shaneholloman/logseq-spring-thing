/**
 * XRNetworkAdapter - originally ADR-033, retained under ADR-071.
 *
 * Strategy interface for XR network connectivity. Quest3AutoDetector remains
 * backend-agnostic; in the current ADR-071 substrate only `NullAdapter` is
 * shipped from the browser side, with multi-user presence handled by the
 * Godot APK against the `/ws/presence` route.
 */
export interface XRNetworkAdapter {
  connect(): Promise<void>;
  disconnect(): Promise<void>;
  isConnected(): boolean;
  onStateChange(cb: (connected: boolean) => void): void;
}
