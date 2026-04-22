import { createLogger } from '../utils/loggerConfig';

const logger = createLogger('binaryProtocol');


/**
 * Binary Protocol for WebSocket node data communication
 * Aligns with server-side src/utils/binary_protocol.rs
 *
 * ============================================================================
 * ARCHITECTURE LOCK — DO NOT RE-INTRODUCE DELTA PROTOCOLS (V4)
 * ============================================================================
 * The wire protocol is LITERAL-ONLY. Every broadcast is a full snapshot.
 *
 * V4 delta encoding WAS tried. It was removed because:
 *   • Force-directed spring networks move every node every tick — our
 *     "deltas" always contained every node, saving nothing.
 *   • Stale-position drift on reconnect / packet loss.
 *   • Silent drop of user pin signals when the threshold filtered them out.
 *   • Parallel decoders (V3 full + V4 delta) doubled bug surface area.
 *
 * The V4 parser below remains ONLY to detect unintended server regressions;
 * receiving a V4 frame throws loudly. DO NOT call it from new code.
 *
 * The real bandwidth lever is BROADCAST CADENCE: server broadcasts on
 * settlement change, pin change, topology change, or heartbeat — not every
 * physics tick. See ForceComputeActor::broadcast path.
 *
 * Relitigated 2026-04-21. Any PR that re-enables V4, adds a delta_threshold
 * prop to wire parsing, or introduces a new "delta-compressed" protocol
 * variant is REJECTED on sight. See ADR-037.
 * ============================================================================
 *
 * Protocol Versions:
 * - V3: 48 bytes per node (server default — literal absolute positions)
 * - V4: REMOVED — see lock above; parser remains as regression detector only
 * - V5: V3 node data with 9-byte envelope (version + broadcast sequence)
 */

export interface Vec3 {
  x: number;
  y: number;
  z: number;
}

export interface BinaryNodeData {
  nodeId: number;
  position: Vec3;
  velocity: Vec3;
  ssspDistance: number;
  ssspParent: number;
  // V3 analytics fields (optional for backwards compatibility)
  clusterId?: number;
  anomalyScore?: number;
  communityId?: number;
}

/**
 * Result of parsing binary node data, distinguishing full vs delta updates.
 * - Full updates: nodes contain absolute positions (replace targetPositions)
 * - Delta updates: nodes contain position/velocity DELTAS (add to targetPositions)
 */
export interface ParsedBinaryFrame {
  /** 'full' for V3/V5 full state, 'delta' for V4 delta encoding */
  type: 'full' | 'delta';
  /** Parsed node data. For delta frames, position/velocity are DELTAS, not absolute. */
  nodes: BinaryNodeData[];
  /** V4 only: frame number within the delta cycle (0-59) */
  frameNumber?: number;
  /** V5 only: authoritative server broadcast sequence for backpressure ack correlation */
  broadcastSequence?: number;
}

// V4 delta encoding constants (must match server delta_encoding.rs)
const DELTA_SCALE_FACTOR = 100.0;
const DELTA_ITEM_SIZE = 20;
const DELTA_POSITION_CHANGED = 0x01;
const DELTA_VELOCITY_CHANGED = 0x02;

// Protocol version constants (must match server)
export const PROTOCOL_V3 = 3;
export const PROTOCOL_V4 = 4;
export const PROTOCOL_V5 = 5;

/** Last broadcast sequence received from server (V5 frames). Undefined until first V5 frame. */
export let lastBroadcastSequence: number | undefined;

// V2 wire format: 36 bytes per node
export const BINARY_NODE_SIZE_V2 = 36;
// V3 wire format: 48 bytes per node (V2 + 12 bytes analytics)
export const BINARY_NODE_SIZE_V3 = 48;
// Default to V3 (current server default)
export const BINARY_NODE_SIZE = BINARY_NODE_SIZE_V3;

// Field offsets (same for V2 and V3)
export const BINARY_NODE_ID_OFFSET = 0;
export const BINARY_POSITION_OFFSET = 4;
export const BINARY_VELOCITY_OFFSET = 16;
export const BINARY_SSSP_DISTANCE_OFFSET = 28;
export const BINARY_SSSP_PARENT_OFFSET = 32;
// V3 analytics offsets
export const BINARY_CLUSTER_ID_OFFSET = 36;
export const BINARY_ANOMALY_SCORE_OFFSET = 40;
export const BINARY_COMMUNITY_ID_OFFSET = 44;

