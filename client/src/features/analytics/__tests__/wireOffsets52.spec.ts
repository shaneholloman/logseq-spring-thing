// ADR-031 D2/D7 — Client-side wire-offset contract for the 52 B NodeAnalytics
// V3 record. Asserts the client parser agrees with the server `NodeAnalytics`
// struct offsets (centrality@48) and that the per-node stride is 52 B.
//
// This is the TS half of the cross-language offset assertion: the Rust suite
// (tests/analytics_wire_snapshot_test.rs) pins the encoder bytes; this spec
// pins the decoder offsets. Both must reference the SAME numbers.
//
// CURRENT STATE GAP: the live client (src/types/binaryProtocol.ts) still
// declares BINARY_NODE_SIZE_V3 = 48 with no centrality offset. The
// `currentClientStillOn48` test below documents that gap; it will need to be
// updated (and the production constants moved to 52) when ADR-031 D2 lands.

import { describe, it, expect } from 'vitest';
import * as wire from '../../../types/binaryProtocol';

// Canonical 52 B layout (must match tests/analytics_fixtures.rs exactly).
const OFF_ID = 0;
const OFF_POSITION = 4;
const OFF_VELOCITY = 16;
const OFF_SSSP_DISTANCE = 28;
const OFF_SSSP_PARENT = 32;
const OFF_CLUSTER_ID = 36;
const OFF_ANOMALY = 40;
const OFF_COMMUNITY = 44;
const OFF_CENTRALITY = 48;
const STRIDE_52 = 52;

/** Build one 52 B little-endian record from explicit field values. */
function encodeRecord52(r: {
  id: number;
  pos: [number, number, number];
  vel: [number, number, number];
  ssspDistance: number;
  ssspParent: number;
  clusterId: number;
  anomaly: number;
  communityId: number;
  centrality: number;
}): ArrayBuffer {
  const buf = new ArrayBuffer(STRIDE_52);
  const dv = new DataView(buf);
  dv.setUint32(OFF_ID, r.id, true);
  dv.setFloat32(OFF_POSITION, r.pos[0], true);
  dv.setFloat32(OFF_POSITION + 4, r.pos[1], true);
  dv.setFloat32(OFF_POSITION + 8, r.pos[2], true);
  dv.setFloat32(OFF_VELOCITY, r.vel[0], true);
  dv.setFloat32(OFF_VELOCITY + 4, r.vel[1], true);
  dv.setFloat32(OFF_VELOCITY + 8, r.vel[2], true);
  dv.setFloat32(OFF_SSSP_DISTANCE, r.ssspDistance, true);
  dv.setInt32(OFF_SSSP_PARENT, r.ssspParent, true);
  dv.setUint32(OFF_CLUSTER_ID, r.clusterId, true);
  dv.setFloat32(OFF_ANOMALY, r.anomaly, true);
  dv.setUint32(OFF_COMMUNITY, r.communityId, true);
  dv.setFloat32(OFF_CENTRALITY, r.centrality, true);
  return buf;
}

