import { Vector3, Color } from 'three';
import type { GraphData } from '../../managers/graphDataManager';

export interface TimeTravelState {
  isActive: boolean;
  currentStep: number;
  totalSteps: number;
  graphStates: GraphData[];
  animationSpeed: number;
  isPlaying: boolean;
  playbackDirection: 'forward' | 'backward';
  onStateChange?: (step: number, graphData: GraphData) => void;
}

export interface ExplorationState {
  isActive: boolean;
  currentTour: string | null;
  currentWaypoint: number;
  isGuidedMode: boolean;
  highlightedElements: Set<string>;
  narrativeText: string;
  autoAdvance: boolean;
  onWaypointReached?: (waypoint: ExplorationWaypoint) => void;
}

export interface ExplorationWaypoint {
  id: string;
  position: Vector3;
  target: Vector3;
  duration: number;
  title: string;
  description: string;
  highlightNodes: string[];
  highlightEdges: string[];
  interactiveElements: InteractiveElement[];
  triggers: WaypointTrigger[];
}

export interface InteractiveElement {
  type: 'info_panel' | 'mini_graph' | 'statistics' | 'comparison' | 'quiz';
  position: Vector3;
  content: any;
  isVisible: boolean;
}

export interface WaypointTrigger {
  type: 'proximity' | 'interaction' | 'time' | 'completion';
  parameters: any;
  action: 'advance' | 'highlight' | 'show_element' | 'play_animation';
}

export interface CollaborationState {
  isActive: boolean;
  sessionId: string;
  participants: CollaborationParticipant[];
  sharedCursor: Map<string, Vector3>;
  sharedSelections: Map<string, Set<string>>;
  chatMessages: ChatMessage[];
  annotations: GraphAnnotation[];
  permissions: CollaborationPermissions;
}

export interface CollaborationParticipant {
  id: string;
  name: string;
  color: Color;
  isActive: boolean;
  lastActivity: number;
  cursorPosition: Vector3;
  currentSelection: Set<string>;
  permissions: string[];
}

export interface ChatMessage {
  id: string;
  participantId: string;
  message: string;
  timestamp: number;
  type: 'text' | 'annotation' | 'highlight' | 'suggestion';
  attachedElements?: string[];
}

export interface GraphAnnotation {
  id: string;
  creatorId: string;
  position: Vector3;
  content: string;
  type: 'note' | 'question' | 'explanation' | 'warning';
  attachedNodes: string[];
  visibility: 'private' | 'shared' | 'public';
  reactions: Map<string, string>;
}

export interface CollaborationPermissions {
  canEdit: boolean;
  canAnnotate: boolean;
  canHighlight: boolean;
  canUseVoiceChat: boolean;
  canModifyLayout: boolean;
  canCreateTours: boolean;
}

export interface VRARState {
  isActive: boolean;
  mode: 'VR' | 'AR' | 'mixed';
  handTracking: boolean;
  eyeTracking: boolean;
  hapticFeedback: boolean;
  spatialAudio: boolean;
  immersiveUI: boolean;
  roomScale: boolean;
  passthrough: boolean;
}

export interface ImmersiveInteraction {
  type: 'hand_grab' | 'eye_select' | 'voice_command' | 'gesture' | 'haptic_tap';
  targetElement: string;
  parameters: any;
  confidence: number;
  timestamp: number;
}

export interface SpatialUI {
  panels: SpatialPanel[];
  menus: SpatialMenu[];
  notifications: SpatialNotification[];
  workspace: SpatialWorkspace;
}

export interface SpatialPanel {
  id: string;
  position: Vector3;
  rotation: Vector3;
  size: { width: number; height: number };
  content: 'graph_controls' | 'information' | 'tools' | 'collaboration' | 'settings';
  isVisible: boolean;
  canMove: boolean;
  canResize: boolean;
}

export interface SpatialMenu {
  id: string;
  triggerType: 'hand' | 'eye' | 'voice';
  position: Vector3;
  items: SpatialMenuItem[];
  isVisible: boolean;
  autoHide: boolean;
}

export interface SpatialMenuItem {
  id: string;
  label: string;
  icon: string;
  action: () => void;
  isEnabled: boolean;
  submenu?: SpatialMenuItem[];
}

export interface SpatialNotification {
  id: string;
  type: 'info' | 'warning' | 'error' | 'success';
  message: string;
  position: Vector3;
  duration: number;
  isVisible: boolean;
}

export interface SpatialWorkspace {
  center: Vector3;
  bounds: { min: Vector3; max: Vector3 };
  scale: number;
  orientation: Vector3;
  snapPoints: Vector3[];
}
