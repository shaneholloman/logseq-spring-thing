/**
 * binaryProtocol.decoder.test.ts -- Client unit tests for the unified
 * position-frame decoder.
 *
 * Pins (PRD-007 §4.1 / ADR-061 §D1 / DDD aggregate `PositionFrame`):
 *   - Frame layout: [u8 0x42][u64 broadcast_sequence LE][N x 28-byte node]
 *   - Decoder produces `Map<number, NodeEntry>`
 *   - Non-0x42 preamble is rejected with a logged warning, returns null
 *     (does NOT throw, does NOT silently pass bytes through to renderers).
 *   - Frame size always equals 9 + 28*N for any N (including 0).
 *
 * Implementation under test: `decodePositionFrame` from
 * `client/src/types/binaryProtocol.ts`. The store-layer
 * `client/src/store/websocket/binaryProtocol.ts` re-uses it as a
 * pass-through.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';

// ── Mocks (logger only — the decoder is otherwise pure) ─────────────────────
//
// `vi.hoisted` is mandatory: vi.mock factories run BEFORE the module body, so
// regular `const warnSpy = vi.fn()` declarations are in TDZ when the factory
// fires. `vi.hoisted` lifts the spy creation to the same hoist phase as the
// mock itself.

const { warnSpy, errorSpy } = vi.hoisted(() => ({
  warnSpy: vi.fn(),
  errorSpy: vi.fn(),
}));

vi.mock('../../../utils/loggerConfig', () => ({
  createLogger: () => ({
    info: vi.fn(),
    warn: warnSpy,
    error: errorSpy,
    debug: vi.fn(),
  }),
  createErrorMetadata: vi.fn((e: unknown) => e),
}));

// ── Import under test ────────────────────────────────────────────────────────

import {
  decodePositionFrame,
  isPositionFrame,
  BINARY_PROTOCOL_PREAMBLE,
  BINARY_NODE_SIZE,
  BINARY_FRAME_HEADER_SIZE,
} from '../../../types/binaryProtocol';

// ── Constants & helpers ──────────────────────────────────────────────────────

const PREAMBLE = BINARY_PROTOCOL_PREAMBLE;
const NODE_STRIDE = BINARY_NODE_SIZE; // 28 — id(4) + 6×f32
const HEADER_LEN = BINARY_FRAME_HEADER_SIZE; // 9 — preamble + u64 sequence

interface NodePos {
  x: number;
  y: number;
  z: number;
  vx: number;
  vy: number;
  vz: number;
}

/**
 * Build a synthetic position frame matching the server's
 * `encode_position_frame`:
 *   [u8 0x42][u64 sequence_LE][N × 28 bytes:
 *       [u32 id_LE][f32 x][f32 y][f32 z][f32 vx][f32 vy][f32 vz]
 *   ]
 */
function buildFrame(
  nodes: Array<{ id: number } & NodePos>,
  sequence: bigint,
  preamble: number = PREAMBLE,
): ArrayBuffer {
  const buf = new ArrayBuffer(HEADER_LEN + NODE_STRIDE * nodes.length);
  const dv = new DataView(buf);
  dv.setUint8(0, preamble);
  dv.setBigUint64(1, sequence, true);
  for (let i = 0; i < nodes.length; i++) {
    const off = HEADER_LEN + i * NODE_STRIDE;
    const n = nodes[i];
    dv.setUint32(off + 0, n.id, true);
    dv.setFloat32(off + 4, n.x, true);
    dv.setFloat32(off + 8, n.y, true);
    dv.setFloat32(off + 12, n.z, true);
    dv.setFloat32(off + 16, n.vx, true);
    dv.setFloat32(off + 20, n.vy, true);
    dv.setFloat32(off + 24, n.vz, true);
  }
  return buf;
}

beforeEach(() => {
  warnSpy.mockReset();
  errorSpy.mockReset();
});

// ── Constants pin ────────────────────────────────────────────────────────────

