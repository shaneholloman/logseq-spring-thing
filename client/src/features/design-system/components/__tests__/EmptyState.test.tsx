import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import React from 'react';
import { EmptyState } from '../EmptyState';

describe('EmptyState', () => {
  it('renders title', () => {
    render(<EmptyState title="No items found" />);
    expect(screen.getByText('No items found')).toBeTruthy();
  });

  it('renders description when provided', () => {
    render(<EmptyState title="Empty" description="Try adding some items" />);
    expect(screen.getByText('Try adding some items')).toBeTruthy();
  });

  it('does not render description when not provided', () => {
    const { container } = render(<EmptyState title="Empty" />);
    const descriptionEl = container.querySelector('.text-muted-foreground\\/70');
    expect(descriptionEl).toBeNull();
  });

  it('renders icon when provided', () => {
    render(<EmptyState title="Empty" icon={<span data-testid="icon">ICON</span>} />);
    expect(screen.getByTestId('icon')).toBeTruthy();
  });

  it('does not render icon container when not provided', () => {
    const { container } = render(<EmptyState title="Empty" />);
    const iconDiv = container.querySelector('.text-4xl');
    expect(iconDiv).toBeNull();
  });

  it('renders action when provided', () => {
    render(
      <EmptyState
        title="Empty"
        action={<button data-testid="action-btn">Add Item</button>}
      />
    );
    expect(screen.getByTestId('action-btn')).toBeTruthy();
    expect(screen.getByText('Add Item')).toBeTruthy();
  });

  it('does not render action container when not provided', () => {
    const { container } = render(<EmptyState title="Empty" />);
    const actionDiv = container.querySelector('.mt-2');
    expect(actionDiv).toBeNull();
  });

  it('applies small size variant classes', () => {
    const { container } = render(<EmptyState title="Empty" size="sm" />);
    const wrapper = container.firstElementChild as HTMLElement;
    expect(wrapper.className).toContain('py-6');
    expect(wrapper.className).toContain('gap-2');
  });

  it('applies medium size variant classes (default)', () => {
    const { container } = render(<EmptyState title="Empty" />);
    const wrapper = container.firstElementChild as HTMLElement;
    expect(wrapper.className).toContain('py-12');
    expect(wrapper.className).toContain('gap-3');
  });

  it('applies large size variant classes', () => {
    const { container } = render(<EmptyState title="Empty" size="lg" />);
    const wrapper = container.firstElementChild as HTMLElement;
    expect(wrapper.className).toContain('py-20');
    expect(wrapper.className).toContain('gap-4');
  });
});
