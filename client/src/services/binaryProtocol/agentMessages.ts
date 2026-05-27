
import { createLogger } from '../../utils/loggerConfig';
import {
  MessageType,
  AGENT_POSITION_SIZE_V2,
  AGENT_STATE_SIZE_V2,
  AGENT_ACTION_HEADER_SIZE,
} from './frameTypes';
import type {
  AgentPositionUpdate,
  AgentStateData,
  AgentActionEvent,
  AgentActionType,
} from './frameTypes';

const logger = createLogger('BinaryWebSocketProtocol');

// encodePositionPayload writes agent position updates into a pre-sized payload ArrayBuffer.
// Caller is responsible for allocating `allUpdates.length * AGENT_POSITION_SIZE_V2` bytes.
export function encodePositionPayload(
  allUpdates: AgentPositionUpdate[],
  payload: ArrayBuffer
): void {
  const view = new DataView(payload);
  allUpdates.forEach((update, index) => {
    const offset = index * AGENT_POSITION_SIZE_V2;
    view.setUint32(offset, update.agentId, true);
    view.setFloat32(offset + 4, update.position.x, true);
    view.setFloat32(offset + 8, update.position.y, true);
    view.setFloat32(offset + 12, update.position.z, true);
    view.setUint32(offset + 16, update.timestamp, true);
    view.setUint8(offset + 20, update.flags);
  });
}

export function decodePositionUpdates(payload: ArrayBuffer): AgentPositionUpdate[] {
  const updates: AgentPositionUpdate[] = [];

  if (payload.byteLength < AGENT_POSITION_SIZE_V2) {
    if (payload.byteLength === 0) return updates;
    logger.error(`Position update payload too small: ${payload.byteLength}`);
    return updates;
  }

  // Version is in the message header (validated by parseHeader/validateMessage),
  // not embedded in the payload. Decode payload directly as V2+ data.
  const view = new DataView(payload);

  if ((payload.byteLength % AGENT_POSITION_SIZE_V2) !== 0) {
    logger.error(`Invalid position update payload size: ${payload.byteLength}`);
    return updates;
  }

  const updateCount = payload.byteLength / AGENT_POSITION_SIZE_V2;
  for (let i = 0; i < updateCount; i++) {
    const offset = i * AGENT_POSITION_SIZE_V2;

    if (offset + AGENT_POSITION_SIZE_V2 > payload.byteLength) {
      logger.warn('Truncated position update data');
      break;
    }

    updates.push({
      agentId: view.getUint32(offset, true),
      position: {
        x: view.getFloat32(offset + 4, true),
        y: view.getFloat32(offset + 8, true),
        z: view.getFloat32(offset + 12, true)
      },
      timestamp: view.getUint32(offset + 16, true),
      flags: view.getUint8(offset + 20)
    });
  }

  return updates;
}

export function encodeAgentStatePayload(agents: AgentStateData[]): ArrayBuffer {
  const payload = new ArrayBuffer(agents.length * AGENT_STATE_SIZE_V2);
  const view = new DataView(payload);

  agents.forEach((agent, index) => {
    const offset = index * AGENT_STATE_SIZE_V2;
    view.setUint32(offset, agent.agentId, true);
    view.setFloat32(offset + 4, agent.position.x, true);
    view.setFloat32(offset + 8, agent.position.y, true);
    view.setFloat32(offset + 12, agent.position.z, true);
    view.setFloat32(offset + 16, agent.velocity.x, true);
    view.setFloat32(offset + 20, agent.velocity.y, true);
    view.setFloat32(offset + 24, agent.velocity.z, true);
    view.setFloat32(offset + 28, agent.health, true);
    view.setFloat32(offset + 32, agent.cpuUsage, true);
    view.setFloat32(offset + 36, agent.memoryUsage, true);
    view.setFloat32(offset + 40, agent.workload, true);
    view.setUint32(offset + 44, agent.tokens, true);
    view.setUint8(offset + 48, agent.flags);
  });

  return payload;
}

