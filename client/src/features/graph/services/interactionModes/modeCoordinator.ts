import { Vector3 } from 'three';
import { createLogger } from '../../../../utils/loggerConfig';
import type { GraphData } from '../../managers/graphDataManager';
import type {
  TimeTravelState,
  ExplorationState,
  ExplorationWaypoint,
  CollaborationState,
  CollaborationParticipant,
  CollaborationPermissions,
  ChatMessage,
  GraphAnnotation,
  VRARState,
  SpatialUI,
  ImmersiveInteraction
} from './types';
import {
  createTimeTravelState,
  activateTimeTravelMode,
  seekTimeTravel,
  startTimeTravelAnimation
} from './timeTravelMode';
import {
  createExplorationState,
  startExplorationTour as startTour,
  moveToWaypoint,
  finishExploration
} from './explorationMode';
import {
  createCollaborationState,
  startCollaborationSession as startSession,
  addParticipant as addParticipantFn,
  updateParticipantCursor as updateCursorFn,
  updateParticipantSelection as updateSelectionFn,
  sendChatMessage as sendMessageFn,
  createAnnotation as createAnnotationFn
} from './collaborationMode';
import {
  createVRARState,
  createSpatialUI,
  activateVRMode as activateVR,
  activateARMode as activateAR,
  processImmersiveInteraction as processInteraction
} from './vrArMode';

const logger = createLogger('ModeCoordinator');

export class AdvancedInteractionModes {
  private static instance: AdvancedInteractionModes;

  private timeTravelState: TimeTravelState;
  private explorationState: ExplorationState;
  private collaborationState: CollaborationState;
  private vrArState: VRARState;
  private spatialUI: SpatialUI;

  private tours: Map<string, ExplorationWaypoint[]> = new Map();
  private activeAnimations: Map<string, any> = new Map();
  private eventListeners: Map<string, Set<Function>> = new Map();

  private constructor() {
    this.timeTravelState = createTimeTravelState();
    this.explorationState = createExplorationState();
    this.collaborationState = createCollaborationState();
    this.vrArState = createVRARState();
    this.spatialUI = createSpatialUI();
  }

  public static getInstance(): AdvancedInteractionModes {
    if (!AdvancedInteractionModes.instance) {
      AdvancedInteractionModes.instance = new AdvancedInteractionModes();
    }
    return AdvancedInteractionModes.instance;
  }

  // ── Time Travel ──────────────────────────────────────────────────────────

  public activateTimeTravelMode(
    graphStates: GraphData[],
    options: {
      animationSpeed?: number;
      startStep?: number;
      onStateChange?: (step: number, graphData: GraphData) => void;
    } = {}
  ): void {
    this.timeTravelState = activateTimeTravelMode(this.timeTravelState, graphStates, options);
    this.emit('timeTravelActivated', this.timeTravelState);
  }

  public playTimeTravel(): void {
    if (!this.timeTravelState.isActive) return;
    this.timeTravelState = { ...this.timeTravelState, isPlaying: true };
    startTimeTravelAnimation(
      () => this.timeTravelState,
      s => { this.timeTravelState = s; },
      this.emit.bind(this)
    );
    this.emit('timeTravelPlay', this.timeTravelState);
  }

  public pauseTimeTravel(): void {
    this.timeTravelState = { ...this.timeTravelState, isPlaying: false };
    this.emit('timeTravelPause', this.timeTravelState);
  }

  public seekTimeTravel(step: number): void {
    this.timeTravelState = seekTimeTravel(this.timeTravelState, step, this.emit.bind(this));
  }

  // ── Exploration ───────────────────────────────────────────────────────────

  public createExplorationTour(tourId: string, waypoints: ExplorationWaypoint[]): void {
    logger.info(`Creating exploration tour: ${tourId}`);
    this.tours.set(tourId, waypoints);
    this.emit('tourCreated', { tourId, waypoints });
  }

  public startExplorationTour(
    tourId: string,
    options: {
      autoAdvance?: boolean;
      onWaypointReached?: (waypoint: ExplorationWaypoint) => void;
    } = {}
  ): void {
    const tour = this.tours.get(tourId);
    if (!tour) { logger.error(`Tour not found: ${tourId}`); return; }

    this.explorationState = startTour(tourId, tour, options);
    this.on('__advanceWaypoint', () => this.nextWaypoint());
    this.moveToWaypoint(0);
    this.emit('explorationStarted', this.explorationState);
  }

  public nextWaypoint(): void {
    if (!this.explorationState.isActive || !this.explorationState.currentTour) return;
    const tour = this.tours.get(this.explorationState.currentTour)!;
    const nextIndex = this.explorationState.currentWaypoint + 1;
    if (nextIndex < tour.length) {
      this.moveToWaypoint(nextIndex);
    } else {
      this.explorationState = finishExploration(this.explorationState, this.emit.bind(this));
    }
  }

