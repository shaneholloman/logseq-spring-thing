/**
 * WasmMiniGraph — WASM-accelerated mini-graph thumbnail (ADR-047).
 *
 * Used by:
 *   - Decision Canvas skill preview cards (broker workbench, ADR-041)
 *   - `/studio/:workspaceId` embedded work-lane graph (BC18 surface)
 *
 * Pattern mirrors the existing `scene-effects-bridge.ts` progressive-enhancement
 * flow: a Canvas2D baseline renders immediately; the WASM renderer is loaded
 * asynchronously and, if available, upgrades the canvas to a higher-fidelity
 * view using the zero-copy Float32Array bridge.
 *
 * Inputs follow the stride-7 node layout defined by the WASM crate
 * (`[x, y, r, g, b, a, weight]`) plus a flat edge list.
 */

import React, { useEffect, useMemo, useRef, useState } from 'react';
import {
  initSceneEffects,
  MINI_GRAPH_NODE_STRIDE,
  type MiniGraphBridge,
} from '../../../wasm/scene-effects-bridge';

/** A single node on the mini-graph. Colours are 0..1; NDC coords in [-1, 1]. */
export interface MiniGraphNode {
  x: number;
  y: number;
  r: number;
  g: number;
  b: number;
  a: number;
  weight: number;
}

/** Undirected edge expressed as a pair of node indices into `nodes`. */
export interface MiniGraphEdge {
  from: number;
  to: number;
}

export interface WasmMiniGraphProps {
  nodes: MiniGraphNode[];
  edges: MiniGraphEdge[];
  width?: number;
  height?: number;
  className?: string;
  ariaLabel?: string;
  /** Disable WASM and always use the Canvas2D fallback. */
  forceFallback?: boolean;
}

/**
 * Pack the node array into a stride-7 Float32Array in one allocation. This
 * buffer is shipped zero-copy into WASM linear memory via `byteOffset`.
 */
function packNodes(nodes: MiniGraphNode[]): Float32Array {
  const buf = new Float32Array(nodes.length * MINI_GRAPH_NODE_STRIDE);
  for (let i = 0; i < nodes.length; i++) {
    const n = nodes[i];
    const base = i * MINI_GRAPH_NODE_STRIDE;
    buf[base] = n.x;
    buf[base + 1] = n.y;
    buf[base + 2] = n.r;
    buf[base + 3] = n.g;
    buf[base + 4] = n.b;
    buf[base + 5] = n.a;
    buf[base + 6] = n.weight;
  }
  return buf;
}

/** Pack edges into a flat Uint32Array. */
function packEdges(edges: MiniGraphEdge[]): Uint32Array {
  const buf = new Uint32Array(edges.length * 2);
  for (let i = 0; i < edges.length; i++) {
    buf[i * 2] = edges[i].from >>> 0;
    buf[i * 2 + 1] = edges[i].to >>> 0;
  }
  return buf;
}

/** Canvas2D fallback renderer. Produces an approximation of the WASM output. */
function drawFallback(
  canvas: HTMLCanvasElement,
  nodes: MiniGraphNode[],
  edges: MiniGraphEdge[],
  width: number,
  height: number,
): void {
  const ctx = canvas.getContext('2d');
  if (!ctx) return;
  const dpr = window.devicePixelRatio || 1;
  canvas.width = Math.round(width * dpr);
  canvas.height = Math.round(height * dpr);
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, width, height);

  const inset = 6;
  const map = (x: number, y: number): [number, number] => {
    const cx = Math.max(-1, Math.min(1, x));
    const cy = Math.max(-1, Math.min(1, y));
    return [
      inset + ((cx + 1) * 0.5) * (width - inset * 2),
      inset + ((cy + 1) * 0.5) * (height - inset * 2),
    ];
  };

  // Edges.
  ctx.lineCap = 'round';
  for (const e of edges) {
    const from = nodes[e.from];
    const to = nodes[e.to];
    if (!from || !to) continue;
    const [fx, fy] = map(from.x, from.y);
    const [tx, ty] = map(to.x, to.y);
    const w = Math.min(from.weight, to.weight);
    ctx.strokeStyle = `rgba(${Math.round(((from.r + to.r) / 2) * 255)},${Math.round(((from.g + to.g) / 2) * 255)},${Math.round(((from.b + to.b) / 2) * 255)},${0.2 + w * 0.3})`;
    ctx.lineWidth = 1 + w * 0.8;
    ctx.beginPath();
    ctx.moveTo(fx, fy);
    ctx.lineTo(tx, ty);
    ctx.stroke();
  }

  // Nodes.
  for (const n of nodes) {
    const [px, py] = map(n.x, n.y);
    const core = 2 + n.weight * 2;
    const halo = 4.5 + n.weight * 5;
    const fillHalo = `rgba(${Math.round(n.r * 255)},${Math.round(n.g * 255)},${Math.round(n.b * 255)},0.25)`;
    const fillCore = `rgba(${Math.round(n.r * 255)},${Math.round(n.g * 255)},${Math.round(n.b * 255)},${Math.max(0.5, n.a)})`;
    ctx.beginPath();
    ctx.arc(px, py, halo, 0, Math.PI * 2);
    ctx.fillStyle = fillHalo;
    ctx.fill();
    ctx.beginPath();
    ctx.arc(px, py, core, 0, Math.PI * 2);
    ctx.fillStyle = fillCore;
    ctx.fill();
  }
}

