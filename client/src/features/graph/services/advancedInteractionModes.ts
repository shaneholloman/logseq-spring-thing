// Thin facade — re-exports everything from the interactionModes sub-package.
// Public API is unchanged; callers import from this file as before.

export type {
  TimeTravelState,
  ExplorationState,
  ExplorationWaypoint,
  InteractiveElement,
  WaypointTrigger,
  CollaborationState,
  CollaborationParticipant,
  ChatMessage,
  GraphAnnotation,
  CollaborationPermissions,
  VRARState,
  ImmersiveInteraction,
  SpatialUI,
  SpatialPanel,
  SpatialMenu,
  SpatialMenuItem,
  SpatialNotification,
  SpatialWorkspace
} from './interactionModes/types';

export { AdvancedInteractionModes } from './interactionModes/modeCoordinator';

import { AdvancedInteractionModes } from './interactionModes/modeCoordinator';

// Singleton instance — preserved for backward compatibility.
export const advancedInteractionModes = AdvancedInteractionModes.getInstance();
