
import { createLogger } from '../../utils/loggerConfig';
import type { BroadcastAckData } from './frameTypes';

const logger = createLogger('BinaryWebSocketProtocol');

// ACK payload: 8 bytes sequenceId + 4 bytes nodesReceived + 8 bytes timestamp = 20 bytes
export const BROADCAST_ACK_PAYLOAD_SIZE = 20;

/**
 * Encode a broadcast acknowledgement payload for backpressure flow control.
 * Send after the client processes a position update broadcast.
 *
 * @param sequenceId - Server broadcast sequence number (from position update header)
 * @param nodesReceived - Number of nodes successfully processed
 * @returns Binary payload (caller wraps with createMessage)
 */
export function encodeBroadcastAckPayload(
  sequenceId: number,
  nodesReceived: number
): ArrayBuffer {
  const payload = new ArrayBuffer(BROADCAST_ACK_PAYLOAD_SIZE);
  const view = new DataView(payload);

  // Write sequenceId as BigInt64 (8 bytes, little-endian)
  const lowBits = sequenceId >>> 0;
  const highBits = Math.floor(sequenceId / 0x100000000) >>> 0;
  view.setUint32(0, lowBits, true);
  view.setUint32(4, highBits, true);

  // Write nodesReceived (4 bytes, little-endian)
  view.setUint32(8, nodesReceived, true);

  // Write timestamp (8 bytes, little-endian)
  const timestamp = Date.now();
  const tsLowBits = timestamp >>> 0;
  const tsHighBits = Math.floor(timestamp / 0x100000000) >>> 0;
  view.setUint32(12, tsLowBits, true);
  view.setUint32(16, tsHighBits, true);

  return payload;
}

/**
 * Decode a broadcast acknowledgement payload.
 */
export function decodeBroadcastAck(payload: ArrayBuffer): BroadcastAckData | null {
  if (payload.byteLength < BROADCAST_ACK_PAYLOAD_SIZE) {
    logger.error('Broadcast ACK payload too small');
    return null;
  }

  const view = new DataView(payload);

  // Read sequenceId (8 bytes, little-endian)
  const lowBits = view.getUint32(0, true);
  const highBits = view.getUint32(4, true);
  const sequenceId = lowBits + highBits * 0x100000000;

  // Read nodesReceived (4 bytes)
  const nodesReceived = view.getUint32(8, true);

  // Read timestamp (8 bytes)
  const tsLowBits = view.getUint32(12, true);
  const tsHighBits = view.getUint32(16, true);
  const timestamp = tsLowBits + tsHighBits * 0x100000000;

  return { sequenceId, nodesReceived, timestamp };
}