describe('binaryProtocol — wire-constant invariants (F1 fix)', () => {
  it('NODE_STRIDE is 28 bytes — id(4) + 6×f32(24)', () => {
    expect(NODE_STRIDE).toBe(28);
  });
  it('HEADER_LEN is 9 bytes — preamble(1) + u64 sequence(8)', () => {
    expect(HEADER_LEN).toBe(9);
  });
  it('PREAMBLE is 0x42 ("B")', () => {
    expect(PREAMBLE).toBe(0x42);
  });
});

// ── Happy-path decoding ──────────────────────────────────────────────────────

describe('decodePositionFrame — single-node decode', () => {
  it('decodes 28-byte stride correctly with ids and float fields', () => {
    const buf = buildFrame(
      [{ id: 7, x: 1, y: 2, z: 3, vx: 0, vy: 0, vz: 0 }],
      999n,
    );

    const frame = decodePositionFrame(buf);
    expect(frame).not.toBeNull();
    expect(frame!.broadcastSequence).toBe(999);
    expect(frame!.nodes.size).toBe(1);

    const got = frame!.nodes.get(7)!;
    expect(got.nodeId).toBe(7);
    expect(got.position).toEqual({ x: 1, y: 2, z: 3 });
    expect(got.velocity).toEqual({ x: 0, y: 0, z: 0 });
  });
});

describe('decodePositionFrame — multi-node decode (the F1 regression test)', () => {
  it('preserves byte alignment for two nodes (no 4-byte stride drift)', () => {
    // This was the BLOCKER bug: with NODE_STRIDE = 24 the second node's
    // ID/position would have been read from the first node's vz/x/y/z,
    // yielding garbage. With the 28-byte stride fix this MUST decode both
    // nodes correctly.
    const buf = buildFrame(
      [
        { id: 1, x: 1.5, y: 2.5, z: 3.5, vx: 0.1, vy: 0.2, vz: 0.3 },
        { id: 42, x: -100.5, y: 0.0, z: 1000.0, vx: 0.0, vy: 0.0, vz: 0.0 },
      ],
      12345n,
    );

    const frame = decodePositionFrame(buf);
    expect(frame).not.toBeNull();
    expect(frame!.broadcastSequence).toBe(12345);
    expect(frame!.nodes.size).toBe(2);

    const n1 = frame!.nodes.get(1)!;
    expect(n1.position.x).toBeCloseTo(1.5, 5);
    expect(n1.position.y).toBeCloseTo(2.5, 5);
    expect(n1.position.z).toBeCloseTo(3.5, 5);
    expect(n1.velocity.x).toBeCloseTo(0.1, 5);
    expect(n1.velocity.y).toBeCloseTo(0.2, 5);
    expect(n1.velocity.z).toBeCloseTo(0.3, 5);

    const n42 = frame!.nodes.get(42)!;
    expect(n42.position.x).toBeCloseTo(-100.5, 3);
    expect(n42.position.y).toBe(0);
    expect(n42.position.z).toBeCloseTo(1000, 0);
  });
});

describe('decodePositionFrame — empty frame', () => {
  it('decodes a header-only buffer to an empty node map', () => {
    const buf = buildFrame([], 0n);
    expect(buf.byteLength).toBe(HEADER_LEN);

    const frame = decodePositionFrame(buf);
    expect(frame).not.toBeNull();
    expect(frame!.broadcastSequence).toBe(0);
    expect(frame!.nodes.size).toBe(0);
  });
});

// ── Bad preamble handling ────────────────────────────────────────────────────

describe('decodePositionFrame — bad preamble', () => {
  it('returns null for the legacy V5 preamble (5)', () => {
    const buf = new ArrayBuffer(9);
    new DataView(buf).setUint8(0, 5);
    expect(decodePositionFrame(buf)).toBeNull();
  });

  it('returns null for the legacy V3 preamble (3)', () => {
    const buf = new ArrayBuffer(9);
    new DataView(buf).setUint8(0, 3);
    expect(decodePositionFrame(buf)).toBeNull();
  });

  it('returns null for a zero preamble byte', () => {
    const buf = new ArrayBuffer(9);
    new DataView(buf).setUint8(0, 0);
    expect(decodePositionFrame(buf)).toBeNull();
  });

  it('logs a warning when the preamble is wrong (does not silently pass)', () => {
    const buf = new ArrayBuffer(9);
    new DataView(buf).setUint8(0, 5);

    const result = decodePositionFrame(buf);
    expect(result).toBeNull();
    expect(warnSpy).toHaveBeenCalled();
    const allWarnArgs = warnSpy.mock.calls.flat().map((a) => String(a)).join(' ');
    expect(allWarnArgs.toLowerCase()).toMatch(/preamble/);
  });
});

