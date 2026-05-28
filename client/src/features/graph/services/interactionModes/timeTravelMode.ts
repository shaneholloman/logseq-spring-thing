import { createLogger } from '../../../../utils/loggerConfig';
import type { GraphData } from '../../managers/graphDataManager';
import type { TimeTravelState } from './types';

const logger = createLogger('TimeTravelMode');

export function createTimeTravelState(): TimeTravelState {
  return {
    isActive: false,
    currentStep: 0,
    totalSteps: 0,
    graphStates: [],
    animationSpeed: 1.0,
    isPlaying: false,
    playbackDirection: 'forward'
  };
}

export function activateTimeTravelMode(
  state: TimeTravelState,
  graphStates: GraphData[],
  options: {
    animationSpeed?: number;
    startStep?: number;
    onStateChange?: (step: number, graphData: GraphData) => void;
  } = {}
): TimeTravelState {
  logger.info('Activating time-travel mode');
  return {
    isActive: true,
    currentStep: options.startStep || 0,
    totalSteps: graphStates.length,
    graphStates,
    animationSpeed: options.animationSpeed || 1.0,
    isPlaying: false,
    playbackDirection: 'forward',
    onStateChange: options.onStateChange
  };
}

export function seekTimeTravel(
  state: TimeTravelState,
  step: number,
  emit: (event: string, data?: any) => void
): TimeTravelState {
  if (!state.isActive) return state;

  const currentStep = Math.max(0, Math.min(step, state.totalSteps - 1));
  const currentGraph = state.graphStates[currentStep];
  const next: TimeTravelState = { ...state, currentStep };

  next.onStateChange?.(currentStep, currentGraph);
  emit('timeTravelSeek', next);
  return next;
}

export function startTimeTravelAnimation(
  getState: () => TimeTravelState,
  setState: (s: TimeTravelState) => void,
  emit: (event: string, data?: any) => void
): void {
  const state = getState();
  if (!state.isPlaying) return;

  const stepDuration = 1000 / state.animationSpeed;

  const animate = () => {
    const current = getState();
    if (!current.isPlaying) return;

    const direction = current.playbackDirection === 'forward' ? 1 : -1;
    const nextStep = current.currentStep + direction;

    if (nextStep >= 0 && nextStep < current.totalSteps) {
      setState(seekTimeTravel(current, nextStep, emit));
      setTimeout(animate, stepDuration);
    } else {
      setState({ ...current, isPlaying: false });
      emit('timeTravelPause', getState());
    }
  };

  setTimeout(animate, stepDuration);
}
