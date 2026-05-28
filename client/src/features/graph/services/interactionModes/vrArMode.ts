import { Vector3 } from 'three';
import { createLogger } from '../../../../utils/loggerConfig';
import type { VRARState, SpatialUI, ImmersiveInteraction } from './types';

const logger = createLogger('VRArMode');

export function createVRARState(): VRARState {
  return {
    isActive: false,
    mode: 'VR',
    handTracking: false,
    eyeTracking: false,
    hapticFeedback: false,
    spatialAudio: false,
    immersiveUI: false,
    roomScale: false,
    passthrough: false
  };
}

export function createSpatialUI(): SpatialUI {
  return {
    panels: [],
    menus: [],
    notifications: [],
    workspace: {
      center: new Vector3(0, 0, 0),
      bounds: {
        min: new Vector3(-50, -50, -50),
        max: new Vector3(50, 50, 50)
      },
      scale: 1.0,
      orientation: new Vector3(0, 0, 0),
      snapPoints: []
    }
  };
}

export function activateVRMode(
  state: VRARState,
  options: Partial<VRARState>,
  emit: (event: string, data?: any) => void
): { vrArState: VRARState; spatialUI: SpatialUI } {
  logger.info('Activating VR mode');
  const vrArState: VRARState = { ...state, isActive: true, mode: 'VR', ...options };
  const spatialUI = initializeSpatialUI();
  setupVRControls(vrArState);
  emit('vrActivated', vrArState);
  return { vrArState, spatialUI };
}

export function activateARMode(
  state: VRARState,
  options: Partial<VRARState>,
  emit: (event: string, data?: any) => void
): { vrArState: VRARState; spatialUI: SpatialUI } {
  logger.info('Activating AR mode');
  const vrArState: VRARState = { ...state, isActive: true, mode: 'AR', passthrough: true, ...options };
  const spatialUI = initializeSpatialUI();
  setupARControls();
  emit('arActivated', vrArState);
  return { vrArState, spatialUI };
}

function initializeSpatialUI(): SpatialUI {
  return {
    panels: [
      {
        id: 'main-controls',
        position: new Vector3(-2, 1, -1),
        rotation: new Vector3(0, 0.3, 0),
        size: { width: 1.5, height: 1 },
        content: 'graph_controls',
        isVisible: true,
        canMove: true,
        canResize: false
      },
      {
        id: 'information',
        position: new Vector3(2, 1, -1),
        rotation: new Vector3(0, -0.3, 0),
        size: { width: 1.2, height: 0.8 },
        content: 'information',
        isVisible: true,
        canMove: true,
        canResize: false
      }
    ],
    menus: [],
    notifications: [],
    workspace: {
      center: new Vector3(0, 0, 0),
      bounds: {
        min: new Vector3(-20, -10, -20),
        max: new Vector3(20, 10, 20)
      },
      scale: 1.0,
      orientation: new Vector3(0, 0, 0),
      snapPoints: [
        new Vector3(0, 0, 0),
        new Vector3(-5, 0, 0),
        new Vector3(5, 0, 0),
        new Vector3(0, 0, -5),
        new Vector3(0, 0, 5)
      ]
    }
  };
}

function setupVRControls(state: VRARState): void {
  if (state.handTracking) logger.info('Hand tracking enabled');
  if (state.eyeTracking) logger.info('Eye tracking enabled');
  if (state.hapticFeedback) logger.info('Haptic feedback enabled');
}

function setupARControls(): void {
  logger.info('World tracking setup');
  logger.info('Occlusion setup');
  logger.info('Light estimation setup');
}

export function processImmersiveInteraction(
  interaction: ImmersiveInteraction,
  spatialUI: SpatialUI,
  emit: (event: string, data?: any) => void,
  onNextWaypoint: () => void,
  onPreviousWaypoint: () => void,
  onStartTour: (id: string) => void
): SpatialUI {
  logger.info(`Processing immersive interaction: ${interaction.type}`);
  let nextUI = spatialUI;

  switch (interaction.type) {
    case 'hand_grab':
      nextUI = handleHandGrab(interaction, spatialUI, emit);
      break;
    case 'eye_select':
      handleEyeSelect(interaction, emit);
      break;
    case 'voice_command':
      nextUI = handleVoiceCommand(interaction, spatialUI, emit, onNextWaypoint, onStartTour);
      break;
    case 'gesture':
      nextUI = handleGesture(interaction, spatialUI, emit, onNextWaypoint, onPreviousWaypoint);
      break;
    case 'haptic_tap':
      handleHapticTap(interaction, emit);
      break;
  }

  emit('immersiveInteraction', interaction);
  return nextUI;
}