// ── Frame-size invariant: 9 + 28*N for all N ─────────────────────────────────

describe('decodePositionFrame — frame size invariant', () => {
  it.each([0, 1, 5, 100])('accepts frame of length 9 + 28*%i', (n) => {
    const nodes = Array.from({ length: n }, (_, i) => ({
      id: i + 1,
      x: i * 1.0,
      y: i * 2.0,
      z: i * 3.0,
      vx: 0,
      vy: 0,
      vz: 0,
    }));
    const buf = buildFrame(nodes, BigInt(n));

    expect(buf.byteLength).toBe(HEADER_LEN + NODE_STRIDE * n);

    const frame = decodePositionFrame(buf);
    expect(frame).not.toBeNull();
    expect(frame!.nodes.size).toBe(n);
  });

  it('returns a partially-empty result when body is not a multiple of 28', () => {
    // Header + 27 bytes (one short of a node entry). The decoder logs a
    // warning and decodes floor(27/28) = 0 nodes. It must NOT throw and
    // must NOT yield a partially-populated entry.
    const buf = new ArrayBuffer(HEADER_LEN + NODE_STRIDE - 1);
    new DataView(buf).setUint8(0, PREAMBLE);

    const frame = decodePositionFrame(buf);
    expect(frame).not.toBeNull();
    expect(frame!.nodes.size).toBe(0);
  });

  it('returns null for a buffer shorter than the 9-byte header', () => {
    const buf = new ArrayBuffer(4);
    new DataView(buf).setUint8(0, PREAMBLE);
    expect(decodePositionFrame(buf)).toBeNull();
  });
});

// ── No flag-bit decode in the per-frame path ────────────────────────────────

describe('decodePositionFrame — no flag-bit residue (DDD §I03 + §7)', () => {
  it('returns the raw id even when high bits look like legacy flags', () => {
    // ADR-061 §D3: bits 26-31 are no longer used as type/visibility
    // discriminators. The id is the raw u32.
    const buf = buildFrame(
      [{ id: 0x03ffffff, x: 0, y: 0, z: 0, vx: 0, vy: 0, vz: 0 }],
      0n,
    );

    const frame = decodePositionFrame(buf);
    expect([...frame!.nodes.keys()]).toEqual([0x03ffffff]);
  });

  it('does NOT inspect bits 26-31 to derive node type', () => {
    // All of bits 26-31 set — would have been
    // "agent + knowledge + private + ontology-property" under the legacy
    // flag scheme. The decoder must surface the literal id.
    // Coerce to unsigned u32: `|` returns a signed i32 in JS, but the wire
    // and the decoder's `getUint32` both produce unsigned values, so the
    // map key needs the `>>> 0` round-trip to compare equal.
    const wireId = (0xfc000000 | 7) >>> 0;
    const buf = buildFrame(
      [{ id: wireId, x: 9, y: 9, z: 9, vx: 0, vy: 0, vz: 0 }],
      0n,
    );

    const frame = decodePositionFrame(buf);
    expect(frame!.nodes.has(wireId)).toBe(true);
  });
});

// ── isPositionFrame guard ────────────────────────────────────────────────────

describe('isPositionFrame', () => {
  it('returns true for a buffer beginning with 0x42', () => {
    const buf = buildFrame([], 0n);
    expect(isPositionFrame(buf)).toBe(true);
  });

  it('returns false for a buffer beginning with anything else', () => {
    const buf = new ArrayBuffer(9);
    new DataView(buf).setUint8(0, 0x05);
    expect(isPositionFrame(buf)).toBe(false);
  });

  it('returns false for a buffer too small to be a frame', () => {
    expect(isPositionFrame(new ArrayBuffer(4))).toBe(false);
  });
});