/**
 * Paint the WASM-produced RGBA8 buffer into the canvas. The buffer is a
 * zero-copy `Uint8Array` view over WASM memory; we copy into `ImageData`
 * (which requires a `Uint8ClampedArray`) to hand off to Canvas2D.
 */
function paintPixels(
  canvas: HTMLCanvasElement,
  pixels: Uint8Array,
  width: number,
  height: number,
): void {
  const ctx = canvas.getContext('2d');
  if (!ctx) return;
  const dpr = window.devicePixelRatio || 1;
  canvas.width = Math.round(width * dpr);
  canvas.height = Math.round(height * dpr);
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.clearRect(0, 0, canvas.width, canvas.height);

  // The WASM renderer renders at the CSS pixel grid; we scale up with nearest
  // neighbour for DPR > 1 to avoid a second (costly) WASM render at 2x.
  const clamped = new Uint8ClampedArray(pixels.length);
  clamped.set(pixels);
  const image = new ImageData(clamped, width, height);
  if (dpr === 1) {
    ctx.putImageData(image, 0, 0);
    return;
  }
  const tmp = document.createElement('canvas');
  tmp.width = width;
  tmp.height = height;
  const tmpCtx = tmp.getContext('2d');
  if (!tmpCtx) {
    ctx.putImageData(image, 0, 0);
    return;
  }
  tmpCtx.putImageData(image, 0, 0);
  ctx.imageSmoothingEnabled = false;
  ctx.drawImage(tmp, 0, 0, width, height, 0, 0, canvas.width, canvas.height);
}

/**
 * React wrapper around the WASM mini-graph renderer.
 *
 * On mount:
 *   1. Immediately paints the Canvas2D baseline using the provided nodes/edges.
 *   2. Asynchronously loads the scene-effects WASM module.
 *   3. If loaded, instantiates a `MiniGraphBridge` and re-renders via WASM.
 *   4. If loading fails, the Canvas2D baseline remains.
 *
 * On unmount or when dimensions change, the bridge is disposed to release
 * WASM resources.
 */
export function WasmMiniGraph({
  nodes,
  edges,
  width = 160,
  height = 96,
  className,
  ariaLabel,
  forceFallback = false,
}: WasmMiniGraphProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const bridgeRef = useRef<MiniGraphBridge | null>(null);
  const [wasmReady, setWasmReady] = useState(false);

  const packedNodes = useMemo(() => packNodes(nodes), [nodes]);
  const packedEdges = useMemo(() => packEdges(edges), [edges]);

  // Try to upgrade to WASM once per (dimension, forceFallback) tuple.
  useEffect(() => {
    if (forceFallback) return;
    let cancelled = false;
    initSceneEffects()
      .then((api) => {
        if (cancelled) return;
        const bridge = api.createMiniGraph(width, height);
        bridgeRef.current = bridge;
        setWasmReady(true);
      })
      .catch(() => {
        // Canvas2D baseline continues; swallow WASM init failure.
      });
    return () => {
      cancelled = true;
      bridgeRef.current?.dispose();
      bridgeRef.current = null;
      setWasmReady(false);
    };
  }, [width, height, forceFallback]);

  // Render: WASM if available, else Canvas2D.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const bridge = bridgeRef.current;
    if (wasmReady && bridge && !bridge.isDisposed) {
      try {
        bridge.render(packedNodes, packedEdges);
        paintPixels(canvas, bridge.getPixels(), width, height);
        return;
      } catch {
        // Fall through to Canvas2D on any WASM-side error.
      }
    }
    drawFallback(canvas, nodes, edges, width, height);
  }, [packedNodes, packedEdges, wasmReady, nodes, edges, width, height]);

  return (
    <canvas
      ref={canvasRef}
      width={width}
      height={height}
      className={className}
      style={{ width, height }}
      role="img"
      aria-label={
        ariaLabel ||
        `Mini graph with ${nodes.length} nodes and ${edges.length} edges`
      }
    />
  );
}

export default WasmMiniGraph;