export function decodeAgentState(payload: ArrayBuffer): AgentStateData[] {
  const agents: AgentStateData[] = [];

  if (payload.byteLength < AGENT_STATE_SIZE_V2) {
    if (payload.byteLength === 0) return agents;
    logger.error(`Agent state payload too small: ${payload.byteLength}`);
    return agents;
  }

  // Version is in the message header (validated by parseHeader/validateMessage),
  // not embedded in the payload. Decode payload directly as V2+ data.
  const view = new DataView(payload);

  if ((payload.byteLength % AGENT_STATE_SIZE_V2) !== 0) {
    logger.error(`Invalid agent state payload size: ${payload.byteLength}`);
    return agents;
  }

  const agentCount = payload.byteLength / AGENT_STATE_SIZE_V2;
  for (let i = 0; i < agentCount; i++) {
    const offset = i * AGENT_STATE_SIZE_V2;

    if (offset + AGENT_STATE_SIZE_V2 > payload.byteLength) {
      logger.warn('Truncated agent state data');
      break;
    }

    agents.push({
      agentId: view.getUint32(offset, true),
      position: {
        x: view.getFloat32(offset + 4, true),
        y: view.getFloat32(offset + 8, true),
        z: view.getFloat32(offset + 12, true)
      },
      velocity: {
        x: view.getFloat32(offset + 16, true),
        y: view.getFloat32(offset + 20, true),
        z: view.getFloat32(offset + 24, true)
      },
      health: view.getFloat32(offset + 28, true),
      cpuUsage: view.getFloat32(offset + 32, true),
      memoryUsage: view.getFloat32(offset + 36, true),
      workload: view.getFloat32(offset + 40, true),
      tokens: view.getUint32(offset + 44, true),
      flags: view.getUint8(offset + 48)
    });
  }

  return agents;
}

export function decodeAgentAction(payload: ArrayBuffer): AgentActionEvent | null {
  if (payload.byteLength < AGENT_ACTION_HEADER_SIZE) {
    logger.error('Agent action payload too small');
    return null;
  }

  const view = new DataView(payload);

  const sourceAgentId = view.getUint32(0, true);
  const targetNodeId = view.getUint32(4, true);
  const actionType = view.getUint8(8) as AgentActionType;
  const timestamp = view.getUint32(9, true);
  const durationMs = view.getUint16(13, true);

  const event: AgentActionEvent = {
    sourceAgentId,
    targetNodeId,
    actionType,
    timestamp,
    durationMs,
  };

  // Extract optional payload
  if (payload.byteLength > AGENT_ACTION_HEADER_SIZE) {
    event.payload = new Uint8Array(payload.slice(AGENT_ACTION_HEADER_SIZE));
  }

  return event;
}

export function decodeAgentActions(payload: ArrayBuffer): AgentActionEvent[] {
  const events: AgentActionEvent[] = [];

  if (payload.byteLength < 2) {
    logger.error('Agent actions batch payload too small');
    return events;
  }

  const view = new DataView(payload);
  const eventCount = view.getUint16(0, true);
  let offset = 2;

  for (let i = 0; i < eventCount; i++) {
    if (offset + 2 > payload.byteLength) {
      logger.warn('Truncated event length in agent actions batch');
      break;
    }

    const eventLen = view.getUint16(offset, true);
    offset += 2;

    if (offset + eventLen > payload.byteLength) {
      logger.warn('Truncated event data in agent actions batch');
      break;
    }

    const eventPayload = payload.slice(offset, offset + eventLen);
    const event = decodeAgentAction(eventPayload);
    if (event) {
      events.push(event);
    }
    offset += eventLen;
  }

  return events;
}

export function encodeAgentActionPayload(event: AgentActionEvent): ArrayBuffer {
  const payloadLen = event.payload?.length ?? 0;
  const payload = new ArrayBuffer(AGENT_ACTION_HEADER_SIZE + payloadLen);
  const view = new DataView(payload);

  view.setUint32(0, event.sourceAgentId, true);
  view.setUint32(4, event.targetNodeId, true);
  view.setUint8(8, event.actionType);
  view.setUint32(9, event.timestamp, true);
  view.setUint16(13, event.durationMs, true);

  // Copy optional payload
  if (event.payload && event.payload.length > 0) {
    new Uint8Array(payload, AGENT_ACTION_HEADER_SIZE).set(event.payload);
  }

  return payload;
}

export { MessageType };
