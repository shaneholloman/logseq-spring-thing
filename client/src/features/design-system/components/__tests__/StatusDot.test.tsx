import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import React from 'react';
import { StatusDot } from '../StatusDot';

describe('StatusDot', () => {
  it('renders with active status', () => {
    const { container } = render(<StatusDot status="active" />);
    const dot = container.querySelector('[aria-hidden="true"]');
    expect(dot).toBeTruthy();
    expect(dot?.className).toContain('bg-emerald-500');
    expect(screen.getByText('Active')).toBeTruthy();
  });

  it('renders with warning status', () => {
    const { container } = render(<StatusDot status="warning" />);
    const dot = container.querySelector('[aria-hidden="true"]');
    expect(dot?.className).toContain('bg-amber-500');
    expect(screen.getByText('Warning')).toBeTruthy();
  });

  it('renders with error status', () => {
    const { container } = render(<StatusDot status="error" />);
    const dot = container.querySelector('[aria-hidden="true"]');
    expect(dot?.className).toContain('bg-red-500');
    expect(screen.getByText('Error')).toBeTruthy();
  });

  it('renders with inactive status', () => {
    const { container } = render(<StatusDot status="inactive" />);
    const dot = container.querySelector('[aria-hidden="true"]');
    expect(dot?.className).toContain('bg-gray-500');
    expect(screen.getByText('Inactive')).toBeTruthy();
  });

  it('renders with processing status', () => {
    const { container } = render(<StatusDot status="processing" />);
    const dot = container.querySelector('[aria-hidden="true"]');
    expect(dot?.className).toContain('bg-blue-500');
    expect(dot?.className).toContain('animate-pulse');
    expect(screen.getByText('Processing')).toBeTruthy();
  });

  it('renders custom label when provided', () => {
    render(<StatusDot status="active" label="Online" />);
    expect(screen.getByText('Online')).toBeTruthy();
    // Should use the custom label, not the default STATUS_TEXT
    expect(screen.queryByText('Active')).toBeNull();
  });

  it('renders default status text when no label provided', () => {
    render(<StatusDot status="active" />);
    expect(screen.getByText('Active')).toBeTruthy();
  });

  it('applies small size class', () => {
    const { container } = render(<StatusDot status="active" size="sm" />);
    const dot = container.querySelector('[aria-hidden="true"]');
    expect(dot?.className).toContain('h-1.5');
    expect(dot?.className).toContain('w-1.5');
  });

  it('applies medium size class (default)', () => {
    const { container } = render(<StatusDot status="active" size="md" />);
    const dot = container.querySelector('[aria-hidden="true"]');
    expect(dot?.className).toContain('h-2.5');
    expect(dot?.className).toContain('w-2.5');
  });

  it('applies large size class', () => {
    const { container } = render(<StatusDot status="active" size="lg" />);
    const dot = container.querySelector('[aria-hidden="true"]');
    expect(dot?.className).toContain('h-3.5');
    expect(dot?.className).toContain('w-3.5');
  });

  it('dot element has aria-hidden for accessibility', () => {
    const { container } = render(<StatusDot status="active" />);
    const dot = container.querySelector('[aria-hidden="true"]');
    expect(dot).toBeTruthy();
    expect(dot?.getAttribute('aria-hidden')).toBe('true');
  });

  it('applies default status and size when none specified', () => {
    const { container } = render(<StatusDot />);
    const dot = container.querySelector('[aria-hidden="true"]');
    // CVA defaultVariants apply CSS: status=inactive, size=md
    expect(dot?.className).toContain('bg-gray-500');
    expect(dot?.className).toContain('h-2.5');
    expect(dot?.className).toContain('w-2.5');
    // Text span exists but status prop is undefined so no STATUS_TEXT lookup match
    const textSpan = container.querySelector('.text-muted-foreground');
    expect(textSpan).toBeTruthy();
  });
});
