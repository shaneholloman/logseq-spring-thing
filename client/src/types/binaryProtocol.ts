/**
 * Binary Protocol — single per-physics-tick wire format (ADR-061 / PRD-007).
 *
 * Wire layout (28 bytes/node, fixed, forever):
 *   [u8  preamble = 0x42]
 *   [u64 broadcast_sequence_LE]
 *   [N × Node]
 *
 * Per-Node layout (28 bytes = u32 + 6 × f32):
 *   [u32 id_LE]
 *   [f32 x_LE][f32 y_LE][f32 z_LE]      (position)
 *   [f32 vx_LE][f32 vy_LE][f32 vz_LE]   (velocity)
 *
 * There are no "versions" of this protocol. The preamble byte is a fixed
 * sanity check — if it ever needs to evolve, it does so via a new endpoint.
 *
 * Sticky GPU outputs (cluster_id, community_id, anomaly_score, sssp_*) move
 * to the `analytics_update` text-message channel; node type / visibility
 * ride the JSON `/api/graph/data` init payload.
 */

import { createLogger } from '../utils/loggerConfig';

const logger = createLogger('binaryProtocol');

// ── Wire constants ─────────────────────────────────────────────────────

/** Sanity preamble byte — not a version dispatch. */
export const BINARY_PROTOCOL_PREAMBLE = 0x42;

/** Bytes per node entry on the wire: id(4) + pos(12) + vel(12) = 28. */
export const BINARY_NODE_SIZE = 28;

/** Frame envelope size (preamble + broadcast_sequence). */
export const BINARY_FRAME_HEADER_SIZE = 9;

// ── Types ──────────────────────────────────────────────────────────────

export interface Vec3 {
  x: number;
  y: number;
  z: number;
}

/** A single node entry in a position frame. */
export interface NodeEntry {
  nodeId: number;
  position: Vec3;
  velocity: Vec3;
}

/** Decoded position frame. */
export interface PositionFrame {
  /** Server-authored monotonic sequence number, used for backpressure ack. */
  broadcastSequence: number;
  /** Map keyed by raw u32 node id — no flag bits, no masking. */
  nodes: Map<number, NodeEntry>;
}

/**
 * Outbound batch node payload.
 *
 * Used by the position-batch-queue scaffolding to ship user-driven node
 * drag positions back to the server. Same 28-byte wire layout as inbound
 * frames; analytics columns are gone.
 */
export interface BinaryNodeData {
  nodeId: number;
  position: Vec3;
  velocity: Vec3;
}

/**
 * NodeType enum — kept for typing of the JSON-init side-table.
 *
 * Pre-ADR-061 this was decoded from id flag bits; the wire no longer
 * carries flag bits. Node type now arrives via `/api/graph/data` JSON
 * (`node.node_type`) and is consumed by `useGraphVisualState`.
 */
export enum NodeType {
  Knowledge = 'knowledge',
  Agent = 'agent',
  OntologyClass = 'ontology_class',
  OntologyIndividual = 'ontology_individual',
  OntologyProperty = 'ontology_property',
  Unknown = 'unknown',
}

// ── Encoder (outbound: client → server) ────────────────────────────────

/**
 * Encode an outbound position-update buffer.
 *
 * Wire matches the server's inbound decoder: preamble byte + 8-byte
 * broadcast sequence (set to 0 for client-to-server traffic; the server
 * ignores it) + N × 28 B node entries.
 */
export function createBinaryNodeData(nodes: BinaryNodeData[]): ArrayBuffer {
  const buffer = new ArrayBuffer(BINARY_FRAME_HEADER_SIZE + nodes.length * BINARY_NODE_SIZE);
  const view = new DataView(buffer);

  view.setUint8(0, BINARY_PROTOCOL_PREAMBLE);
  // broadcast_sequence (u64 LE) — irrelevant for client→server, write 0
  view.setUint32(1, 0, true);
  view.setUint32(5, 0, true);

  for (let i = 0; i < nodes.length; i++) {
    const offset = BINARY_FRAME_HEADER_SIZE + i * BINARY_NODE_SIZE;
    const node = nodes[i];

    view.setUint32(offset, node.nodeId, true);
    view.setFloat32(offset + 4, node.position.x, true);
    view.setFloat32(offset + 8, node.position.y, true);
    view.setFloat32(offset + 12, node.position.z, true);
    view.setFloat32(offset + 16, node.velocity.x, true);
    view.setFloat32(offset + 20, node.velocity.y, true);
    view.setFloat32(offset + 24, node.velocity.z, true);
  }

  return buffer;
}

// ── Decoder (inbound: server → client) ─────────────────────────────────

/**
 * Decode a server-authored position frame.
 *
 * Returns null on validation failure; logs the reason once. Out-of-band
 * frames are silently skipped (decoder never throws — bad frames simply
 * yield an empty node map).
 */
export function decodePositionFrame(buffer: ArrayBuffer): PositionFrame | null {
  if (!buffer || buffer.byteLength < BINARY_FRAME_HEADER_SIZE) {
    return null;
  }

  const view = new DataView(buffer);
  const preamble = view.getUint8(0);
  if (preamble !== BINARY_PROTOCOL_PREAMBLE) {
    logger.warn(`Invalid binary protocol preamble: 0x${preamble.toString(16)} (expected 0x42)`);
    return null;
  }

  // u64 LE → JS Number (safe up to 2^53)
  const seqLow = view.getUint32(1, true);
  const seqHigh = view.getUint32(5, true);
  const broadcastSequence = seqLow + seqHigh * 0x100000000;

  const payloadLen = buffer.byteLength - BINARY_FRAME_HEADER_SIZE;
  if (payloadLen % BINARY_NODE_SIZE !== 0) {
    logger.warn(
      `Binary frame payload length ${payloadLen} is not a multiple of ${BINARY_NODE_SIZE}`,
    );
  }

  const nodeCount = Math.floor(payloadLen / BINARY_NODE_SIZE);
  const nodes = new Map<number, NodeEntry>();

  for (let i = 0; i < nodeCount; i++) {
    const offset = BINARY_FRAME_HEADER_SIZE + i * BINARY_NODE_SIZE;

    const nodeId = view.getUint32(offset, true);
    const x = view.getFloat32(offset + 4, true);
    const y = view.getFloat32(offset + 8, true);
    const z = view.getFloat32(offset + 12, true);
    const vx = view.getFloat32(offset + 16, true);
    const vy = view.getFloat32(offset + 20, true);
    const vz = view.getFloat32(offset + 24, true);

    // Reject non-finite values silently (server should never emit these,
    // but a single malformed entry shouldn't poison the whole frame).
    if (
      !Number.isFinite(x) || !Number.isFinite(y) || !Number.isFinite(z) ||
      !Number.isFinite(vx) || !Number.isFinite(vy) || !Number.isFinite(vz)
    ) {
      continue;
    }

    nodes.set(nodeId, {
      nodeId,
      position: { x, y, z },
      velocity: { x: vx, y: vy, z: vz },
    });
  }

  return { broadcastSequence, nodes };
}

/** Validate a buffer looks like a position frame (preamble byte check + size sanity). */
export function isPositionFrame(buffer: ArrayBuffer): boolean {
  if (!buffer || buffer.byteLength < BINARY_FRAME_HEADER_SIZE) {
    return false;
  }
  return new DataView(buffer).getUint8(0) === BINARY_PROTOCOL_PREAMBLE;
}
