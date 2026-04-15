import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import React from 'react';
import { BrokerInbox } from '../BrokerInbox';

describe('BrokerInbox', () => {
  let fetchSpy: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    fetchSpy = vi.fn().mockResolvedValue({
      ok: true,
      json: () => Promise.resolve({ cases: [], total: 0 }),
    });
    global.fetch = fetchSpy as unknown as typeof fetch;
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('renders loading state initially', () => {
    // Make fetch hang so loading stays visible
    fetchSpy.mockReturnValue(new Promise(() => {}));
    render(<BrokerInbox />);
    expect(screen.getByText('Loading broker inbox...')).toBeTruthy();
  });

  it('shows empty state when no cases', async () => {
    render(<BrokerInbox />);
    await waitFor(() => {
      expect(screen.getByText('No cases in inbox')).toBeTruthy();
    });
  });

  it('displays cases from API response', async () => {
    fetchSpy.mockResolvedValue({
      ok: true,
      json: () =>
        Promise.resolve({
          cases: [
            {
              id: 'case-1',
              title: 'Unapproved LLM usage detected',
              description: 'Data team using GPT-4 without approval',
              priority: 'high',
              source: 'policy_violation',
              status: 'open',
              createdAt: '2026-01-15',
              assignedTo: null,
            },
            {
              id: 'case-2',
              title: 'Trust drift in deployment pipeline',
              description: 'Confidence dropped below threshold',
              priority: 'critical',
              source: 'confidence_threshold',
              status: 'escalated',
              createdAt: '2026-01-16',
              assignedTo: 'broker-1',
            },
          ],
          total: 2,
        }),
    });

    render(<BrokerInbox />);
    await waitFor(() => {
      expect(screen.getByText('Unapproved LLM usage detected')).toBeTruthy();
      expect(screen.getByText('Trust drift in deployment pipeline')).toBeTruthy();
    });
  });

  it('status filter Select has aria-label', async () => {
    render(<BrokerInbox />);
    await waitFor(() => {
      expect(screen.queryByText('Loading broker inbox...')).toBeNull();
    });
    const trigger = screen.getByRole('combobox', { name: 'Filter cases by status' });
    expect(trigger).toBeTruthy();
  });

  it('case card has role="button" and tabIndex', async () => {
    fetchSpy.mockResolvedValue({
      ok: true,
      json: () =>
        Promise.resolve({
          cases: [
            {
              id: 'case-1',
              title: 'Test case',
              description: 'desc',
              priority: 'medium',
              source: 'manual_submission',
              status: 'open',
              createdAt: '2026-01-15',
              assignedTo: null,
            },
          ],
          total: 1,
        }),
    });

    render(<BrokerInbox />);
    await waitFor(() => {
      expect(screen.getByText('Test case')).toBeTruthy();
    });

    const card = screen.getByRole('button');
    expect(card).toBeTruthy();
    expect(card.getAttribute('tabindex')).toBe('0');
  });
});
