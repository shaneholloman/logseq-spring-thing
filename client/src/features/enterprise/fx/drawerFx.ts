/**
 * drawerFx.ts
 *
 * Lazy loader + rAF driver for the `drawer-fx` WASM effect.
 *
 *   const fx = await loadDrawerFx();
 *   if (fx) { fx.attach(canvas); fx.start(); }
 *
 * Returns `null` (never throws) on any failure path -- callers should
 * apply a CSS fallback in that case.
 */

export interface DrawerFxController {
  attach(canvas: HTMLCanvasElement): void;
  resize(width: number, height: number): void;
  start(): void;
  stop(): void;
  pulse(x: number, y: number): void;
  setQuality(q: 0 | 1 | 2): void;
  destroy(): void;
}

// The WASM module exposes `DrawerFx` (class) and `version` (fn) after init.
type DrawerFxWasmModule = {
  default: (input?: unknown) => Promise<unknown>;
  DrawerFx: new (canvasId: string, width: number, height: number) => {
    tick(dt_ms: number): void;
    resize(w: number, h: number): void;
    pulse(x: number, y: number): void;
    set_quality(q: number): void;
    free(): void;
  };
  version: () => string;
};

let modulePromise: Promise<DrawerFxWasmModule | null> | null = null;

async function loadModule(): Promise<DrawerFxWasmModule | null> {
  if (!modulePromise) {
    // Dynamic import so the WASM chunk is split from the main bundle and
    // deferred until the drawer is actually opened.
    modulePromise = (async () => {
      try {
        // @ts-expect-error -- generated artifact path; exists after wasm:drawer-fx build.
        const mod: DrawerFxWasmModule = await import('./wasm/drawer_fx.js');
        await mod.default();
        return mod;
      } catch (err) {
        // eslint-disable-next-line no-console
        console.warn('[drawer-fx] WASM load failed; falling back to CSS:', err);
        return null;
      }
    })();
  }
  return modulePromise;
}

export async function loadDrawerFx(): Promise<DrawerFxController | null> {
  const mod = await loadModule();
  if (!mod) return null;

  let instance: InstanceType<DrawerFxWasmModule['DrawerFx']> | null = null;
  let canvas: HTMLCanvasElement | null = null;
  let rafId = 0;
  let lastTs = 0;
  let running = false;
  let canvasId = '';

  const ensureId = (c: HTMLCanvasElement) => {
    if (!c.id) c.id = `drawer-fx-${Math.random().toString(36).slice(2, 9)}`;
    return c.id;
  };

  const loop = (ts: number) => {
    if (!running || !instance) return;
    if (document.hidden) {
      // Pause the rAF chain while the tab is hidden. Resume fires on
      // visibilitychange below.
      rafId = 0;
      return;
    }
    const dt = lastTs === 0 ? 16.6 : ts - lastTs;
    lastTs = ts;
    try {
      instance.tick(dt);
    } catch (e) {
      // eslint-disable-next-line no-console
      console.warn('[drawer-fx] tick error:', e);
      running = false;
      return;
    }
    rafId = requestAnimationFrame(loop);
  };

  const onVisibility = () => {
    if (!document.hidden && running && rafId === 0) {
      lastTs = 0;
      rafId = requestAnimationFrame(loop);
    }
  };
  document.addEventListener('visibilitychange', onVisibility);

  const controller: DrawerFxController = {
    attach(c) {
      canvas = c;
      canvasId = ensureId(c);
      const w = Math.max(1, c.clientWidth || c.width || 600);
      const h = Math.max(1, c.clientHeight || c.height || 400);
      if (instance) { try { instance.free(); } catch { /* ignore */ } }
      try {
        instance = new mod.DrawerFx(canvasId, w, h);
      } catch (err) {
        // eslint-disable-next-line no-console
        console.warn('[drawer-fx] init error:', err);
        instance = null;
      }
    },
    resize(w, h) {
      if (!instance || !canvas) return;
      canvas.width = w;
      canvas.height = h;
      try { instance.resize(w, h); } catch { /* ignore */ }
    },
    start() {
      if (!instance || running) return;
      running = true;
      lastTs = 0;
      rafId = requestAnimationFrame(loop);
    },
    stop() {
      running = false;
      if (rafId) cancelAnimationFrame(rafId);
      rafId = 0;
    },
    pulse(x, y) {
      if (!instance) return;
      try { instance.pulse(x, y); } catch { /* ignore */ }
    },
    setQuality(q) {
      if (!instance) return;
      try { instance.set_quality(q); } catch { /* ignore */ }
    },
    destroy() {
      controller.stop();
      document.removeEventListener('visibilitychange', onVisibility);
      if (instance) { try { instance.free(); } catch { /* ignore */ } }
      instance = null;
      canvas = null;
    },
  };

  return controller;
}
