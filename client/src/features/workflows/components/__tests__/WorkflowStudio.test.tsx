import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import React from 'react';
import { WorkflowStudio } from '../WorkflowStudio';

describe('WorkflowStudio', () => {
  let fetchSpy: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    fetchSpy = vi.fn().mockResolvedValue({
      ok: true,
      json: () => Promise.resolve({ proposals: [], patterns: [] }),
    });
    global.fetch = fetchSpy as unknown as typeof fetch;
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('renders proposals tab by default', async () => {
    render(<WorkflowStudio />);
    await waitFor(() => {
      expect(screen.getByText('No proposals yet')).toBeTruthy();
    });
  });

  it('shows empty state with no proposals', async () => {
    render(<WorkflowStudio />);
    await waitFor(() => {
      expect(screen.getByText('No proposals yet')).toBeTruthy();
      expect(screen.getByText('Create a proposal to start the workflow governance loop.')).toBeTruthy();
    });
  });

  it('"New Proposal" button toggles form', async () => {
    render(<WorkflowStudio />);
    await waitFor(() => {
      expect(screen.queryByText('Loading...')).toBeNull();
    });

    const newButton = screen.getByRole('button', { name: /new proposal/i });
    expect(screen.queryByLabelText('Title')).toBeNull();

    fireEvent.click(newButton);
    expect(screen.getByLabelText('Title')).toBeTruthy();
    expect(screen.getByText('New Workflow Proposal')).toBeTruthy();

    // Click again to cancel
    const cancelButton = screen.getByRole('button', { name: /cancel/i });
    fireEvent.click(cancelButton);
    expect(screen.queryByText('New Workflow Proposal')).toBeNull();
  });

  it('form validates empty title (submit button disabled)', async () => {
    render(<WorkflowStudio />);
    await waitFor(() => {
      expect(screen.queryByText('Loading...')).toBeNull();
    });

    fireEvent.click(screen.getByRole('button', { name: /new proposal/i }));
    const submitButton = screen.getByRole('button', { name: /submit proposal/i });
    expect(submitButton).toBeDisabled();
  });
});
