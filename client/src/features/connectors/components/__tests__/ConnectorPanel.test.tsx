import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import React from 'react';
import { ConnectorPanel } from '../ConnectorPanel';

describe('ConnectorPanel', () => {
  let fetchSpy: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    fetchSpy = vi.fn().mockResolvedValue({
      ok: true,
      json: () => Promise.resolve({ success: true, data: { connectors: [], total: 0 } }),
    });
    global.fetch = fetchSpy as unknown as typeof fetch;
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('renders empty state', async () => {
    render(<ConnectorPanel />);
    await waitFor(() => {
      expect(screen.getByText('No connectors configured')).toBeTruthy();
    });
  });

  it('"Add Connector" button shows setup form', async () => {
    render(<ConnectorPanel />);
    await waitFor(() => {
      expect(screen.getByText('No connectors configured')).toBeTruthy();
    });

    expect(screen.queryByText('Configure GitHub Connector')).toBeNull();

    fireEvent.click(screen.getByRole('button', { name: /add connector/i }));
    expect(screen.getByText('Configure GitHub Connector')).toBeTruthy();
    expect(screen.getByLabelText('GitHub Organisation')).toBeTruthy();
  });

  it('form validates empty org name (Create button disabled)', async () => {
    render(<ConnectorPanel />);
    await waitFor(() => {
      expect(screen.getByText('No connectors configured')).toBeTruthy();
    });

    fireEvent.click(screen.getByRole('button', { name: /add connector/i }));

    const createButton = screen.getByRole('button', { name: /create connector/i });
    expect(createButton).toBeDisabled();
  });

  it('tabs exist for connectors and signals', async () => {
    render(<ConnectorPanel />);
    await waitFor(() => {
      expect(screen.getByText('No connectors configured')).toBeTruthy();
    });

    // Both tab triggers are present
    const connectorsTab = screen.getByRole('tab', { name: /connectors/i });
    const signalsTab = screen.getByRole('tab', { name: /signal feed/i });
    expect(connectorsTab).toBeTruthy();
    expect(signalsTab).toBeTruthy();

    // Connectors tab is active by default
    expect(connectorsTab.getAttribute('data-state')).toBe('active');
    expect(signalsTab.getAttribute('data-state')).toBe('inactive');
  });
});