describe('NodeAnalytics 52 B wire offsets (ADR-031 D2)', () => {
  it('offsets match the server NodeAnalytics struct contract', () => {
    expect(OFF_ID).toBe(0);
    expect(OFF_POSITION).toBe(4);
    expect(OFF_VELOCITY).toBe(16);
    expect(OFF_SSSP_DISTANCE).toBe(28);
    expect(OFF_SSSP_PARENT).toBe(32);
    expect(OFF_CLUSTER_ID).toBe(36);
    expect(OFF_ANOMALY).toBe(40);
    expect(OFF_COMMUNITY).toBe(44);
    expect(OFF_CENTRALITY).toBe(48);
    expect(OFF_CENTRALITY + 4).toBe(STRIDE_52);
  });

  it('decodes every field from its declared offset (centrality@48)', () => {
    const buf = encodeRecord52({
      id: 42,
      pos: [1.0, 2.0, 3.0],
      vel: [0.5, -0.5, 0.25],
      ssspDistance: 7.5,
      ssspParent: 13,
      clusterId: 3,
      anomaly: 1.75,
      communityId: 9,
      centrality: 0.125,
    });
    const dv = new DataView(buf);
    expect(dv.getUint32(OFF_ID, true)).toBe(42);
    expect(dv.getFloat32(OFF_POSITION, true)).toBeCloseTo(1.0);
    expect(dv.getFloat32(OFF_VELOCITY + 4, true)).toBeCloseTo(-0.5);
    expect(dv.getFloat32(OFF_SSSP_DISTANCE, true)).toBeCloseTo(7.5);
    expect(dv.getInt32(OFF_SSSP_PARENT, true)).toBe(13);
    expect(dv.getUint32(OFF_CLUSTER_ID, true)).toBe(3);
    expect(dv.getFloat32(OFF_ANOMALY, true)).toBeCloseTo(1.75);
    expect(dv.getUint32(OFF_COMMUNITY, true)).toBe(9);
    // The new D2 slot — centrality must decode from byte 48.
    expect(dv.getFloat32(OFF_CENTRALITY, true)).toBeCloseTo(0.125);
  });

  it('byte-matches the Rust golden snapshot (cross-language parity)', () => {
    const buf = encodeRecord52({
      id: 42,
      pos: [1.0, 2.0, 3.0],
      vel: [0.5, -0.5, 0.25],
      ssspDistance: 7.5,
      ssspParent: 13,
      clusterId: 3,
      anomaly: 1.75,
      communityId: 9,
      centrality: 0.125,
    });
    // Same golden bytes pinned in tests/analytics_wire_snapshot_test.rs.
    const golden = new Uint8Array([
      0x2a, 0x00, 0x00, 0x00,
      0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x40, 0x40,
      0x00, 0x00, 0x00, 0x3f, 0x00, 0x00, 0x00, 0xbf, 0x00, 0x00, 0x80, 0x3e,
      0x00, 0x00, 0xf0, 0x40,
      0x0d, 0x00, 0x00, 0x00,
      0x03, 0x00, 0x00, 0x00,
      0x00, 0x00, 0xe0, 0x3f,
      0x09, 0x00, 0x00, 0x00,
      0x00, 0x00, 0x00, 0x3e,
    ]);
    expect(new Uint8Array(buf)).toEqual(golden);
  });

  it('multi-node stride: centrality does not bleed across the 52 B boundary', () => {
    const n = 3;
    const buf = new ArrayBuffer(n * STRIDE_52);
    const u8 = new Uint8Array(buf);
    for (let i = 0; i < n; i++) {
      const rec = encodeRecord52({
        id: i,
        pos: [i, i + 0.5, i + 1],
        vel: [0, 0, 0],
        ssspDistance: i * 2,
        ssspParent: i - 1,
        clusterId: i + 1,
        anomaly: i / 10,
        communityId: i + 100,
        centrality: i / 1000,
      });
      u8.set(new Uint8Array(rec), i * STRIDE_52);
    }
    const dv = new DataView(buf);
    for (let i = 0; i < n; i++) {
      const base = i * STRIDE_52;
      expect(dv.getUint32(base + OFF_ID, true)).toBe(i);
      expect(dv.getUint32(base + OFF_CLUSTER_ID, true)).toBe(i + 1);
      expect(dv.getUint32(base + OFF_COMMUNITY, true)).toBe(i + 100);
      expect(dv.getFloat32(base + OFF_CENTRALITY, true)).toBeCloseTo(i / 1000);
    }
  });
});

describe('live client constants agree with the 52 B contract (ADR-031 D2)', () => {
  // The client has already landed the 52 B layout (BINARY_NODE_SIZE_V3 = 52,
  // BINARY_CENTRALITY_OFFSET = 48). These bind the contract to the LIVE
  // exported constants so any client-side regression fails CI.
  it('client node size is 52 B (centrality appended)', () => {
    expect(wire.BINARY_NODE_SIZE_V3).toBe(STRIDE_52);
    expect(wire.BINARY_NODE_SIZE).toBe(STRIDE_52);
  });

  it('client exports centrality offset at byte 48', () => {
    expect(wire.BINARY_CENTRALITY_OFFSET).toBe(OFF_CENTRALITY);
  });

  it('client preserves the pre-existing analytics offsets', () => {
    expect(wire.BINARY_NODE_ID_OFFSET).toBe(OFF_ID);
    expect(wire.BINARY_POSITION_OFFSET).toBe(OFF_POSITION);
    expect(wire.BINARY_VELOCITY_OFFSET).toBe(OFF_VELOCITY);
    expect(wire.BINARY_SSSP_DISTANCE_OFFSET).toBe(OFF_SSSP_DISTANCE);
    expect(wire.BINARY_SSSP_PARENT_OFFSET).toBe(OFF_SSSP_PARENT);
    expect(wire.BINARY_CLUSTER_ID_OFFSET).toBe(OFF_CLUSTER_ID);
    expect(wire.BINARY_ANOMALY_SCORE_OFFSET).toBe(OFF_ANOMALY);
    expect(wire.BINARY_COMMUNITY_ID_OFFSET).toBe(OFF_COMMUNITY);
  });

  it('client parser decodes a 52 B record produced from the contract offsets', () => {
    const buf = encodeRecord52({
      id: 99,
      pos: [4, 5, 6],
      vel: [0.1, 0.2, 0.3],
      ssspDistance: 2.5,
      ssspParent: 7,
      clusterId: 4,
      anomaly: 2.0,
      communityId: 11,
      centrality: 0.42,
    });
    // Prepend the 1-byte protocol-version header the client parser expects.
    const framed = new Uint8Array(1 + buf.byteLength);
    framed[0] = wire.PROTOCOL_V3;
    framed.set(new Uint8Array(buf), 1);
    const parsed = wire.parseBinaryNodeData(framed.buffer);
    expect(parsed).toHaveLength(1);
    const n = parsed[0];
    expect(n.clusterId).toBe(4);
    expect(n.communityId).toBe(11);
    expect(n.anomalyScore).toBeCloseTo(2.0);
    expect((n as { centrality?: number }).centrality).toBeCloseTo(0.42);
  });
});