// Node type flag constants (Protocol V2/V3 - must match server)
export const AGENT_NODE_FLAG = 0x80000000;
export const KNOWLEDGE_NODE_FLAG = 0x40000000;
// NODE_ID_MASK: bits 0-25 only (excludes ALL flag bits 26-31: agent, knowledge, ontology type)
// Must match server's NODE_ID_MASK (0x03FFFFFF) to correctly strip ontology type flags
export const NODE_ID_MASK = 0x03FFFFFF;

// Ontology node type flags (bits 26-28)
export const ONTOLOGY_TYPE_MASK = 0x1C000000;
export const ONTOLOGY_CLASS_FLAG = 0x04000000;
export const ONTOLOGY_INDIVIDUAL_FLAG = 0x08000000;
export const ONTOLOGY_PROPERTY_FLAG = 0x10000000;

export enum NodeType {
  Knowledge = 'knowledge',
  Agent = 'agent',
  OntologyClass = 'ontology_class',
  OntologyIndividual = 'ontology_individual',
  OntologyProperty = 'ontology_property',
  Unknown = 'unknown'
}

export function getNodeType(nodeId: number): NodeType {
  if ((nodeId & AGENT_NODE_FLAG) !== 0) {
    return NodeType.Agent;
  } else if ((nodeId & KNOWLEDGE_NODE_FLAG) !== 0) {
    return NodeType.Knowledge;
  } else if ((nodeId & ONTOLOGY_TYPE_MASK) === ONTOLOGY_CLASS_FLAG) {
    return NodeType.OntologyClass;
  } else if ((nodeId & ONTOLOGY_TYPE_MASK) === ONTOLOGY_INDIVIDUAL_FLAG) {
    return NodeType.OntologyIndividual;
  } else if ((nodeId & ONTOLOGY_TYPE_MASK) === ONTOLOGY_PROPERTY_FLAG) {
    return NodeType.OntologyProperty;
  }
  return NodeType.Unknown;
}

export function getActualNodeId(nodeId: number): number {
  return nodeId & NODE_ID_MASK;
}

export function isAgentNode(nodeId: number): boolean {
  return (nodeId & AGENT_NODE_FLAG) !== 0;
}

export function isKnowledgeNode(nodeId: number): boolean {
  return (nodeId & KNOWLEDGE_NODE_FLAG) !== 0;
}

export function isOntologyNode(nodeId: number): boolean {
  return (nodeId & ONTOLOGY_TYPE_MASK) !== 0;
}

/**
 * Parse binary node data from server
 * Supports Protocol V2 (36 bytes) and V3 (48 bytes)
 */