function handleHandGrab(
  interaction: ImmersiveInteraction,
  spatialUI: SpatialUI,
  emit: (event: string, data?: any) => void
): SpatialUI {
  const target = interaction.targetElement;
  if (target.startsWith('node-')) {
    emit('nodeSelected', { nodeId: target.replace('node-', '') });
    return spatialUI;
  }
  if (target.startsWith('panel-')) {
    return movePanelInUI(spatialUI, target.replace('panel-', ''), interaction.parameters.position, emit);
  }
  return spatialUI;
}

function handleEyeSelect(
  interaction: ImmersiveInteraction,
  emit: (event: string, data?: any) => void
): void {
  if (interaction.confidence > 0.8) {
    emit('elementHighlighted', { elementId: interaction.targetElement });
  }
}

function handleVoiceCommand(
  interaction: ImmersiveInteraction,
  spatialUI: SpatialUI,
  emit: (event: string, data?: any) => void,
  onNextWaypoint: () => void,
  onStartTour: (id: string) => void
): SpatialUI {
  const command = interaction.parameters.command;
  switch (command) {
    case 'show information':
      return setPanelVisible(spatialUI, 'information', true, emit);
    case 'hide controls':
      return setPanelVisible(spatialUI, 'main-controls', false, emit);
    case 'start tour':
      onStartTour('default');
      break;
    case 'reset view':
      return resetWorkspaceView(spatialUI, emit);
  }
  return spatialUI;
}

function handleGesture(
  interaction: ImmersiveInteraction,
  spatialUI: SpatialUI,
  emit: (event: string, data?: any) => void,
  onNextWaypoint: () => void,
  onPreviousWaypoint: () => void
): SpatialUI {
  const gesture = interaction.parameters.gesture;
  switch (gesture) {
    case 'pinch':
      return scaleWorkspace(spatialUI, interaction.parameters.scale, emit);
    case 'swipe_left':
      onNextWaypoint();
      break;
    case 'swipe_right':
      onPreviousWaypoint();
      break;
    case 'point':
      emit('elementHighlighted', { elementId: interaction.targetElement });
      break;
  }
  return spatialUI;
}

function handleHapticTap(
  interaction: ImmersiveInteraction,
  emit: (event: string, data?: any) => void
): void {
  emit('elementSelected', { elementId: interaction.targetElement });
  emit('hapticFeedback', { type: 'tap' });
}

function movePanelInUI(
  spatialUI: SpatialUI,
  panelId: string,
  position: Vector3,
  emit: (event: string, data?: any) => void
): SpatialUI {
  const panels = spatialUI.panels.map(p =>
    p.id === panelId && p.canMove ? { ...p, position } : p
  );
  if (panels.some((p, i) => p !== spatialUI.panels[i])) {
    emit('panelMoved', { panelId, position });
  }
  return { ...spatialUI, panels };
}

function setPanelVisible(
  spatialUI: SpatialUI,
  panelId: string,
  isVisible: boolean,
  emit: (event: string, data?: any) => void
): SpatialUI {
  const panels = spatialUI.panels.map(p => (p.id === panelId ? { ...p, isVisible } : p));
  emit(isVisible ? 'panelShown' : 'panelHidden', { panelId });
  return { ...spatialUI, panels };
}

function resetWorkspaceView(
  spatialUI: SpatialUI,
  emit: (event: string, data?: any) => void
): SpatialUI {
  const workspace = { ...spatialUI.workspace, scale: 1.0, orientation: new Vector3(0, 0, 0) };
  emit('workspaceReset');
  return { ...spatialUI, workspace };
}

function scaleWorkspace(
  spatialUI: SpatialUI,
  scale: number,
  emit: (event: string, data?: any) => void
): SpatialUI {
  const workspace = { ...spatialUI.workspace, scale: spatialUI.workspace.scale * scale };
  emit('workspaceScaled', { scale: workspace.scale });
  return { ...spatialUI, workspace };
}
