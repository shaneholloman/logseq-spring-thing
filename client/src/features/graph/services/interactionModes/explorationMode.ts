import { createLogger } from '../../../../utils/loggerConfig';
import type { ExplorationState, ExplorationWaypoint } from './types';

const logger = createLogger('ExplorationMode');

export function createExplorationState(): ExplorationState {
  return {
    isActive: false,
    currentTour: null,
    currentWaypoint: 0,
    isGuidedMode: true,
    highlightedElements: new Set(),
    narrativeText: '',
    autoAdvance: false
  };
}

export function startExplorationTour(
  tourId: string,
  tour: ExplorationWaypoint[],
  options: {
    autoAdvance?: boolean;
    onWaypointReached?: (waypoint: ExplorationWaypoint) => void;
  } = {}
): ExplorationState {
  logger.info(`Starting exploration tour: ${tourId}`);
  return {
    isActive: true,
    currentTour: tourId,
    currentWaypoint: 0,
    isGuidedMode: true,
    highlightedElements: new Set(),
    narrativeText: tour[0]?.description || '',
    autoAdvance: options.autoAdvance || false,
    onWaypointReached: options.onWaypointReached
  };
}

export function moveToWaypoint(
  state: ExplorationState,
  index: number,
  tour: ExplorationWaypoint[],
  emit: (event: string, data?: any) => void
): ExplorationState {
  const waypoint = tour[index];
  const highlightedElements = new Set<string>();

  waypoint.highlightNodes.forEach(nodeId => highlightedElements.add(nodeId));
  waypoint.highlightEdges.forEach(edgeId => highlightedElements.add(edgeId));

  const next: ExplorationState = {
    ...state,
    currentWaypoint: index,
    narrativeText: waypoint.description,
    highlightedElements
  };

  executeWaypointTriggers(waypoint, next, emit);

  next.onWaypointReached?.(waypoint);
  emit('waypointReached', { waypoint, index });
  return next;
}

function executeWaypointTriggers(
  waypoint: ExplorationWaypoint,
  state: ExplorationState,
  emit: (event: string, data?: any) => void
): void {
  waypoint.triggers.forEach(trigger => {
    switch (trigger.action) {
      case 'advance':
        if (state.autoAdvance) {
          setTimeout(() => emit('__advanceWaypoint'), trigger.parameters.delay || 3000);
        }
        break;
      case 'highlight':
        break;
      case 'show_element':
        waypoint.interactiveElements.forEach(element => {
          element.isVisible = true;
        });
        break;
      case 'play_animation':
        break;
    }
  });
}

export function finishExploration(
  state: ExplorationState,
  emit: (event: string, data?: any) => void
): ExplorationState {
  const next: ExplorationState = {
    ...state,
    isActive: false,
    highlightedElements: new Set()
  };
  emit('explorationFinished', next);
  return next;
}
