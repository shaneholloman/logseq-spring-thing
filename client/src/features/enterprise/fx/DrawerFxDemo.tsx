/**
 * DrawerFxDemo
 *
 * Full-viewport preview of the drawer-fx WASM effect.
 * Mount via the hash route `#/fx-demo` (see enterprise-standalone.tsx).
 */

import React, { useRef, useState } from 'react';
import { useDrawerFx } from './useDrawerFx';

export function DrawerFxDemo() {
  const ref = useRef<HTMLCanvasElement>(null);
  const [quality, setQuality] = useState<0 | 1 | 2>(1);
  const [active, setActive] = useState(true);

  useDrawerFx(active, ref, { quality });

  const reduced =
    typeof window !== 'undefined' &&
    window.matchMedia('(prefers-reduced-motion: reduce)').matches;

  return (
    <div
      style={{
        position: 'fixed',
        inset: 0,
        // CSS fallback -- visible even if WASM fails or reduced-motion is on.
        background:
          'radial-gradient(ellipse at 30% 40%, #2a1b4a 0%, #0a0816 55%, #05040c 100%)',
        overflow: 'hidden',
      }}
    >
      <canvas
        ref={ref}
        style={{
          position: 'absolute',
          inset: 0,
          width: '100%',
          height: '100%',
          // Frosted-glass simulation: the effect sits *behind* a translucent
          // surface in the real drawer. Here we expose it directly.
          filter: 'saturate(1.05)',
        }}
      />
      <div
        style={{
          position: 'absolute',
          top: 20,
          left: 20,
          padding: '12px 16px',
          borderRadius: 12,
          background: 'rgba(15, 12, 28, 0.55)',
          backdropFilter: 'blur(14px)',
          WebkitBackdropFilter: 'blur(14px)',
          color: '#e6e0ff',
          fontFamily: 'ui-sans-serif, system-ui, sans-serif',
          fontSize: 13,
          lineHeight: 1.5,
          border: '1px solid rgba(168, 135, 255, 0.25)',
          userSelect: 'none',
        }}
      >
        <div style={{ fontWeight: 600, marginBottom: 6 }}>drawer-fx demo</div>
        <div style={{ opacity: 0.85 }}>Mouse-move = pulse</div>
        <div style={{ marginTop: 10, display: 'flex', gap: 8 }}>
          {[0, 1, 2].map((q) => (
            <button
              key={q}
              onClick={() => setQuality(q as 0 | 1 | 2)}
              style={{
                padding: '4px 10px',
                borderRadius: 6,
                border: '1px solid rgba(168, 135, 255, 0.35)',
                background:
                  quality === q
                    ? 'rgba(168, 135, 255, 0.25)'
                    : 'transparent',
                color: '#e6e0ff',
                cursor: 'pointer',
                fontSize: 12,
              }}
            >
              Q{q}
            </button>
          ))}
          <button
            onClick={() => setActive((a) => !a)}
            style={{
              padding: '4px 10px',
              borderRadius: 6,
              border: '1px solid rgba(168, 135, 255, 0.35)',
              background: 'transparent',
              color: '#e6e0ff',
              cursor: 'pointer',
              fontSize: 12,
            }}
          >
            {active ? 'Stop' : 'Start'}
          </button>
        </div>
        {reduced && (
          <div style={{ marginTop: 8, color: '#ffd5a8', fontSize: 11 }}>
            prefers-reduced-motion: effect disabled, CSS fallback shown
          </div>
        )}
      </div>
    </div>
  );
}

export default DrawerFxDemo;
