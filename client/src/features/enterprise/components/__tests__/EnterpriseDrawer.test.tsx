import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
import React from 'react';
import { EnterpriseDrawer } from '../EnterpriseDrawer';

// framer-motion reads `window.matchMedia` via useReducedMotion; jsdom omits it.
function stubMatchMedia(prefersReduced: boolean) {
  Object.defineProperty(window, 'matchMedia', {
    writable: true,
    configurable: true,
    value: (query: string) => ({
      matches: query.includes('reduce') ? prefersReduced : false,
      media: query,
      onchange: null,
      addListener: vi.fn(),
      removeListener: vi.fn(),
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      dispatchEvent: vi.fn(),
    }),
  });
}

describe('EnterpriseDrawer', () => {
  beforeEach(() => {
    stubMatchMedia(false);
  });

  it('renders nothing when closed', () => {
    render(
      <EnterpriseDrawer open={false} onClose={vi.fn()}>
        <div>Inner</div>
      </EnterpriseDrawer>,
    );
    expect(screen.queryByTestId('enterprise-drawer')).toBeNull();
  });

  it('renders container and children when open', () => {
    render(
      <EnterpriseDrawer open={true} onClose={vi.fn()}>
        <div>Inner</div>
      </EnterpriseDrawer>,
    );
    expect(screen.getByTestId('enterprise-drawer')).toBeTruthy();
    expect(screen.getByText('Inner')).toBeTruthy();
    expect(screen.getByRole('dialog').getAttribute('aria-modal')).toBe('true');
  });

  it('close button invokes onClose', () => {
    const onClose = vi.fn();
    render(
      <EnterpriseDrawer open={true} onClose={onClose}>
        <div>Inner</div>
      </EnterpriseDrawer>,
    );
    fireEvent.click(screen.getByLabelText('Close enterprise drawer'));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('Escape key invokes onClose', () => {
    const onClose = vi.fn();
    render(
      <EnterpriseDrawer open={true} onClose={onClose}>
        <div>Inner</div>
      </EnterpriseDrawer>,
    );
    act(() => {
      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('clicking the scrim does NOT dismiss (workspace semantics)', () => {
    const onClose = vi.fn();
    render(
      <EnterpriseDrawer open={true} onClose={onClose}>
        <div>Inner</div>
      </EnterpriseDrawer>,
    );
    const scrim = screen
      .getByTestId('enterprise-drawer')
      .querySelector('[aria-hidden="true"]')!;
    fireEvent.click(scrim);
    expect(onClose).not.toHaveBeenCalled();
  });

  it('respects prefers-reduced-motion (still renders, no crash)', () => {
    stubMatchMedia(true);
    render(
      <EnterpriseDrawer open={true} onClose={vi.fn()}>
        <div>Inner</div>
      </EnterpriseDrawer>,
    );
    // The panel should still mount; variants differ but assertions stay behavioural.
    expect(screen.getByRole('dialog')).toBeTruthy();
  });
});
