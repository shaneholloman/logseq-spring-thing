import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import React from 'react';
import { CaseSubmitForm } from '../CaseSubmitForm';

describe('CaseSubmitForm', () => {
  let fetchSpy: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    fetchSpy = vi.fn().mockResolvedValue({
      ok: true,
      json: () => Promise.resolve({ id: 'case-123' }),
    });
    global.fetch = fetchSpy as unknown as typeof fetch;
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('renders form fields (title, description, priority)', () => {
    render(<CaseSubmitForm />);
    expect(screen.getByLabelText('Title')).toBeTruthy();
    expect(screen.getByLabelText('Description')).toBeTruthy();
    expect(screen.getByText('Priority')).toBeTruthy();
  });

  it('submit button is disabled when title is empty', () => {
    render(<CaseSubmitForm />);
    const submitBtn = screen.getByRole('button', { name: /submit case/i });
    expect(submitBtn).toBeDisabled();
  });

  it('submit button is enabled when title has content', () => {
    render(<CaseSubmitForm />);
    const titleInput = screen.getByLabelText('Title');
    fireEvent.change(titleInput, { target: { value: 'Test case' } });
    const submitBtn = screen.getByRole('button', { name: /submit case/i });
    expect(submitBtn).not.toBeDisabled();
  });

  it('calls fetch on form submission', async () => {
    render(<CaseSubmitForm />);
    const titleInput = screen.getByLabelText('Title');
    fireEvent.change(titleInput, { target: { value: 'Unapproved LLM usage' } });

    fireEvent.click(screen.getByRole('button', { name: /submit case/i }));

    await waitFor(() => {
      // `apiPost` (utils/apiFetch.ts) wraps headers in a `Headers` instance
      // before calling `fetch`, so an exact-shape match on a plain object
      // would never hit. Assert structural fields that survive the wrapper.
      expect(fetchSpy).toHaveBeenCalledWith(
        '/api/broker/cases',
        expect.objectContaining({ method: 'POST' }),
      );
    });

    const init = fetchSpy.mock.calls[0][1] as RequestInit;
    const headers = init.headers instanceof Headers
      ? init.headers
      : new Headers(init.headers as HeadersInit | undefined);
    expect(headers.get('Content-Type')).toBe('application/json');

    const callBody = JSON.parse(init.body as string);
    expect(callBody.title).toBe('Unapproved LLM usage');
    expect(callBody.source).toBe('manual_submission');
  });

  it('shows success message after submission', async () => {
    render(<CaseSubmitForm />);
    const titleInput = screen.getByLabelText('Title');
    fireEvent.change(titleInput, { target: { value: 'Test case' } });
    fireEvent.click(screen.getByRole('button', { name: /submit case/i }));

    await waitFor(() => {
      expect(screen.getByText('Case case-123 created')).toBeTruthy();
    });
  });

  it('calls onSubmitted callback after submission', async () => {
    vi.useFakeTimers();
    const onSubmitted = vi.fn();
    render(<CaseSubmitForm onSubmitted={onSubmitted} />);
    const titleInput = screen.getByLabelText('Title');
    fireEvent.change(titleInput, { target: { value: 'Test' } });
    fireEvent.click(screen.getByRole('button', { name: /submit case/i }));

    // Wait for the fetch to complete
    await vi.waitFor(() => {
      expect(fetchSpy).toHaveBeenCalled();
    });

    // Allow fetch promise microtasks to resolve
    await vi.advanceTimersByTimeAsync(100);

    // onSubmitted is called after a 1500ms setTimeout
    vi.advanceTimersByTime(1600);
    expect(onSubmitted).toHaveBeenCalled();
    vi.useRealTimers();
  });

  it('handles fetch error gracefully', async () => {
    const consoleSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    fetchSpy.mockResolvedValueOnce({
      ok: false,
      status: 500,
      statusText: 'Internal Server Error',
      json: () => Promise.resolve({ error: 'Server failed' }),
    });

    render(<CaseSubmitForm />);
    const titleInput = screen.getByLabelText('Title');
    fireEvent.change(titleInput, { target: { value: 'Test' } });
    fireEvent.click(screen.getByRole('button', { name: /submit case/i }));

    await waitFor(() => {
      expect(consoleSpy).toHaveBeenCalled();
    });

    // No success message should appear
    expect(screen.queryByText(/Case .* created/)).toBeNull();
    consoleSpy.mockRestore();
  });
});
