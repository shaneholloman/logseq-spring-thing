/**
 * useDrawerFx
 *
 * React hook that wires a canvas ref to the drawer-fx WASM effect,
 * respecting `prefers-reduced-motion`, pausing in hidden tabs, and
 * throttling mouse-move pulses to the display refresh.
 *
 *   const ref = useRef<HTMLCanvasElement>(null);
 *   useDrawerFx(isDrawerOpen, ref);
 */

import { useEffect, type RefObject } from 'react';
import { loadDrawerFx, type DrawerFxController } from './drawerFx';

export interface UseDrawerFxOptions {
  quality?: 0 | 1 | 2;
  /** DPR cap -- 2.0 covers retina without blowing up GPU upload cost. */
  dprCap?: number;
}

export function useDrawerFx(
  active: boolean,
  ref: RefObject<HTMLCanvasElement | null>,
  opts: UseDrawerFxOptions = {}
) {
  const { quality = 1, dprCap = 2 } = opts;

  useEffect(() => {
    if (!active) return;
    const canvas = ref.current;
    if (!canvas) return;

    // Reduced motion -> bail immediately; caller should render a static
    // CSS gradient fallback.
    const mq = window.matchMedia('(prefers-reduced-motion: reduce)');
    if (mq.matches) return;

    let ctrl: DrawerFxController | null = null;
    let destroyed = false;
    let ro: ResizeObserver | null = null;
    let lastPulse = 0;

    const dpr = Math.min(dprCap, window.devicePixelRatio || 1);

    const applySize = () => {
      if (!canvas || !ctrl) return;
      const w = Math.max(1, Math.floor(canvas.clientWidth * dpr));
      const h = Math.max(1, Math.floor(canvas.clientHeight * dpr));
      ctrl.resize(w, h);
    };

    const onMove = (e: MouseEvent) => {
      if (!ctrl) return;
      const now = performance.now();
      // ~60 Hz throttle (16ms). Pulse() itself is cheap but we cap the
      // FFI round-trip regardless.
      if (now - lastPulse < 16) return;
      lastPulse = now;
      const rect = canvas.getBoundingClientRect();
      const x = (e.clientX - rect.left) * dpr;
      const y = (e.clientY - rect.top) * dpr;
      ctrl.pulse(x, y);
    };

    (async () => {
      ctrl = await loadDrawerFx();
      if (destroyed || !ctrl || !canvas) return;
      // Set initial size before attach so WASM sees sensible dims.
      canvas.width = Math.max(1, Math.floor(canvas.clientWidth * dpr));
      canvas.height = Math.max(1, Math.floor(canvas.clientHeight * dpr));
      ctrl.attach(canvas);
      ctrl.setQuality(quality);
      ctrl.start();

      ro = new ResizeObserver(() => applySize());
      ro.observe(canvas);
      canvas.addEventListener('mousemove', onMove);
    })();

    return () => {
      destroyed = true;
      canvas.removeEventListener('mousemove', onMove);
      if (ro) ro.disconnect();
      if (ctrl) ctrl.destroy();
    };
  }, [active, ref, quality, dprCap]);
}