  public previousWaypoint(): void {
    if (!this.explorationState.isActive) return;
    const prevIndex = this.explorationState.currentWaypoint - 1;
    if (prevIndex >= 0) this.moveToWaypoint(prevIndex);
  }

  private moveToWaypoint(index: number): void {
    if (!this.explorationState.currentTour) return;
    const tour = this.tours.get(this.explorationState.currentTour)!;
    this.explorationState = moveToWaypoint(
      this.explorationState,
      index,
      tour,
      this.emit.bind(this)
    );
  }

  // ── Collaboration ─────────────────────────────────────────────────────────

  public startCollaborationSession(
    sessionId: string,
    permissions: Partial<CollaborationPermissions> = {}
  ): void {
    this.collaborationState = startSession(
      this.collaborationState,
      sessionId,
      permissions,
      this.emit.bind(this)
    );
  }

  public addParticipant(participant: Omit<CollaborationParticipant, 'lastActivity'>): void {
    this.collaborationState = addParticipantFn(
      this.collaborationState,
      participant,
      this.emit.bind(this)
    );
  }

  public updateParticipantCursor(participantId: string, position: Vector3): void {
    this.collaborationState = updateCursorFn(
      this.collaborationState,
      participantId,
      position,
      this.emit.bind(this)
    );
  }

  public updateParticipantSelection(participantId: string, selection: Set<string>): void {
    this.collaborationState = updateSelectionFn(
      this.collaborationState,
      participantId,
      selection,
      this.emit.bind(this)
    );
  }

  public sendChatMessage(
    participantId: string,
    message: string,
    type: ChatMessage['type'] = 'text'
  ): void {
    this.collaborationState = sendMessageFn(
      this.collaborationState,
      participantId,
      message,
      type,
      this.emit.bind(this)
    );
  }

  public createAnnotation(
    creatorId: string,
    position: Vector3,
    content: string,
    type: GraphAnnotation['type'],
    attachedNodes: string[] = []
  ): string {
    const { state, annotationId } = createAnnotationFn(
      this.collaborationState,
      creatorId,
      position,
      content,
      type,
      attachedNodes,
      this.emit.bind(this)
    );
    this.collaborationState = state;
    return annotationId;
  }

  // ── VR / AR ───────────────────────────────────────────────────────────────

  public activateVRMode(options: Partial<VRARState> = {}): void {
    const result = activateVR(this.vrArState, options, this.emit.bind(this));
    this.vrArState = result.vrArState;
    this.spatialUI = result.spatialUI;
  }

  public activateARMode(options: Partial<VRARState> = {}): void {
    const result = activateAR(this.vrArState, options, this.emit.bind(this));
    this.vrArState = result.vrArState;
    this.spatialUI = result.spatialUI;
  }

  public processImmersiveInteraction(interaction: ImmersiveInteraction): void {
    this.spatialUI = processInteraction(
      interaction,
      this.spatialUI,
      this.emit.bind(this),
      () => this.nextWaypoint(),
      () => this.previousWaypoint(),
      (id: string) => this.startExplorationTour(id)
    );
  }

  // ── Event emitter ─────────────────────────────────────────────────────────

  private emit(event: string, data?: any): void {
    const listeners = this.eventListeners.get(event);
    if (listeners) {
      listeners.forEach(listener => {
        try {
          listener(data);
        } catch (error) {
          logger.error(`Error in event listener for ${event}:`, error);
        }
      });
    }
  }

  public on(event: string, listener: Function): () => void {
    if (!this.eventListeners.has(event)) {
      this.eventListeners.set(event, new Set());
    }
    this.eventListeners.get(event)!.add(listener);
    return () => {
      this.eventListeners.get(event)?.delete(listener);
    };
  }

  // ── State accessors ───────────────────────────────────────────────────────

  public getTimeTravelState(): TimeTravelState {
    return { ...this.timeTravelState };
  }

  public getExplorationState(): ExplorationState {
    return { ...this.explorationState };
  }

  public getCollaborationState(): CollaborationState {
    return { ...this.collaborationState };
  }

  public getVRARState(): VRARState {
    return { ...this.vrArState };
  }

  public getSpatialUI(): SpatialUI {
    return { ...this.spatialUI };
  }

  // ── Lifecycle ─────────────────────────────────────────────────────────────

  public dispose(): void {
    this.timeTravelState = { ...this.timeTravelState, isPlaying: false };
    this.explorationState = { ...this.explorationState, isActive: false };
    this.collaborationState = { ...this.collaborationState, isActive: false };
    this.vrArState = { ...this.vrArState, isActive: false };
    this.tours.clear();
    this.activeAnimations.clear();
    this.eventListeners.clear();
    logger.info('Advanced interaction modes disposed');
  }
}