export function parseBinaryNodeData(buffer: ArrayBuffer): BinaryNodeData[] {
  if (!buffer || buffer.byteLength === 0) {
    return [];
  }

  // Create a copy to avoid issues with detached buffers
  const safeBuffer = buffer.slice(0);
  const view = new DataView(safeBuffer);
  const nodes: BinaryNodeData[] = [];

  try {
    if (safeBuffer.byteLength < 1) {
      return [];
    }

    // Read protocol version byte
    const protocolVersion = view.getUint8(0);
    let offset = 1; // Skip version byte
    let nodeSize: number;
    let hasAnalytics: boolean;

    switch (protocolVersion) {
      case PROTOCOL_V3:
        nodeSize = BINARY_NODE_SIZE_V3;
        hasAnalytics = true;
        break;
      case PROTOCOL_V4:
        // ARCHITECTURE LOCK (see top of file): V4 delta encoding is REMOVED.
        // Receiving a V4 frame means the server regressed. Fail loudly so the
        // regression is caught immediately rather than silently corrupting
        // positions via delta accumulation.
        logger.error('[binaryProtocol] SERVER REGRESSION: received PROTOCOL_V4 (delta) frame. Wire protocol is literal-only — see ADR-037. Dropping frame.');
        return [];
      case PROTOCOL_V5:
        // V5 = V3 node data with 8-byte broadcast sequence prefix
        // Extract sequence, then parse remainder as V3
        return parseV5Nodes(safeBuffer);
      default:
        // Unknown version - try to detect format by size
        logger.warn(`Unknown protocol version: ${protocolVersion}, attempting auto-detection`);
        offset = 0; // No version byte - legacy format?
        nodeSize = BINARY_NODE_SIZE_V2;
        hasAnalytics = false;
    }

    const dataLength = safeBuffer.byteLength - offset;

    // Validate data length
    if (dataLength % nodeSize !== 0) {
      // Check if it might be the other version
      const otherSize = hasAnalytics ? BINARY_NODE_SIZE_V2 : BINARY_NODE_SIZE_V3;
      if (dataLength % otherSize === 0) {
        logger.warn(`Data size suggests ${hasAnalytics ? 'V2' : 'V3'} format, switching...`);
        nodeSize = otherSize;
        hasAnalytics = !hasAnalytics;
      } else {
        logger.warn(
          `Binary data length (${dataLength} bytes) is not a multiple of node size (${nodeSize}). ` +
          `Expected ${Math.floor(dataLength / nodeSize)} complete nodes.`
        );
      }
    }

    const completeNodes = Math.floor(dataLength / nodeSize);

    if (completeNodes === 0) {
      return [];
    }

    // Parse each node
    for (let i = 0; i < completeNodes; i++) {
      const nodeOffset = offset + (i * nodeSize);

      if (nodeOffset + nodeSize > safeBuffer.byteLength) {
        break;
      }

      // Node ID (4 bytes) - includes type flags in high bits
      const nodeId = view.getUint32(nodeOffset + BINARY_NODE_ID_OFFSET, true);

      // Position (12 bytes)
      const position: Vec3 = {
        x: view.getFloat32(nodeOffset + BINARY_POSITION_OFFSET, true),
        y: view.getFloat32(nodeOffset + BINARY_POSITION_OFFSET + 4, true),
        z: view.getFloat32(nodeOffset + BINARY_POSITION_OFFSET + 8, true)
      };

      // Velocity (12 bytes)
      const velocity: Vec3 = {
        x: view.getFloat32(nodeOffset + BINARY_VELOCITY_OFFSET, true),
        y: view.getFloat32(nodeOffset + BINARY_VELOCITY_OFFSET + 4, true),
        z: view.getFloat32(nodeOffset + BINARY_VELOCITY_OFFSET + 8, true)
      };

      // SSSP data (8 bytes)
      const ssspDistance = view.getFloat32(nodeOffset + BINARY_SSSP_DISTANCE_OFFSET, true);
      const ssspParent = view.getInt32(nodeOffset + BINARY_SSSP_PARENT_OFFSET, true);

      // Validate position and velocity (reject NaN/Inf)
      const isValid =
        !isNaN(position.x) && isFinite(position.x) &&
        !isNaN(position.y) && isFinite(position.y) &&
        !isNaN(position.z) && isFinite(position.z) &&
        !isNaN(velocity.x) && isFinite(velocity.x) &&
        !isNaN(velocity.y) && isFinite(velocity.y) &&
        !isNaN(velocity.z) && isFinite(velocity.z);

      if (isValid) {
        const node: BinaryNodeData = {
          nodeId,
          position,
          velocity,
          ssspDistance,
          ssspParent
        };

        // Parse V3 analytics fields if present
        if (hasAnalytics) {
          node.clusterId = view.getUint32(nodeOffset + BINARY_CLUSTER_ID_OFFSET, true);
          node.anomalyScore = view.getFloat32(nodeOffset + BINARY_ANOMALY_SCORE_OFFSET, true);
          node.communityId = view.getUint32(nodeOffset + BINARY_COMMUNITY_ID_OFFSET, true);
        }

        nodes.push(node);
      } else {
        // Only log first few corrupted nodes to avoid spam
        if (i < 3) {
          logger.warn(
            `Skipping corrupted node at index ${i}: id=${nodeId}, ` +
            `pos=[${position.x}, ${position.y}, ${position.z}]`
          );
        }
      }
    }
  } catch (error) {
    logger.error('Error parsing binary data:', error);
  }

  return nodes;
}

/**
 * Parse V4 delta-encoded frame into BinaryNodeData entries.
 * The returned nodes contain DELTA values (not absolute positions).
 * Position and velocity fields represent the CHANGE from the previous frame.
 *
 * V4 wire format:
 *   [1 byte: version=4][1 byte: frame_number][2 bytes: num_changed (u16 LE)]
 *   For each changed node (20 bytes):
 *     [4 bytes: node_id (u32 LE)][1 byte: change_flags][3 bytes: padding]
 *     [2 bytes: dx (i16 LE)][2 bytes: dy][2 bytes: dz]
 *     [2 bytes: dvx (i16 LE)][2 bytes: dvy][2 bytes: dvz]
 */
