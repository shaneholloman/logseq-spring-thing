import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import React from 'react';
import { Timeline, type TimelineItem } from '../Timeline';

const sampleItems: TimelineItem[] = [
  {
    id: '1',
    title: 'Created',
    timestamp: '10:00 AM',
    status: 'success',
    description: 'The item was created',
    metadata: { author: 'Alice', version: '1.0' },
  },
  {
    id: '2',
    title: 'Updated',
    timestamp: '11:00 AM',
    status: 'warning',
  },
  {
    id: '3',
    title: 'Deployed',
    timestamp: '12:00 PM',
    status: 'info',
  },
];

describe('Timeline', () => {
  it('renders all items', () => {
    render(<Timeline items={sampleItems} />);
    expect(screen.getByText('Created')).toBeTruthy();
    expect(screen.getByText('Updated')).toBeTruthy();
    expect(screen.getByText('Deployed')).toBeTruthy();
    expect(screen.getByText('10:00 AM')).toBeTruthy();
    expect(screen.getByText('11:00 AM')).toBeTruthy();
    expect(screen.getByText('12:00 PM')).toBeTruthy();
  });

  it('returns null for empty array', () => {
    const { container } = render(<Timeline items={[]} />);
    expect(container.innerHTML).toBe('');
  });

  it('shows connecting lines between items except last', () => {
    const { container } = render(<Timeline items={sampleItems} />);
    // The vertical line div has class "w-px bg-border"
    const lines = container.querySelectorAll('.w-px.bg-border');
    // 3 items: lines between 1-2 and 2-3, so 2 lines
    expect(lines.length).toBe(2);
  });

  it('applies status-specific dot colors for success', () => {
    const { container } = render(
      <Timeline items={[{ id: '1', title: 'Test', timestamp: 'now', status: 'success' }]} />
    );
    const dot = container.querySelector('.rounded-full');
    expect(dot?.className).toContain('bg-emerald-500');
  });

  it('applies status-specific dot colors for warning', () => {
    const { container } = render(
      <Timeline items={[{ id: '1', title: 'Test', timestamp: 'now', status: 'warning' }]} />
    );
    const dot = container.querySelector('.rounded-full');
    expect(dot?.className).toContain('bg-amber-500');
  });

  it('applies status-specific dot colors for error', () => {
    const { container } = render(
      <Timeline items={[{ id: '1', title: 'Test', timestamp: 'now', status: 'error' }]} />
    );
    const dot = container.querySelector('.rounded-full');
    expect(dot?.className).toContain('bg-red-500');
  });

  it('applies status-specific dot colors for info', () => {
    const { container } = render(
      <Timeline items={[{ id: '1', title: 'Test', timestamp: 'now', status: 'info' }]} />
    );
    const dot = container.querySelector('.rounded-full');
    expect(dot?.className).toContain('bg-blue-500');
  });

  it('applies default dot color when status is default', () => {
    const { container } = render(
      <Timeline items={[{ id: '1', title: 'Test', timestamp: 'now', status: 'default' }]} />
    );
    const dot = container.querySelector('.rounded-full');
    expect(dot?.className).toContain('bg-muted-foreground/50');
  });

  it('renders metadata badges', () => {
    render(<Timeline items={sampleItems} />);
    expect(screen.getByText('author: Alice')).toBeTruthy();
    expect(screen.getByText('version: 1.0')).toBeTruthy();
  });

  it('renders description when provided', () => {
    render(<Timeline items={sampleItems} />);
    expect(screen.getByText('The item was created')).toBeTruthy();
  });

  it('does not render description when not provided', () => {
    const { container } = render(
      <Timeline items={[{ id: '1', title: 'Test', timestamp: 'now' }]} />
    );
    // No <p> with description class
    const descriptions = container.querySelectorAll('p.text-sm.text-muted-foreground.mt-0\\.5');
    expect(descriptions.length).toBe(0);
  });
});
