import { describe, it, expect } from 'vitest';
import { findFreeMappedId } from '../id-mapping';
import { stringToU32 } from '../../../../../types/idMapping';

describe('findFreeMappedId', () => {
  it('returns the FNV hash when the slot is free', () => {
    const map = new Map<number, string>();
    const id = 'node-a';
    const result = findFreeMappedId(id, map);
    expect(result).toBe(stringToU32(id));
  });

  it('returns the existing slot when the same nodeId already occupies it', () => {
    const map = new Map<number, string>();
    const id = 'node-a';
    const h = stringToU32(id);
    map.set(h, id);
    expect(findFreeMappedId(id, map)).toBe(h);
  });

  it('probes past a collision to find a free slot', () => {
    const map = new Map<number, string>();
    const id = 'node-a';
    const h = stringToU32(id);
    // Pre-occupy the natural hash slot with a DIFFERENT node
    map.set(h, 'other-node');
    const result = findFreeMappedId(id, map);
    // Must differ from natural hash and not equal the occupying node's entry
    expect(result).not.toBe(h);
    expect(map.has(result)).toBe(false); // slot was free
  });

  it('fills the map for many distinct IDs without collision', () => {
    const map = new Map<number, string>();
    const ids = Array.from({ length: 200 }, (_, i) => `node-${i}`);
    const assigned: number[] = [];
    for (const id of ids) {
      const h = findFreeMappedId(id, map);
      expect(assigned).not.toContain(h);
      assigned.push(h);
      map.set(h, id);
    }
    expect(assigned.length).toBe(200);
  });

  it('throws when probe limit is exhausted', () => {
    // The function checks slots: h (probe=0), then (h + 1^2), (h + 2^2), ..., (h + 1000^2).
    // Fill all of them with a dummy occupant (not equal to `id`) so no free slot is found.
    const id = 'probe-storm';
    let h = stringToU32(id);
    const map = new Map<number, string>();
    // Slot at probe=0 (initial h)
    map.set(h, 'dummy-occupant-0');
    // Slots after each quadratic step (probe 1..1000)
    for (let probe = 1; probe <= 1000; probe++) {
      h = (h + probe * probe) >>> 0;
      map.set(h, `dummy-occupant-${probe}`);
    }
    expect(() => findFreeMappedId(id, map)).toThrow('Hash collision limit exceeded');
  });

  it('handles single-character IDs', () => {
    const map = new Map<number, string>();
    const result = findFreeMappedId('x', map);
    expect(typeof result).toBe('number');
    expect(result).toBeGreaterThanOrEqual(0);
  });

  it('handles empty-string ID', () => {
    const map = new Map<number, string>();
    const result = findFreeMappedId('', map);
    expect(typeof result).toBe('number');
  });
});
