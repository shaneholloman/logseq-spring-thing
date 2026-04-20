/**
 * OntologyNeighborThumb — WASM-accelerated ontology-neighbourhood thumbnail.
 *
 * Surfaced in Sensei nudge cards (ADR-047 extension). Given a focus ontology
 * node and up to ~8 related neighbours, renders a small radial thumbnail: a
 * central glowing dot with neighbour dots connected by tinted radial lines.
 *
 * The input layout (`OntologyNeighbor[]` with the focus as index 0) is packed
 * into a single stride-7 `Float32Array` and shipped zero-copy into the WASM
 * module through the `scene-effects-bridge`.
 */

import React, { useEffect, useMemo, useRef, useState } from 'react';
import {
  initSceneEffects,
  MINI_GRAPH_NODE_STRIDE,
  type OntologyNeighborThumbBridge,
} from '../../../wasm/scene-effects-bridge';

export interface OntologyNeighbor {
  /** x in [-1, 1] NDC. Focus is normally at (0, 0). */
  x: number;
  /** y in [-1, 1] NDC. */
  y: number;
  /** Colour channels in 0..1. */
  r: number;
  g: number;
  b: number;
  /** Alpha in 0..1. */
  a: number;
  /** Edge/node weight in 0..1 driving stroke alpha and dot radius. */
  weight: number;
}

export interface OntologyNeighborThumbProps {
  /** Focus is neighbors[0]; neighbours are neighbors[1..]. */
  neighbors: OntologyNeighbor[];
  width?: number;
  height?: number;
  className?: string;
  ariaLabel?: string;
  /** Force the Canvas2D fallback (useful for tests / degraded environments). */
  forceFallback?: boolean;
}

function packNeighbors(neighbors: OntologyNeighbor[]): Float32Array {
  const buf = new Float32Array(neighbors.length * MINI_GRAPH_NODE_STRIDE);
  for (let i = 0; i < neighbors.length; i++) {
    const n = neighbors[i];
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

function drawFallback(
  canvas: HTMLCanvasElement,
  neighbors: OntologyNeighbor[],
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
  if (neighbors.length === 0) return;

  const inset = 4;
  const map = (x: number, y: number): [number, number] => {
    const cx = Math.max(-1, Math.min(1, x));
    const cy = Math.max(-1, Math.min(1, y));
    return [
      inset + ((cx + 1) * 0.5) * (width - inset * 2),
      inset + ((cy + 1) * 0.5) * (height - inset * 2),
    ];
  };

  const focus = neighbors[0];
  const [fpx, fpy] = map(focus.x, focus.y);

  // Radial edges.
  ctx.lineCap = 'round';
  for (let i = 1; i < neighbors.length; i++) {
    const n = neighbors[i];
    const [npx, npy] = map(n.x, n.y);
    ctx.strokeStyle = `rgba(${Math.round(n.r * 255)},${Math.round(n.g * 255)},${Math.round(n.b * 255)},${0.25 + n.weight * 0.35})`;
    ctx.lineWidth = 0.8 + n.weight * 1.2;
    ctx.beginPath();
    ctx.moveTo(fpx, fpy);
    ctx.lineTo(npx, npy);
    ctx.stroke();
  }

  // Neighbour dots.
  for (let i = 1; i < neighbors.length; i++) {
    const n = neighbors[i];
    const [npx, npy] = map(n.x, n.y);
    const r = 2.5 + n.weight * 3.5;
    ctx.beginPath();
    ctx.arc(npx, npy, r, 0, Math.PI * 2);
    ctx.fillStyle = `rgba(${Math.round(n.r * 255)},${Math.round(n.g * 255)},${Math.round(n.b * 255)},${Math.max(0.4, n.a)})`;
    ctx.fill();
  }

  // Focus dot + halo.
  const fr = Math.max(0.7, focus.r);
  const fg = Math.max(0.8, focus.g);
  const fb = Math.max(0.95, focus.b);
  ctx.beginPath();
  ctx.arc(fpx, fpy, 7, 0, Math.PI * 2);
  ctx.fillStyle = `rgba(${Math.round(fr * 255)},${Math.round(fg * 255)},${Math.round(fb * 255)},0.35)`;
  ctx.fill();
  ctx.beginPath();
  ctx.arc(fpx, fpy, 3.5, 0, Math.PI * 2);
  ctx.fillStyle = `rgba(${Math.round(fr * 255)},${Math.round(fg * 255)},${Math.round(fb * 255)},1.0)`;
  ctx.fill();
}

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
 * React wrapper around the WASM ontology-neighbour thumbnail renderer.
 *
 * Progressive enhancement: Canvas2D baseline renders immediately, WASM
 * upgrades the thumbnail once the module loads. Failure to load leaves the
 * baseline in place (no user-facing error).
 */
export function OntologyNeighborThumb({
  neighbors,
  width = 72,
  height = 72,
  className,
  ariaLabel,
  forceFallback = false,
}: OntologyNeighborThumbProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const bridgeRef = useRef<OntologyNeighborThumbBridge | null>(null);
  const [wasmReady, setWasmReady] = useState(false);

  const packed = useMemo(() => packNeighbors(neighbors), [neighbors]);

  useEffect(() => {
    if (forceFallback) return;
    let cancelled = false;
    initSceneEffects()
      .then((api) => {
        if (cancelled) return;
        const bridge = api.createOntologyNeighborThumb(width, height);
        bridgeRef.current = bridge;
        setWasmReady(true);
      })
      .catch(() => {
        // Baseline continues.
      });
    return () => {
      cancelled = true;
      bridgeRef.current?.dispose();
      bridgeRef.current = null;
      setWasmReady(false);
    };
  }, [width, height, forceFallback]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const bridge = bridgeRef.current;
    if (wasmReady && bridge && !bridge.isDisposed) {
      try {
        bridge.render(packed);
        paintPixels(canvas, bridge.getPixels(), width, height);
        return;
      } catch {
        // Fall through.
      }
    }
    drawFallback(canvas, neighbors, width, height);
  }, [packed, wasmReady, neighbors, width, height]);

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
        `Ontology neighbourhood thumbnail with ${Math.max(0, neighbors.length - 1)} related terms`
      }
    />
  );
}

export default OntologyNeighborThumb;
