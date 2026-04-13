/**
 * Immersive Mode Hooks
 *
 * React hooks for VR/XR functionality.
 */

// LOD management
export {
  useVRConnectionsLOD,
  calculateOptimalThresholds,
  getLODDistribution,
} from './useVRConnectionsLOD';
export type { LODLevel, VRConnectionsLODConfig } from './useVRConnectionsLOD';

// Hand tracking
export {
  useVRHandTracking,
  xrControllerToHandState,
  agentsToTargetNodes,
} from './useVRHandTracking';
export type {
  HandState,
  TargetNode,
  VRHandTrackingConfig,
  VRHandTrackingResult,
} from './useVRHandTracking';

// Hand tracking session update
export { updateHandTrackingFromSession } from './updateHandTrackingFromSession';

// Canonical VR types
export type { AgentData, AgentStatus, HandIdentity, XRHandedness } from '../types';
export { toHandIdentity } from '../types';

// Re-export existing hooks if present
export * from './useImmersiveData';
