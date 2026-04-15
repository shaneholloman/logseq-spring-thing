import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import React from 'react';
import { StatusDot } from '../StatusDot';

describe('StatusDot', () => {
  it('renders with active status', () => {
    render(<StatusDot status="active" />);
    const dot = screen.getByRole('status');
    expect(dot).toBeTruthy();
    expect(dot.className).toContain('bg-emerald-500');
  });

  it('renders with warning status', () => {
    render(<StatusDot status="warning" />);
    const dot = screen.getByRole('status');
    expect(dot.className).toContain('bg-amber-500');
  });

  it('renders with error status', () => {
    render(<StatusDot status="error" />);
    const dot = screen.getByRole('status');
    expect(dot.className).toContain('bg-red-500');
  });

  it('renders with inactive status', () => {
    render(<StatusDot status="inactive" />);
    const dot = screen.getByRole('status');
    expect(dot.className).toContain('bg-gray-500');
  });

  it('renders with processing status', () => {
    render(<StatusDot status="processing" />);
    const dot = screen.getByRole('status');
    expect(dot.className).toContain('bg-blue-500');
    expect(dot.className).toContain('animate-pulse');
  });

  it('renders label text when provided', () => {
    render(<StatusDot status="active" label="Online" />);
    expect(screen.getByText('Online')).toBeTruthy();
  });

  it('does not render label text when not provided', () => {
    const { container } = render(<StatusDot status="active" />);
    // Only the dot span should exist, no label span
    const spans = container.querySelectorAll('span');
    // outer wrapper + dot = 2 spans; no label span
    expect(spans.length).toBe(2);
  });

  it('applies small size class', () => {
    render(<StatusDot status="active" size="sm" />);
    const dot = screen.getByRole('status');
    expect(dot.className).toContain('h-1.5');
    expect(dot.className).toContain('w-1.5');
  });

  it('applies medium size class (default)', () => {
    render(<StatusDot status="active" size="md" />);
    const dot = screen.getByRole('status');
    expect(dot.className).toContain('h-2.5');
    expect(dot.className).toContain('w-2.5');
  });

  it('applies large size class', () => {
    render(<StatusDot status="active" size="lg" />);
    const dot = screen.getByRole('status');
    expect(dot.className).toContain('h-3.5');
    expect(dot.className).toContain('w-3.5');
  });

  it('has aria-label matching the label prop when provided', () => {
    render(<StatusDot status="active" label="Running" />);
    const dot = screen.getByRole('status');
    expect(dot.getAttribute('aria-label')).toBe('Running');
  });

  it('has aria-label falling back to status when no label provided', () => {
    render(<StatusDot status="error" />);
    const dot = screen.getByRole('status');
    expect(dot.getAttribute('aria-label')).toBe('error');
  });

  it('applies default status and size when none specified', () => {
    render(<StatusDot />);
    const dot = screen.getByRole('status');
    // defaults: status=inactive, size=md
    expect(dot.className).toContain('bg-gray-500');
    expect(dot.className).toContain('h-2.5');
    expect(dot.className).toContain('w-2.5');
  });
});
