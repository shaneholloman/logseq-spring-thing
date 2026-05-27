
import { createLogger } from '../../utils/loggerConfig';
import { SSSP_DATA_SIZE, VOICE_HEADER_SIZE } from './frameTypes';
import type { SSSPData, VoiceChunk } from './frameTypes';

const logger = createLogger('BinaryWebSocketProtocol');

export function encodeSSSPPayload(nodes: SSSPData[]): ArrayBuffer {
  const payload = new ArrayBuffer(nodes.length * SSSP_DATA_SIZE);
  const view = new DataView(payload);

  nodes.forEach((node, index) => {
    const offset = index * SSSP_DATA_SIZE;
    // V4 layout: [u32 nodeId][f32 distance][u32 parentId][u16 flags] = 14 bytes
    view.setUint32(offset, node.nodeId, true);
    view.setFloat32(offset + 4, node.distance, true);
    view.setUint32(offset + 8, node.parentId, true);
    view.setUint16(offset + 12, node.flags, true);
  });

  return payload;
}

export function decodeSSSPData(payload: ArrayBuffer): SSSPData[] {
  const nodes: SSSPData[] = [];
  const view = new DataView(payload);
  const nodeCount = Math.floor(payload.byteLength / SSSP_DATA_SIZE);

  for (let i = 0; i < nodeCount; i++) {
    const offset = i * SSSP_DATA_SIZE;

    if (offset + SSSP_DATA_SIZE > payload.byteLength) {
      logger.warn('Truncated SSSP data');
      break;
    }

    // V4 layout: [u32 nodeId][f32 distance][u32 parentId][u16 flags] = 14 bytes
    nodes.push({
      nodeId: view.getUint32(offset, true),
      distance: view.getFloat32(offset + 4, true),
      parentId: view.getUint32(offset + 8, true),
      flags: view.getUint16(offset + 12, true)
    });
  }

  return nodes;
}

export function encodeVoiceChunkPayload(chunk: VoiceChunk): ArrayBuffer {
  const totalSize = VOICE_HEADER_SIZE + chunk.audioData.byteLength;
  const payload = new ArrayBuffer(totalSize);
  const view = new DataView(payload);

  // Voice protocol uses uint16 for agentId (max 65535 agents).
  // Position protocol uses uint32 with flag bits (26-bit ID + 6 type flags).
  // These are separate wire formats — no alignment issue.
  view.setUint16(0, chunk.agentId, true);
  view.setUint16(2, chunk.chunkId, true);
  view.setUint8(4, chunk.format);
  view.setUint16(5, chunk.dataLength, true);

  new Uint8Array(payload, VOICE_HEADER_SIZE).set(new Uint8Array(chunk.audioData));

  return payload;
}

export function decodeVoiceChunk(payload: ArrayBuffer): VoiceChunk | null {
  if (payload.byteLength < VOICE_HEADER_SIZE) {
    logger.error('Voice chunk payload too small');
    return null;
  }

  const view = new DataView(payload);
  const dataLength = view.getUint16(5, true);

  if (payload.byteLength < VOICE_HEADER_SIZE + dataLength) {
    logger.error('Voice chunk audio data truncated');
    return null;
  }

  return {
    // Voice protocol uses uint16 for agentId (max 65535 agents).
    // Position protocol uses uint32 with flag bits (26-bit ID + 6 type flags).
    // These are separate wire formats — no alignment issue.
    agentId: view.getUint16(0, true),
    chunkId: view.getUint16(2, true),
    format: view.getUint8(4),
    dataLength: dataLength,
    audioData: payload.slice(VOICE_HEADER_SIZE, VOICE_HEADER_SIZE + dataLength)
  };
}
