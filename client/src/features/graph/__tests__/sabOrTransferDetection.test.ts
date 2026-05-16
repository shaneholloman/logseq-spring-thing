/**
 * T7 — SAB-vs-Comlink capability detection (ADR-03 D3).
 *
 * `WORKER_USES_SAB` is computed once at module load from:
 *   - typeof SharedArrayBuffer !== 'undefined'
 *   - self.crossOriginIsolated === true
 *
 * VITE_FORCE_COMLINK=1 overrides to false.
 *
 * Because the constant is captured at import time, we test by importing
 * the module under different global conditions via `vi.resetModules`.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

describe('WORKER_USES_SAB capability detection (ADR-03 D3)', () => {
  beforeEach(() => {
    vi.resetModules();
  });

  afterEach(() => {
    // Restore defaults
    Object.defineProperty(globalThis, 'crossOriginIsolated', {
      value: false,
      writable: true,
      configurable: true,
    });
  });

  it('returns false when crossOriginIsolated is false', async () => {
    Object.defineProperty(globalThis, 'crossOriginIsolated', {
      value: false,
      writable: true,
      configurable: true,
    });
    const { WORKER_USES_SAB } = await import('../managers/graphWorkerProxy');
    expect(WORKER_USES_SAB).toBe(false);
  });

  it('returns false when SharedArrayBuffer is undefined', async () => {
    const originalSAB = (globalThis as unknown as { SharedArrayBuffer?: unknown }).SharedArrayBuffer;
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    delete (globalThis as any).SharedArrayBuffer;
    try {
      const { WORKER_USES_SAB } = await import('../managers/graphWorkerProxy');
      expect(WORKER_USES_SAB).toBe(false);
    } finally {
      (globalThis as unknown as { SharedArrayBuffer?: unknown }).SharedArrayBuffer = originalSAB;
    }
  });

  it('returns true when both SAB and crossOriginIsolated are available', async () => {
    Object.defineProperty(globalThis, 'crossOriginIsolated', {
      value: true,
      writable: true,
      configurable: true,
    });
    const { WORKER_USES_SAB } = await import('../managers/graphWorkerProxy');
    expect(WORKER_USES_SAB).toBe(true);
  });
});
