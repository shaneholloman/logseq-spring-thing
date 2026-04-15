import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import React from 'react';
import { MeshKpiDashboard } from '../MeshKpiDashboard';

describe('MeshKpiDashboard', () => {
  let fetchSpy: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    fetchSpy = vi.fn().mockResolvedValue({
      ok: true,
      json: () =>
        Promise.resolve({
          kpis: {
            mesh_velocity: { value: 32, unit: 'hours', description: 'Avg time to formalise', status: 'healthy' },
            augmentation_ratio: { value: 72, unit: 'percentage', description: 'Automated vs manual', status: 'healthy' },
            trust_variance: { value: 0.08, unit: 'sigma', description: 'Trust score spread', status: 'healthy' },
            hitl_precision: { value: 94, unit: 'percentage', description: 'Human-in-the-loop accuracy', status: 'healthy' },
          },
        }),
    });
    global.fetch = fetchSpy as unknown as typeof fetch;
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('renders 4 KPI cards', async () => {
    render(<MeshKpiDashboard />);
    await waitFor(() => {
      expect(screen.getByText(/Mesh Velocity/)).toBeTruthy();
      expect(screen.getByText(/Augmentation Ratio/)).toBeTruthy();
      expect(screen.getByText(/Trust Variance/)).toBeTruthy();
      expect(screen.getByText(/HITL Precision/)).toBeTruthy();
    });
  });

  it('shows dash for null values', async () => {
    fetchSpy.mockResolvedValue({
      ok: true,
      json: () =>
        Promise.resolve({
          kpis: {
            mesh_velocity: { value: null, unit: 'hours', description: 'No data', status: 'not_computed' },
            augmentation_ratio: { value: null, unit: 'percentage', description: 'No data', status: 'not_computed' },
            trust_variance: { value: null, unit: 'sigma', description: 'No data', status: 'not_computed' },
            hitl_precision: { value: null, unit: 'percentage', description: 'No data', status: 'not_computed' },
          },
        }),
    });

    render(<MeshKpiDashboard />);
    await waitFor(() => {
      const dashes = screen.getAllByText('\u2014');
      expect(dashes.length).toBe(4);
    });
  });

  it('time window Select has aria-label', async () => {
    render(<MeshKpiDashboard />);
    const trigger = screen.getByRole('combobox', { name: 'Select time window' });
    expect(trigger).toBeTruthy();
  });

  it('Sparkline components render', async () => {
    render(<MeshKpiDashboard />);
    await waitFor(() => {
      const canvases = document.querySelectorAll('canvas[role="img"]');
      expect(canvases.length).toBeGreaterThanOrEqual(4);
    });
  });
});
