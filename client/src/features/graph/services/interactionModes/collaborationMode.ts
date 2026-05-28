import { Vector3 } from 'three';
import { createLogger } from '../../../../utils/loggerConfig';
import type {
  CollaborationState,
  CollaborationParticipant,
  CollaborationPermissions,
  ChatMessage,
  GraphAnnotation
} from './types';

const logger = createLogger('CollaborationMode');

export function createCollaborationState(): CollaborationState {
  return {
    isActive: false,
    sessionId: '',
    participants: [],
    sharedCursor: new Map(),
    sharedSelections: new Map(),
    chatMessages: [],
    annotations: [],
    permissions: {
      canEdit: false,
      canAnnotate: true,
      canHighlight: true,
      canUseVoiceChat: false,
      canModifyLayout: false,
      canCreateTours: false
    }
  };
}

export function startCollaborationSession(
  state: CollaborationState,
  sessionId: string,
  permissions: Partial<CollaborationPermissions> = {},
  emit: (event: string, data?: any) => void
): CollaborationState {
  logger.info(`Starting collaboration session: ${sessionId}`);
  const next: CollaborationState = {
    isActive: true,
    sessionId,
    participants: [],
    sharedCursor: new Map(),
    sharedSelections: new Map(),
    chatMessages: [],
    annotations: [],
    permissions: { ...state.permissions, ...permissions }
  };
  emit('collaborationStarted', next);
  return next;
}

export function addParticipant(
  state: CollaborationState,
  participant: Omit<CollaborationParticipant, 'lastActivity'>,
  emit: (event: string, data?: any) => void
): CollaborationState {
  const fullParticipant: CollaborationParticipant = { ...participant, lastActivity: Date.now() };
  emit('participantJoined', fullParticipant);
  return { ...state, participants: [...state.participants, fullParticipant] };
}

export function updateParticipantCursor(
  state: CollaborationState,
  participantId: string,
  position: Vector3,
  emit: (event: string, data?: any) => void
): CollaborationState {
  const sharedCursor = new Map(state.sharedCursor);
  sharedCursor.set(participantId, position);

  const participants = state.participants.map(p =>
    p.id === participantId ? { ...p, cursorPosition: position, lastActivity: Date.now() } : p
  );

  emit('cursorUpdated', { participantId, position });
  return { ...state, sharedCursor, participants };
}

export function updateParticipantSelection(
  state: CollaborationState,
  participantId: string,
  selection: Set<string>,
  emit: (event: string, data?: any) => void
): CollaborationState {
  const sharedSelections = new Map(state.sharedSelections);
  sharedSelections.set(participantId, selection);

  const participants = state.participants.map(p =>
    p.id === participantId ? { ...p, currentSelection: selection, lastActivity: Date.now() } : p
  );

  emit('selectionUpdated', { participantId, selection });
  return { ...state, sharedSelections, participants };
}

export function sendChatMessage(
  state: CollaborationState,
  participantId: string,
  message: string,
  type: ChatMessage['type'],
  emit: (event: string, data?: any) => void
): CollaborationState {
  const chatMessage: ChatMessage = {
    id: `msg-${Date.now()}-${Math.random()}`,
    participantId,
    message,
    timestamp: Date.now(),
    type
  };
  emit('chatMessage', chatMessage);
  return { ...state, chatMessages: [...state.chatMessages, chatMessage] };
}

export function createAnnotation(
  state: CollaborationState,
  creatorId: string,
  position: Vector3,
  content: string,
  type: GraphAnnotation['type'],
  attachedNodes: string[],
  emit: (event: string, data?: any) => void
): { state: CollaborationState; annotationId: string } {
  const annotation: GraphAnnotation = {
    id: `annotation-${Date.now()}-${Math.random()}`,
    creatorId,
    position,
    content,
    type,
    attachedNodes,
    visibility: 'shared',
    reactions: new Map()
  };
  emit('annotationCreated', annotation);
  return {
    state: { ...state, annotations: [...state.annotations, annotation] },
    annotationId: annotation.id
  };
}
