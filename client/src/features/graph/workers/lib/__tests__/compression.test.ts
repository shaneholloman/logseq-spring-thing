import { describe, it, expect } from 'vitest';
import { isZlibCompressed } from '../compression';

// All four zlib CMF+FLG magic byte combos
const ZLIB_HEADERS: [number, number][] = [
  [0x78, 0x01],
  [0x78, 0x5e],
  [0x78, 0x9c],
  [0x78, 0xda],
];

function makeBuffer(...bytes: number[]): ArrayBuffer {
  return new Uint8Array(bytes).buffer;
}

describe('isZlibCompressed', () => {
  it('returns false for empty buffer', () => {
    expect(isZlibCompressed(new ArrayBuffer(0))).toBe(false);
  });

  it('returns false for single-byte buffer', () => {
    expect(isZlibCompressed(makeBuffer(0x78))).toBe(false);
  });

  it.each(ZLIB_HEADERS)(
    'returns true for valid zlib header 0x78,0x%s',
    (cmf, flg) => {
      expect(isZlibCompressed(makeBuffer(cmf, flg, 0x00))).toBe(true);
    }
  );

  it('returns false when first byte is not 0x78', () => {
    expect(isZlibCompressed(makeBuffer(0x1f, 0x8b))).toBe(false); // gzip magic
  });

  it('returns false for 0x78 followed by unknown FLG byte', () => {
    expect(isZlibCompressed(makeBuffer(0x78, 0x00))).toBe(false);
    expect(isZlibCompressed(makeBuffer(0x78, 0xff))).toBe(false);
  });

  it('returns false for all-zero two-byte buffer', () => {
    expect(isZlibCompressed(makeBuffer(0x00, 0x00))).toBe(false);
  });
});