function parseDeltaNodes(buffer: ArrayBuffer): BinaryNodeData[] {
  const view = new DataView(buffer);
  const nodes: BinaryNodeData[] = [];

  // Minimum size: 1 (version) + 1 (frame) + 2 (count) = 4 bytes
  if (buffer.byteLength < 4) {
    return [];
  }

  // Skip version byte (already validated as V4)
  const _frameNumber = view.getUint8(1);
  const numChanged = view.getUint16(2, true);

  const expectedSize = 4 + numChanged * DELTA_ITEM_SIZE;
  if (buffer.byteLength < expectedSize) {
    logger.warn(`V4 delta frame truncated: expected ${expectedSize} bytes, got ${buffer.byteLength}`);
    return [];
  }

  let offset = 4;
  for (let i = 0; i < numChanged; i++) {
    const nodeId = view.getUint32(offset, true);
    const changeFlags = view.getUint8(offset + 4);
    // offset + 5..7 = padding

    const dxScaled = view.getInt16(offset + 8, true);
    const dyScaled = view.getInt16(offset + 10, true);
    const dzScaled = view.getInt16(offset + 12, true);
    const dvxScaled = view.getInt16(offset + 14, true);
    const dvyScaled = view.getInt16(offset + 16, true);
    const dvzScaled = view.getInt16(offset + 18, true);

    const dx = (changeFlags & DELTA_POSITION_CHANGED) ? dxScaled / DELTA_SCALE_FACTOR : 0;
    const dy = (changeFlags & DELTA_POSITION_CHANGED) ? dyScaled / DELTA_SCALE_FACTOR : 0;
    const dz = (changeFlags & DELTA_POSITION_CHANGED) ? dzScaled / DELTA_SCALE_FACTOR : 0;
    const dvx = (changeFlags & DELTA_VELOCITY_CHANGED) ? dvxScaled / DELTA_SCALE_FACTOR : 0;
    const dvy = (changeFlags & DELTA_VELOCITY_CHANGED) ? dvyScaled / DELTA_SCALE_FACTOR : 0;
    const dvz = (changeFlags & DELTA_VELOCITY_CHANGED) ? dvzScaled / DELTA_SCALE_FACTOR : 0;

    // FIX 3 (client): V4 delta overflow sanity check.
    // If applying this delta would produce an out-of-range position (> 100000),
    // the delta was likely clamped due to i16 overflow on the server.
    // Log a warning so developers can diagnose the issue.
    const POSITION_SANITY_BOUND = 100000;
    if (Math.abs(dx) > POSITION_SANITY_BOUND || Math.abs(dy) > POSITION_SANITY_BOUND || Math.abs(dz) > POSITION_SANITY_BOUND) {
      logger.warn(
        `V4 delta overflow detected for node ${nodeId}: delta=[${dx.toFixed(2)}, ${dy.toFixed(2)}, ${dz.toFixed(2)}]. ` +
        `Likely i16 clamping corruption. Wait for next full V3 frame.`
      );
    }

    nodes.push({
      nodeId,
      position: { x: dx, y: dy, z: dz },
      velocity: { x: dvx, y: dvy, z: dvz },
      ssspDistance: Infinity,
      ssspParent: -1,
    });

    offset += DELTA_ITEM_SIZE;
  }

  return nodes;
}

/**
 * Parse V5 frame: [1 byte: version=5][8 bytes: broadcast_sequence LE][V3 node data without version byte]
 * Extracts broadcast sequence, reconstructs a V3 buffer, and delegates to V3 parsing.
 * Updates the module-level `lastBroadcastSequence` export.
 */
