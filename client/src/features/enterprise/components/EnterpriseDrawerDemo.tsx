import React from 'react';
import { EnterpriseDrawer } from './EnterpriseDrawer';
import { EnterpriseDrawerToggle } from './EnterpriseDrawerToggle';
import { useDrawerStore } from '../store/drawerStore';

/**
 * Eyeball harness for the drawer. Renders a big SVG pseudo-graph so we can
 * assess the frosted-glass blend against a busy background. Hash route:
 * `#/drawer-demo` (wire-up lives wherever the app's hash router is; this
 * component simply exports a default page the router can mount).
 */
const NODE_COUNT = 120;

function pseudoRandom(seed: number): number {
  // deterministic — we don't want the demo flickering on every render.
  const x = Math.sin(seed) * 10_000;
  return x - Math.floor(x);
}

function FakeGraph() {
  const nodes = Array.from({ length: NODE_COUNT }, (_, i) => {
    const cx = pseudoRandom(i + 1) * 1600;
    const cy = pseudoRandom(i + 101) * 900;
    const r = 3 + pseudoRandom(i + 201) * 9;
    const hue = Math.floor(pseudoRandom(i + 301) * 360);
    return { cx, cy, r, hue, id: i };
  });
  const edges = Array.from({ length: 180 }, (_, i) => {
    const a = Math.floor(pseudoRandom(i + 401) * NODE_COUNT);
    const b = Math.floor(pseudoRandom(i + 501) * NODE_COUNT);
    return { a, b, id: i };
  });

  return (
    <svg
      viewBox="0 0 1600 900"
      preserveAspectRatio="xMidYMid slice"
      className="absolute inset-0 h-full w-full"
      aria-hidden="true"
    >
      <defs>
        <radialGradient id="bg-demo" cx="50%" cy="50%" r="75%">
          <stop offset="0%" stopColor="hsl(220 70% 18%)" />
          <stop offset="100%" stopColor="hsl(230 60% 6%)" />
        </radialGradient>
      </defs>
      <rect width="1600" height="900" fill="url(#bg-demo)" />
      {edges.map((e) => {
        const n1 = nodes[e.a];
        const n2 = nodes[e.b];
        return (
          <line
            key={e.id}
            x1={n1.cx}
            y1={n1.cy}
            x2={n2.cx}
            y2={n2.cy}
            stroke="hsl(190 60% 55% / 0.18)"
            strokeWidth={0.8}
          />
        );
      })}
      {nodes.map((n) => (
        <circle
          key={n.id}
          cx={n.cx}
          cy={n.cy}
          r={n.r}
          fill={`hsl(${n.hue} 80% 60% / 0.85)`}
        />
      ))}
    </svg>
  );
}

export function EnterpriseDrawerDemo() {
  const { open, openDrawer, closeDrawer, toggleDrawer } = useDrawerStore();

  return (
    <div className="relative h-screen w-screen overflow-hidden bg-background text-foreground">
      <FakeGraph />

      {/* Floating header to host the toggle */}
      <header className="absolute top-0 inset-x-0 z-30 flex items-center justify-between px-4 py-3 bg-card/40 backdrop-blur-sm border-b border-border/40">
        <h1 className="text-sm font-semibold tracking-tight">
          VisionFlow Drawer Demo
        </h1>
        <EnterpriseDrawerToggle open={open} onToggle={toggleDrawer} />
      </header>

      <EnterpriseDrawer open={open} onClose={closeDrawer} title="Enterprise">
        <div className="space-y-4">
          <section>
            <h3 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground mb-2">
              Demo content
            </h3>
            <p className="text-sm text-foreground/90">
              This is a prototype harness for the frosted-glass workspace drawer.
              Scroll to verify the body scrolls independently of the header,
              press <kbd className="rounded bg-muted px-1.5 py-0.5 text-xs">Esc</kbd>{' '}
              to close, and click outside to confirm it does NOT dismiss.
            </p>
          </section>
          {Array.from({ length: 40 }).map((_, i) => (
            <div
              key={i}
              className="rounded-md border border-border/40 bg-card/40 px-3 py-2 text-sm text-foreground/80"
            >
              Scroll probe row #{i + 1}
            </div>
          ))}
          <button
            type="button"
            onClick={() => openDrawer('broker')}
            className="rounded-md bg-primary/10 px-3 py-2 text-sm text-primary hover:bg-primary/20"
          >
            Persist activeSection = broker
          </button>
        </div>
      </EnterpriseDrawer>
    </div>
  );
}

export default EnterpriseDrawerDemo;
