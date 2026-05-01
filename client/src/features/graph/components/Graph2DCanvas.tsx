import React, { useRef, useEffect } from 'react';
import { cn } from '../../../utils/classNameUtils';
import type { GraphData } from '../managers/graphDataManager';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface Graph2DCanvasProps {
  graphData?: GraphData;
  className?: string;
}

// ---------------------------------------------------------------------------
// Component — placeholder for the full 2D force-layout renderer
// ---------------------------------------------------------------------------

export const Graph2DCanvas: React.FC<Graph2DCanvasProps> = ({ className }) => {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const resize = () => {
      const dpr = window.devicePixelRatio || 1;
      const { clientWidth: w, clientHeight: h } = canvas;
      canvas.width = w * dpr;
      canvas.height = h * dpr;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      draw(ctx, w, h);
    };

    const draw = (c: CanvasRenderingContext2D, w: number, h: number) => {
      c.clearRect(0, 0, w, h);
      c.fillStyle = 'rgba(128, 128, 128, 0.15)';
      c.fillRect(0, 0, w, h);
      c.fillStyle = '#94a3b8';
      c.font = '14px system-ui, sans-serif';
      c.textAlign = 'center';
      c.textBaseline = 'middle';
      c.fillText('2D mode — coming soon', w / 2, h / 2);
    };

    resize();
    window.addEventListener('resize', resize);
    return () => window.removeEventListener('resize', resize);
  }, []);

  return (
    <canvas
      ref={canvasRef}
      className={cn('w-full h-full', className)}
    />
  );
};

export default Graph2DCanvas;
