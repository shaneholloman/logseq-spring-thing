import React, { useRef, useState, useEffect, useCallback } from 'react';
import { cn } from '../../../utils/classNameUtils';

interface ScrollAreaProps {
  children: React.ReactNode;
  className?: string;
  style?: React.CSSProperties;
  orientation?: 'vertical' | 'horizontal' | 'both';
  scrollbarSize?: 'thin' | 'default';
}

/**
 * ScrollArea — CSS-based custom scrollbar component with hover-reveal behavior.
 * Provides styled, thin scrollbars consistent with the design system tokens.
 */
export const ScrollArea: React.FC<ScrollAreaProps> = ({
  children,
  className = '',
  style = {},
  orientation = 'vertical',
  scrollbarSize = 'thin',
}) => {
  const viewportRef = useRef<HTMLDivElement>(null);
  const thumbRef = useRef<HTMLDivElement>(null);
  const trackRef = useRef<HTMLDivElement>(null);
  const [thumbHeight, setThumbHeight] = useState(0);
  const [thumbTop, setThumbTop] = useState(0);
  const [isScrollable, setIsScrollable] = useState(false);
  const [isHovered, setIsHovered] = useState(false);
  const [isDragging, setIsDragging] = useState(false);
  const dragStartRef = useRef({ y: 0, scrollTop: 0 });

  const updateThumb = useCallback(() => {
    const viewport = viewportRef.current;
    if (!viewport) return;

    const { scrollHeight, clientHeight, scrollTop } = viewport;
    const scrollable = scrollHeight > clientHeight;
    setIsScrollable(scrollable);

    if (!scrollable) return;

    const ratio = clientHeight / scrollHeight;
    const height = Math.max(ratio * clientHeight, 24);
    const maxTop = clientHeight - height;
    const top = (scrollTop / (scrollHeight - clientHeight)) * maxTop;

    setThumbHeight(height);
    setThumbTop(top);
  }, []);

  useEffect(() => {
    const viewport = viewportRef.current;
    if (!viewport) return;

    updateThumb();

    const observer = new ResizeObserver(updateThumb);
    observer.observe(viewport);
    if (viewport.firstElementChild) {
      observer.observe(viewport.firstElementChild);
    }

    return () => observer.disconnect();
  }, [updateThumb]);

  const handleScroll = useCallback(() => {
    updateThumb();
  }, [updateThumb]);

  const handleTrackClick = useCallback((e: React.MouseEvent) => {
    const viewport = viewportRef.current;
    const track = trackRef.current;
    if (!viewport || !track) return;

    const rect = track.getBoundingClientRect();
    const clickRatio = (e.clientY - rect.top) / rect.height;
    viewport.scrollTop = clickRatio * (viewport.scrollHeight - viewport.clientHeight);
  }, []);

  const handleThumbMouseDown = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    const viewport = viewportRef.current;
    if (!viewport) return;

    setIsDragging(true);
    dragStartRef.current = { y: e.clientY, scrollTop: viewport.scrollTop };

    const handleMouseMove = (ev: MouseEvent) => {
      const viewport = viewportRef.current;
      if (!viewport) return;
      const delta = ev.clientY - dragStartRef.current.y;
      const scrollRatio = viewport.scrollHeight / viewport.clientHeight;
      viewport.scrollTop = dragStartRef.current.scrollTop + delta * scrollRatio;
    };

    const handleMouseUp = () => {
      setIsDragging(false);
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
    };

    document.addEventListener('mousemove', handleMouseMove);
    document.addEventListener('mouseup', handleMouseUp);
  }, []);

  const overflowClass =
    orientation === 'horizontal'
      ? 'overflow-x-auto overflow-y-hidden'
      : orientation === 'both'
        ? 'overflow-auto'
        : 'overflow-y-auto overflow-x-hidden';

  const trackWidth = scrollbarSize === 'thin' ? 'w-1.5' : 'w-2';

  return (
    <div
      className={cn('relative group', className)}
      style={style}
      onMouseEnter={() => setIsHovered(true)}
      onMouseLeave={() => setIsHovered(false)}
    >
      {/* Viewport */}
      <div
        ref={viewportRef}
        className={cn(overflowClass, 'h-full w-full scrollbar-none')}
        style={{ scrollbarWidth: 'none' }}
        onScroll={handleScroll}
      >
        {children}
      </div>

      {/* Custom scrollbar track (vertical) */}
      {(orientation === 'vertical' || orientation === 'both') && isScrollable && (
        <div
          ref={trackRef}
          className={cn(
            'absolute right-0 top-0 bottom-0 transition-opacity duration-200',
            trackWidth,
            isHovered || isDragging ? 'opacity-100' : 'opacity-0'
          )}
          onClick={handleTrackClick}
        >
          <div
            ref={thumbRef}
            className={cn(
              'absolute right-0 rounded-full bg-muted-foreground/40 transition-colors duration-150 hover:bg-muted-foreground/60',
              trackWidth,
              isDragging && 'bg-muted-foreground/60'
            )}
            style={{
              height: `${thumbHeight}px`,
              transform: `translateY(${thumbTop}px)`,
            }}
            onMouseDown={handleThumbMouseDown}
          />
        </div>
      )}
    </div>
  );
};

export default ScrollArea;
