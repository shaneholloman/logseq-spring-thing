import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, act } from '@testing-library/react';
import React from 'react';
import { Sparkline } from '../Sparkline';

// Build a mock 2d context that tracks calls
function createMockContext2D() {
  return {
    scale: vi.fn(),
    clearRect: vi.fn(),
    beginPath: vi.fn(),
    moveTo: vi.fn(),
    lineTo: vi.fn(),
    closePath: vi.fn(),
    fill: vi.fn(),
    stroke: vi.fn(),
    arc: vi.fn(),
    createLinearGradient: vi.fn(() => ({
      addColorStop: vi.fn(),
    })),
    set fillStyle(_v: unknown) { /* noop setter */ },
    get fillStyle() { return ''; },
    set strokeStyle(_v: unknown) { /* noop setter */ },
    get strokeStyle() { return ''; },
    lineWidth: 0,
    lineJoin: '',
    lineCap: '',
  };
}

describe('Sparkline', () => {
  let mockCtx: ReturnType<typeof createMockContext2D>;
  let originalGetContext: typeof HTMLCanvasElement.prototype.getContext;

  beforeEach(() => {
    mockCtx = createMockContext2D();
    // Save the setupTests mock so we can restore it
    originalGetContext = HTMLCanvasElement.prototype.getContext;
    // Override getContext to return our 2d mock while preserving webgl behavior
    HTMLCanvasElement.prototype.getContext = vi.fn((contextType: string) => {
      if (contextType === '2d') return mockCtx;
      return null;
    }) as unknown as typeof HTMLCanvasElement.prototype.getContext;
  });

  afterEach(() => {
    HTMLCanvasElement.prototype.getContext = originalGetContext;
    vi.restoreAllMocks();
  });

  it('renders a canvas element', () => {
    const { container } = render(<Sparkline data={[1, 2, 3]} />);
    const canvas = container.querySelector('canvas');
    expect(canvas).toBeTruthy();
  });

  it('renders canvas with correct default dimensions', () => {
    const { container } = render(<Sparkline data={[1, 2, 3]} />);
    const canvas = container.querySelector('canvas') as HTMLCanvasElement;
    expect(canvas.style.width).toBe('120px');
    expect(canvas.style.height).toBe('40px');
  });

  it('respects custom width and height props', () => {
    const { container } = render(<Sparkline data={[1, 2, 3]} width={200} height={80} />);
    const canvas = container.querySelector('canvas') as HTMLCanvasElement;
    expect(canvas.style.width).toBe('200px');
    expect(canvas.style.height).toBe('80px');
    expect(canvas.getAttribute('width')).toBe('200');
    expect(canvas.getAttribute('height')).toBe('80');
  });

  it('handles empty data array without crashing', () => {
    const { container } = render(<Sparkline data={[]} />);
    const canvas = container.querySelector('canvas');
    expect(canvas).toBeTruthy();
    // draw() returns early for data.length < 2, so no stroke calls
    expect(mockCtx.stroke).not.toHaveBeenCalled();
  });

  it('handles single data point without drawing', () => {
    render(<Sparkline data={[42]} />);
    // data.length < 2 means draw() returns early
    expect(mockCtx.stroke).not.toHaveBeenCalled();
  });

  it('calls canvas context methods when given valid data', () => {
    render(<Sparkline data={[10, 20, 30, 40]} animated={false} />);
    // With animated=false, draw(1) is called synchronously
    expect(mockCtx.scale).toHaveBeenCalled();
    expect(mockCtx.clearRect).toHaveBeenCalled();
    expect(mockCtx.beginPath).toHaveBeenCalled();
    expect(mockCtx.stroke).toHaveBeenCalled();
    expect(mockCtx.fill).toHaveBeenCalled();
    expect(mockCtx.createLinearGradient).toHaveBeenCalled();
  });

  it('requests animation frame when animated=true', () => {
    const rafSpy = vi.spyOn(global, 'requestAnimationFrame');
    render(<Sparkline data={[1, 2, 3, 4]} animated={true} />);
    expect(rafSpy).toHaveBeenCalled();
  });

  it('does not request animation frame when animated=false', () => {
    const rafSpy = vi.spyOn(global, 'requestAnimationFrame');
    rafSpy.mockClear();
    render(<Sparkline data={[1, 2, 3, 4]} animated={false} />);
    expect(rafSpy).not.toHaveBeenCalled();
  });

  it('cancels animation frame on unmount', () => {
    const cafSpy = vi.spyOn(global, 'cancelAnimationFrame');
    const { unmount } = render(<Sparkline data={[1, 2, 3, 4]} animated={true} />);
    unmount();
    expect(cafSpy).toHaveBeenCalled();
  });

  it('applies custom className', () => {
    const { container } = render(<Sparkline data={[1, 2]} className="my-sparkline" />);
    const canvas = container.querySelector('canvas');
    expect(canvas?.className).toContain('my-sparkline');
  });
});
