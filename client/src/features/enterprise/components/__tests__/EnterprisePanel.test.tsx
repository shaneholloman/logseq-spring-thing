import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import React from 'react';
import { EnterprisePanel } from '../EnterprisePanel';

// Mock all child panels to avoid deep rendering and fetch side effects
vi.mock('../../../broker/components/BrokerWorkbench', () => ({
  BrokerWorkbench: () => <div data-testid="broker-workbench">BrokerWorkbench</div>,
}));

vi.mock('../../../workflows/components/WorkflowStudio', () => ({
  WorkflowStudio: () => <div data-testid="workflow-studio">WorkflowStudio</div>,
}));

vi.mock('../../../kpi/components/MeshKpiDashboard', () => ({
  MeshKpiDashboard: () => <div data-testid="kpi-dashboard">MeshKpiDashboard</div>,
}));

vi.mock('../../../connectors/components/ConnectorPanel', () => ({
  ConnectorPanel: () => <div data-testid="connector-panel">ConnectorPanel</div>,
}));

vi.mock('../../../policy/components/PolicyConsole', () => ({
  PolicyConsole: () => <div data-testid="policy-console">PolicyConsole</div>,
}));

describe('EnterprisePanel', () => {
  let fetchSpy: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    fetchSpy = vi.fn().mockResolvedValue({
      ok: true,
      json: () => Promise.resolve({}),
    });
    global.fetch = fetchSpy as unknown as typeof fetch;
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('default panel is broker (BrokerWorkbench visible)', () => {
    render(<EnterprisePanel />);
    const brokerContainer = screen.getByTestId('broker-workbench').closest('div[class]')!;
    expect(brokerContainer.className).toContain('block');
  });

  it('all panels remain mounted (non-active panels have hidden class)', () => {
    render(<EnterprisePanel />);
    // All panels are rendered in the DOM
    expect(screen.getByTestId('broker-workbench')).toBeTruthy();
    expect(screen.getByTestId('workflow-studio')).toBeTruthy();
    expect(screen.getByTestId('kpi-dashboard')).toBeTruthy();
    expect(screen.getByTestId('connector-panel')).toBeTruthy();
    expect(screen.getByTestId('policy-console')).toBeTruthy();

    // Non-active panels have 'hidden' class
    const workflowContainer = screen.getByTestId('workflow-studio').parentElement!;
    expect(workflowContainer.className).toContain('hidden');
  });
});
