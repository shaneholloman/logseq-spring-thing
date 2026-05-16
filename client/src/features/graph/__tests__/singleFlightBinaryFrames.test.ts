/**
 * T7 — single-flight binary-frame test (ADR-03 D2).
 *
 * The proxy enforces:
 *   1. At most one frame is being processed across the Comlink await.
 *   2. Intermediate frames collapse to one (newest-wins slot).
 *   3. The pending slot drains after the in-flight promise settles,
 *      yielding via queueMicrotask before re-entering.
 *
 * We mock the Comlink-wrapped worker so processBinaryFrame() resolves
 * after a controlled delay. Then we fire N frames rapidly and assert
 * the resulting call sequence on the underlying API.
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';

// Mock comlink before importing the proxy so wrap/transfer are controllable.
vi.mock('comlink', () => ({
  wrap: vi.fn(),
  transfer: vi.fn((value: unknown, _transferList: unknown[]) => value),
  expose: vi.fn(),
}));

// Mock the worker module URL resolution (Vite import.meta.url is unavailable in jsdom).
vi.mock('../workers/graph.worker.ts?worker', () => ({
  default: class FakeWorker {
    onerror: ((e: unknown) => void) | null = null;
    terminate = vi.fn();
  },
}), { virtual: true });

// Mock Worker constructor for the `new Worker(new URL(...))` call.
class StubWorker {
  onerror: ((e: unknown) => void) | null = null;
  terminate = vi.fn();
}
(globalThis as unknown as { Worker: typeof StubWorker }).Worker = StubWorker;

// Pretend cross-origin isolation is unavailable so the proxy uses the Comlink path.
Object.defineProperty(globalThis, 'crossOriginIsolated', {
  value: false,
  writable: true,
  configurable: true,
});

// Import after mocks are wired.
import { wrap } from 'comlink';
import { graphWorkerProxy, WORKER_USES_SAB } from '../managers/graphWorkerProxy';

interface FakeWorkerApi {
  initialize: ReturnType<typeof vi.fn>;
  setupSharedPositions: ReturnType<typeof vi.fn>;
  processBinaryFrame: ReturnType<typeof vi.fn>;
  setGraphData: ReturnType<typeof vi.fn>;
}

describe('single-flight binary frame discipline (ADR-03 D2)', () => {
  let fakeApi: FakeWorkerApi;
  let resolvers: Array<() => void>;

  beforeEach(async () => {
    // Reset singleton state (the proxy is a singleton; we re-initialise).
    await graphWorkerProxy.dispose();

    resolvers = [];
    fakeApi = {
      initialize: vi.fn().mockResolvedValue(undefined),
      setupSharedPositions: vi.fn().mockResolvedValue(undefined),
      processBinaryFrame: vi.fn((_buf: ArrayBuffer) => {
        return new Promise<ArrayBuffer | void>(resolve => {
          resolvers.push(() => resolve(new ArrayBuffer(0)));
        });
      }),
      setGraphData: vi.fn().mockResolvedValue(undefined),
    };

    (wrap as ReturnType<typeof vi.fn>).mockReturnValue(fakeApi);

    await graphWorkerProxy.initialize();
  });

  it('forces Comlink path when crossOriginIsolated is false', () => {
    expect(WORKER_USES_SAB).toBe(false);
  });

  it('processes only first + newest-wins of 10 rapid frames', async () => {
    // Fire 10 frames before any resolves.
    const frames = Array.from({ length: 10 }, (_, i) => new Uint8Array([i]));
    const promises = frames.map(f => graphWorkerProxy.processBinaryFrame(f));

    // First frame should be in flight; subsequent frames should NOT have
    // reached the worker (single-flight guard).
    await Promise.resolve(); // let microtasks settle
    expect(fakeApi.processBinaryFrame).toHaveBeenCalledTimes(1);

    // Resolve the first call. Pending slot (newest = frame[9]) should drain
    // via queueMicrotask.
    resolvers[0]();
    await Promise.all(promises.slice(0, 1));

    // Drain microtask queue.
    await new Promise(r => queueMicrotask(() => r(undefined)));
    await new Promise(r => queueMicrotask(() => r(undefined)));

    // Only two calls total: first + newest-wins.
    expect(fakeApi.processBinaryFrame).toHaveBeenCalledTimes(2);
  });

  it('drops intermediate frames; pending slot always holds newest', async () => {
    const f1 = new Uint8Array([1]);
    const f2 = new Uint8Array([2]);
    const f3 = new Uint8Array([3]);

    // Fire 3 frames: f1 takes the in-flight slot; f2 is replaced by f3.
    void graphWorkerProxy.processBinaryFrame(f1);
    void graphWorkerProxy.processBinaryFrame(f2);
    void graphWorkerProxy.processBinaryFrame(f3);

    await Promise.resolve();
    expect(fakeApi.processBinaryFrame).toHaveBeenCalledTimes(1);

    // Resolve f1 → pending slot drains with f3 (not f2).
    resolvers[0]();
    await new Promise(r => queueMicrotask(() => r(undefined)));
    await new Promise(r => queueMicrotask(() => r(undefined)));

    expect(fakeApi.processBinaryFrame).toHaveBeenCalledTimes(2);
    // The dropped frame counter reflects f2 being replaced.
    const stats = graphWorkerProxy.getStats();
    expect(stats.framesDropped).toBeGreaterThanOrEqual(1);
  });

  it('reports stats: framesProcessed counter increments on success', async () => {
    const f1 = new Uint8Array([1]);
    void graphWorkerProxy.processBinaryFrame(f1);
    resolvers[0]();
    await new Promise(r => queueMicrotask(() => r(undefined)));
    await new Promise(r => queueMicrotask(() => r(undefined)));

    expect(graphWorkerProxy.getStats().framesProcessed).toBeGreaterThanOrEqual(1);
  });
});
