import React, { useRef, useEffect, useCallback } from 'react';

interface SparklineProps {
  data: number[];
  width?: number;
  height?: number;
  color?: string;
  fillColor?: string;
  strokeWidth?: number;
  animated?: boolean;
  className?: string;
  ariaLabel?: string;
}

export function Sparkline({
  data,
  width = 120,
  height = 40,
  color = '#10b981',
  fillColor,
  strokeWidth = 1.5,
  animated = true,
  className,
  ariaLabel,
}: SparklineProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const animationRef = useRef<number>(0);
  const progressRef = useRef(0);

  const draw = useCallback((progress: number) => {
    const canvas = canvasRef.current;
    if (!canvas || data.length < 2) return;

    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const dpr = window.devicePixelRatio || 1;
    canvas.width = width * dpr;
    canvas.height = height * dpr;
    ctx.scale(dpr, dpr);
    ctx.clearRect(0, 0, width, height);

    const min = Math.min(...data);
    const max = Math.max(...data);
    const range = max - min || 1;
    const padding = 2;
    const drawWidth = width - padding * 2;
    const drawHeight = height - padding * 2;

    const visibleCount = Math.ceil(data.length * progress);
    const points: [number, number][] = [];

    for (let i = 0; i < visibleCount; i++) {
      const x = padding + (i / (data.length - 1)) * drawWidth;
      const y = padding + drawHeight - ((data[i] - min) / range) * drawHeight;
      points.push([x, y]);
    }

    if (points.length < 2) return;

    // Fill gradient
    const gradientColor = fillColor || color;
    const gradient = ctx.createLinearGradient(0, 0, 0, height);
    gradient.addColorStop(0, gradientColor + '30');
    gradient.addColorStop(1, gradientColor + '05');

    ctx.beginPath();
    ctx.moveTo(points[0][0], height);
    for (const [x, y] of points) {
      ctx.lineTo(x, y);
    }
    ctx.lineTo(points[points.length - 1][0], height);
    ctx.closePath();
    ctx.fillStyle = gradient;
    ctx.fill();

    // Line
    ctx.beginPath();
    ctx.moveTo(points[0][0], points[0][1]);
    for (let i = 1; i < points.length; i++) {
      ctx.lineTo(points[i][0], points[i][1]);
    }
    ctx.strokeStyle = color;
    ctx.lineWidth = strokeWidth;
    ctx.lineJoin = 'round';
    ctx.lineCap = 'round';
    ctx.stroke();

    // End dot
    if (points.length > 0) {
      const [lastX, lastY] = points[points.length - 1];
      ctx.beginPath();
      ctx.arc(lastX, lastY, 2.5, 0, Math.PI * 2);
      ctx.fillStyle = color;
      ctx.fill();
      // Glow
      ctx.beginPath();
      ctx.arc(lastX, lastY, 5, 0, Math.PI * 2);
      ctx.fillStyle = color + '40';
      ctx.fill();
    }
  }, [data, width, height, color, fillColor, strokeWidth]);

  useEffect(() => {
    if (!animated) {
      draw(1);
      return;
    }

    progressRef.current = 0;
    const startTime = performance.now();
    const duration = 800;

    const animate = (now: number) => {
      const elapsed = now - startTime;
      progressRef.current = Math.min(elapsed / duration, 1);
      // Ease out cubic
      const eased = 1 - Math.pow(1 - progressRef.current, 3);
      draw(eased);

      if (progressRef.current < 1) {
        animationRef.current = requestAnimationFrame(animate);
      }
    };

    animationRef.current = requestAnimationFrame(animate);
    return () => cancelAnimationFrame(animationRef.current);
  }, [data, animated, draw]);

  return (
    <canvas
      ref={canvasRef}
      width={width}
      height={height}
      className={className}
      style={{ width, height }}
      role="img"
      aria-label={ariaLabel || `Trend chart with ${data.length} data points`}
    />
  );
}
