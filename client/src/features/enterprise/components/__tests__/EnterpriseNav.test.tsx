import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import React from 'react';
import { EnterpriseNav } from '../EnterpriseNav';

describe('EnterpriseNav', () => {
  const defaultProps = {
    activePanel: 'broker',
    onPanelChange: vi.fn(),
  };

  it('renders all 5 nav items', () => {
    render(<EnterpriseNav {...defaultProps} />);
    expect(screen.getByText('Broker')).toBeTruthy();
    expect(screen.getByText('Workflows')).toBeTruthy();
    expect(screen.getByText('KPIs')).toBeTruthy();
    expect(screen.getByText('Connectors')).toBeTruthy();
    expect(screen.getByText('Policy')).toBeTruthy();
  });

  it('active item has aria-current="page"', () => {
    render(<EnterpriseNav {...defaultProps} activePanel="workflows" />);
    const workflowsButton = screen.getByText('Workflows').closest('button')!;
    expect(workflowsButton.getAttribute('aria-current')).toBe('page');

    const brokerButton = screen.getByText('Broker').closest('button')!;
    expect(brokerButton.getAttribute('aria-current')).toBeNull();
  });

  it('clicking item calls onPanelChange with correct id', () => {
    const onPanelChange = vi.fn();
    render(<EnterpriseNav {...defaultProps} onPanelChange={onPanelChange} />);

    fireEvent.click(screen.getByText('Connectors'));
    expect(onPanelChange).toHaveBeenCalledWith('connectors');

    fireEvent.click(screen.getByText('Policy'));
    expect(onPanelChange).toHaveBeenCalledWith('policy');

    fireEvent.click(screen.getByText('KPIs'));
    expect(onPanelChange).toHaveBeenCalledWith('kpi');
  });

  it('emoji spans have aria-hidden="true"', () => {
    render(<EnterpriseNav {...defaultProps} />);
    const hiddenSpans = document.querySelectorAll('[aria-hidden="true"]');
    expect(hiddenSpans.length).toBe(5);
  });

  it('nav has role="navigation"', () => {
    render(<EnterpriseNav {...defaultProps} />);
    const nav = screen.getByRole('navigation');
    expect(nav).toBeTruthy();
    expect(nav.getAttribute('aria-label')).toBe('Enterprise navigation');
  });
});