function parseV5Nodes(buffer: ArrayBuffer): BinaryNodeData[] {
  // Minimum: 1 (version) + 8 (sequence) = 9 bytes
  if (buffer.byteLength < 9) {
    return [];
  }
  const view = new DataView(buffer);
  // Read 8-byte LE u64 as Number (safe up to 2^53)
  const seqLow = view.getUint32(1, true);
  const seqHigh = view.getUint32(5, true);
  lastBroadcastSequence = seqLow + seqHigh * 0x100000000;

  // Reconstruct a V3 buffer: [version=3][node data from offset 9 onward]
  const nodeDataLen = buffer.byteLength - 9;
  if (nodeDataLen <= 0) {
    return [];
  }
  const v3Buffer = new ArrayBuffer(1 + nodeDataLen);
  const v3View = new Uint8Array(v3Buffer);
  v3View[0] = PROTOCOL_V3;
  v3View.set(new Uint8Array(buffer, 9), 1);
  return parseBinaryNodeData(v3Buffer);
}

/**
 * Parse binary data and return a typed frame that distinguishes full vs delta updates.
 * This is the preferred entry point for callers that need to handle delta encoding.
 *
 * - Full frames (V2/V3/V5): nodes contain absolute positions
 * - Delta frames (V4): nodes contain position/velocity DELTAS
 */
export function parseBinaryFrameData(buffer: ArrayBuffer): ParsedBinaryFrame {
  if (!buffer || buffer.byteLength === 0) {
    return { type: 'full', nodes: [] };
  }

  const safeBuffer = buffer.slice(0);
  const view = new DataView(safeBuffer);
  const protocolVersion = view.getUint8(0);

  if (protocolVersion === PROTOCOL_V4) {
    // ARCHITECTURE LOCK (see top of file): V4 delta is REMOVED. Fail loudly.
    logger.error('[binaryProtocol] SERVER REGRESSION: PROTOCOL_V4 delta frame received. Dropping — ADR-037.');
    return { type: 'full', nodes: [] };
  }

  if (protocolVersion === PROTOCOL_V5) {
    // V5: extract broadcast sequence, parse node data as V3
    const v5Nodes = parseV5Nodes(safeBuffer);
    return {
      type: 'full',
      nodes: v5Nodes,
      broadcastSequence: lastBroadcastSequence,
    };
  }

  // V3 (or unknown): delegate to existing full-state parser
  const fullNodes = parseBinaryNodeData(buffer);
  return { type: 'full', nodes: fullNodes };
}

/**
 * Create binary node data for sending to server
 * Uses Protocol V3 format (48 bytes per node)
 */
export function createBinaryNodeData(nodes: BinaryNodeData[]): ArrayBuffer {
  // 1 byte version header + nodes * 48 bytes
  const buffer = new ArrayBuffer(1 + nodes.length * BINARY_NODE_SIZE_V3);
  const view = new DataView(buffer);

  // Write version header
  view.setUint8(0, PROTOCOL_V3);

  nodes.forEach((node, i) => {
    const offset = 1 + (i * BINARY_NODE_SIZE_V3);

    // Node ID
    view.setUint32(offset + BINARY_NODE_ID_OFFSET, node.nodeId, true);

    // Position
    view.setFloat32(offset + BINARY_POSITION_OFFSET, node.position.x, true);
    view.setFloat32(offset + BINARY_POSITION_OFFSET + 4, node.position.y, true);
    view.setFloat32(offset + BINARY_POSITION_OFFSET + 8, node.position.z, true);

    // Velocity
    view.setFloat32(offset + BINARY_VELOCITY_OFFSET, node.velocity.x, true);
    view.setFloat32(offset + BINARY_VELOCITY_OFFSET + 4, node.velocity.y, true);
    view.setFloat32(offset + BINARY_VELOCITY_OFFSET + 8, node.velocity.z, true);

    // SSSP data
    view.setFloat32(offset + BINARY_SSSP_DISTANCE_OFFSET, node.ssspDistance ?? Infinity, true);
    view.setInt32(offset + BINARY_SSSP_PARENT_OFFSET, node.ssspParent ?? -1, true);

    // V3 analytics data
    view.setUint32(offset + BINARY_CLUSTER_ID_OFFSET, node.clusterId ?? 0, true);
    view.setFloat32(offset + BINARY_ANOMALY_SCORE_OFFSET, node.anomalyScore ?? 0, true);
    view.setUint32(offset + BINARY_COMMUNITY_ID_OFFSET, node.communityId ?? 0, true);
  });

  return buffer;
}

/**
 * Message type constants (must match server)
 */
export enum MessageType {
  BinaryPositions = 0x00,
  VoiceData = 0x02,
  ControlFrame = 0x03,
  PositionDelta = 0x04,
  BroadcastAck = 0x34,
}
