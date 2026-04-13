/** Agent status in the VR scene */
export type AgentStatus = 'active' | 'idle' | 'error' | 'warning';

/** Canonical agent data for VR targeting and visualization */
export interface AgentData {
  id: string;
  type?: string;
  position?: { x: number; y: number; z: number };
  status?: AgentStatus;
}

/** Hand identity mapping: XR uses left/right, our hooks use primary/secondary */
export type HandIdentity = 'primary' | 'secondary';
export type XRHandedness = 'left' | 'right' | 'none';

/** Map XR handedness to our hand identity */
export function toHandIdentity(handedness: XRHandedness): HandIdentity {
  return handedness === 'right' ? 'primary' : 'secondary';
}
