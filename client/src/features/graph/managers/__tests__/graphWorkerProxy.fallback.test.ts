// @ts-ignore - vitest types may not be available in all environments
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';

/**
 * Regression test for BUG #2:
 *
 * When SharedArrayBuffer is unavailable (e.g. COOP/COEP headers missing so the
 * page isn't cross-origin isolated), `graphWorkerProxy.processBinaryData()`
 * must fall back to `workerApi.getCurrentPositions()` and cache the result in
 * `lastReceivedPositions`. `getPositionsSync()` then returns those cached
 * positions instead of null — the renderer otherwise shows a frozen graph.
 *
 * Path under test:
 *   client/src/features/graph/managers/graphWorkerProxy.ts:198-216, 358-360
 *
 * The fallback is never exercised in the existing graphDataManager tests, and
 * a silent regression here is invisible from backend logs (worker just stops
 * receiving positions).
 */

// --- Hoisted SAB disabling ---
// We must delete SharedArrayBuffer BEFORE the module is imported because
// graphWorkerProxy captures references during initialize(). Vitest hoists
// `vi.stubGlobal` to the top of the file.

const originalSAB = (globalThis as any).SharedArrayBuffer;

// --- Mocks ---
vi.mock('../../../../utils/loggerConfig', () => ({
  createLogger: () => ({
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
    debug: vi.fn(),
  }),
}));

vi.mock('../../../../utils/clientDebugState', () => ({
  debugState: {
    isEnabled: () => false,
    isDataDebugEnabled: () => false,
  },
}));

// Comlink wrap returns whatever we give it — the test installs a fake worker API.
vi.mock('comlink', () => ({
  wrap: (w: any) => w.__api,
}));

describe('GraphWorkerProxy — SharedArrayBuffer fallback (BUG #2 regression)', () => {
  let fakeWorker: any;
  let fakeApi: any;
  let proxy: any;

  beforeEach(async () => {
    vi.resetModules();

    // Make SharedArrayBuffer undefined for this test — simulates the COEP-less
    // browser context where the bug manifests.
    // @ts-ignore
    delete (globalThis as any).SharedArrayBuffer;
    expect(typeof (globalThis as any).SharedArrayBuffer).toBe('undefined');

    // Stub cross-origin isolation flag as false, matching no-SAB reality.
    Object.defineProperty(globalThis, 'crossOriginIsolated', {
      value: false,
      configurable: true,
      writable: true,
    });

    // Fake worker API that mimics the Comlink-wrapped surface the proxy uses.
    fakeApi = {
      initialize: vi.fn().mockResolvedValue(undefined),
      setupSharedPositions: vi.fn().mockResolvedValue(undefined),
      setGraphType: vi.fn().mockResolvedValue(undefined),
      processBinaryData: vi.fn().mockResolvedValue(new Float32Array([1, 2, 3, 4])),
      getCurrentPositions: vi.fn(), // set per test
    };

    fakeWorker = {
      __api: fakeApi,
      addEventListener: vi.fn(),
      terminate: vi.fn(),
      onerror: null,
    };

    // Replace global Worker so the proxy's `new Worker(...)` returns our fake.
    // @ts-ignore
    globalThis.Worker = vi.fn().mockImplementation(() => fakeWorker) as any;

    // Fresh import after globals are stubbed.
    const mod = await import('../graphWorkerProxy');
    proxy = mod.graphWorkerProxy;

    // Reset singleton internal state by disposing any prior instance state.
    proxy.dispose();

    await proxy.initialize();
  });

  afterEach(() => {
    if (originalSAB) {
      (globalThis as any).SharedArrayBuffer = originalSAB;
    }
    vi.restoreAllMocks();
  });

  it('processBinaryData populates lastReceivedPositions from getCurrentPositions() when SAB is undefined', async () => {
    const freshPositions = new Float32Array([10.0, 20.0, 30.0, 40.0, 50.0, 60.0]);
    fakeApi.getCurrentPositions.mockResolvedValueOnce(freshPositions);

    // Before the first processBinaryData call, sync read should be null.
    expect(proxy.getPositionsSync()).toBeNull();

    // Simulate a WS binary frame arriving.
    const buf = new ArrayBuffer(28);
    await proxy.processBinaryData(buf);

    // getCurrentPositions MUST have been queried because the SAB view is null.
    expect(fakeApi.getCurrentPositions).toHaveBeenCalledTimes(1);

    // And the cached positions must be exactly what the worker returned.
    const sync = proxy.getPositionsSync();
    expect(sync).not.toBeNull();
    expect(sync).toBe(freshPositions);
    expect(Array.from(sync!)).toEqual([10, 20, 30, 40, 50, 60]);
  });

  it('does not overwrite lastReceivedPositions when getCurrentPositions returns an empty array', async () => {
    // First tick: valid positions — cache is seeded.
    const good = new Float32Array([1, 1, 1]);
    fakeApi.getCurrentPositions.mockResolvedValueOnce(good);
    await proxy.processBinaryData(new ArrayBuffer(28));
    expect(proxy.getPositionsSync()).toBe(good);

    // Second tick: worker transiently returns nothing — the cache must persist
    // so the renderer keeps the last known frame rather than going blank.
    fakeApi.getCurrentPositions.mockResolvedValueOnce(new Float32Array(0));
    await proxy.processBinaryData(new ArrayBuffer(28));

    expect(proxy.getPositionsSync()).toBe(good);
  });

  it('skips the getCurrentPositions fallback entirely when a SharedArrayBuffer view is present', async () => {
    // Re-enable SAB and re-init so the proxy takes the zero-copy path.
    (globalThis as any).SharedArrayBuffer = originalSAB || ArrayBuffer;
    proxy.dispose();

    // Inject a pretend SAB view directly — avoids needing a real SAB in JSDOM.
    await proxy.initialize();
    (proxy as any).sharedPositionView = new Float32Array([9, 9, 9]);

    fakeApi.getCurrentPositions.mockClear();
    await proxy.processBinaryData(new ArrayBuffer(28));

    // With SAB present the fallback path is dead code.
    expect(fakeApi.getCurrentPositions).not.toHaveBeenCalled();
    expect(proxy.getPositionsSync()).toEqual(new Float32Array([9, 9, 9]));
  });
});
