import { describe, it, expect } from 'vitest';
import { encodeBroadcastAckPayload, decodeBroadcastAck, BROADCAST_ACK_PAYLOAD_SIZE } from '../backpressure';

describe('encodeBroadcastAckPayload / decodeBroadcastAck round-trip', () => {
  it('roundtrips small sequence ID', () => {
    const buf = encodeBroadcastAckPayload(42, 100);
    expect(buf.byteLength).toBe(BROADCAST_ACK_PAYLOAD_SIZE);
    const decoded = decodeBroadcastAck(buf);
    expect(decoded).not.toBeNull();
    expect(decoded!.sequenceId).toBe(42);
    expect(decoded!.nodesReceived).toBe(100);
    expect(decoded!.timestamp).toBeGreaterThan(0);
  });

  it('roundtrips large sequence ID (> 2^32)', () => {
    const largeSeq = 0x1_0000_0001; // 4294967297
    const buf = encodeBroadcastAckPayload(largeSeq, 5000);
    const decoded = decodeBroadcastAck(buf);
    expect(decoded!.sequenceId).toBe(largeSeq);
    expect(decoded!.nodesReceived).toBe(5000);
  });

  it('roundtrips zero sequence and zero nodes', () => {
    const buf = encodeBroadcastAckPayload(0, 0);
    const decoded = decodeBroadcastAck(buf);
    expect(decoded!.sequenceId).toBe(0);
    expect(decoded!.nodesReceived).toBe(0);
  });

  it('returns null for undersized payload', () => {
    expect(decodeBroadcastAck(new ArrayBuffer(BROADCAST_ACK_PAYLOAD_SIZE - 1))).toBeNull();
  });

  it('payload size constant matches encode output', () => {
    const buf = encodeBroadcastAckPayload(1, 1);
    expect(buf.byteLength).toBe(BROADCAST_ACK_PAYLOAD_SIZE);
  });

  it('timestamp is close to Date.now()', () => {
    const before = Date.now();
    const buf = encodeBroadcastAckPayload(1, 1);
    const after = Date.now();
    const decoded = decodeBroadcastAck(buf);
    expect(decoded!.timestamp).toBeGreaterThanOrEqual(before);
    expect(decoded!.timestamp).toBeLessThanOrEqual(after + 10);
  });
});
