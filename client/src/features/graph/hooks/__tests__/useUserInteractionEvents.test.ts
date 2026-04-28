/**
 * ADR-059 §3 / Phase 3 — useUserInteractionEvents tests (vitest).
 *
 * Covers:
 *   1. emitUserInteraction posts JSON to /api/v1/agent-events/user-interaction
 *   2. Negative target_node_id is rejected without POST
 *   3. Hysteresis: select fires immediately, focus fires at 1500 ms only if
 *      the same selection is still active
 *   4. duration_ms clamps to [1, 60_000]
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import React from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { act } from 'react';

import {
  emitUserInteraction,
  useSelectionAndFocusEvents,
} from '../useUserInteractionEvents';

// ---------------------------------------------------------------------------
// fetch mock — captures every request the hook would make.
// ---------------------------------------------------------------------------

let fetchSpy: ReturnType<typeof vi.fn>;

beforeEach(() => {
  fetchSpy = vi.fn().mockResolvedValue(new Response(null, { status: 202 }));
  // jsdom's globalThis.fetch is undefined by default; assign our mock.
  (globalThis as { fetch: typeof fetchSpy }).fetch = fetchSpy;
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

// ---------------------------------------------------------------------------
// 1. emitUserInteraction posts JSON to the right endpoint
// ---------------------------------------------------------------------------

describe('emitUserInteraction', () => {
  it('posts JSON to /api/v1/agent-events/user-interaction', () => {
    emitUserInteraction('select', 4242, 250);
    expect(fetchSpy).toHaveBeenCalledTimes(1);
    const [url, init] = fetchSpy.mock.calls[0];
    expect(url).toBe('/api/v1/agent-events/user-interaction');
    expect(init).toMatchObject({
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      credentials: 'include',
      keepalive: true,
    });
    const body = JSON.parse(init.body as string);
    expect(body).toMatchObject({
      version: 1,
      type: 'user_interaction',
      kind: 'select',
      target_node_id: 4242,
      duration_ms: 250,
    });
    expect(typeof body.session_id).toBe('string');
    expect(body.session_id.length).toBeGreaterThan(0);
    expect(typeof body.timestamp).toBe('number');
  });

  it('forwards optional sessionPubkey and targetUrn', () => {
    emitUserInteraction('hover', 7, 300, {
      sessionPubkey: 'pk-deadbeef',
      targetUrn: 'urn:visionclaw:kg:npub1xyz:sha256-12-abc',
    });
    const body = JSON.parse(fetchSpy.mock.calls[0][1].body as string);
    expect(body.session_pubkey).toBe('pk-deadbeef');
    expect(body.target_urn).toBe('urn:visionclaw:kg:npub1xyz:sha256-12-abc');
  });

  // -------------------------------------------------------------------------
  // 2. Rejection of negative / non-finite target_node_id
  // -------------------------------------------------------------------------

  it('rejects negative target_node_id without POST', () => {
    emitUserInteraction('select', -1, 100);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('rejects NaN target_node_id without POST', () => {
    emitUserInteraction('select', Number.NaN, 100);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('rejects Infinity target_node_id without POST', () => {
    emitUserInteraction('select', Number.POSITIVE_INFINITY, 100);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  // -------------------------------------------------------------------------
  // 4. duration_ms clamps to [1, 60_000]
  // -------------------------------------------------------------------------

  it('clamps duration_ms = 0 up to 1', () => {
    emitUserInteraction('hover', 1, 0);
    const body = JSON.parse(fetchSpy.mock.calls[0][1].body as string);
    expect(body.duration_ms).toBe(1);
  });

  it('clamps duration_ms = -100 up to 1', () => {
    emitUserInteraction('hover', 1, -100);
    const body = JSON.parse(fetchSpy.mock.calls[0][1].body as string);
    expect(body.duration_ms).toBe(1);
  });

  it('clamps duration_ms = 999_999 down to 60_000', () => {
    emitUserInteraction('hover', 1, 999_999);
    const body = JSON.parse(fetchSpy.mock.calls[0][1].body as string);
    expect(body.duration_ms).toBe(60_000);
  });

  it('passes duration_ms = 30_000 unchanged', () => {
    emitUserInteraction('drag', 1, 30_000);
    const body = JSON.parse(fetchSpy.mock.calls[0][1].body as string);
    expect(body.duration_ms).toBe(30_000);
  });
});

// ---------------------------------------------------------------------------
// 3. useSelectionAndFocusEvents — hysteresis: select then focus@1500ms
// ---------------------------------------------------------------------------

interface HookHarnessProps {
  selectedNodeId: string | null;
}

function HookHarness({ selectedNodeId }: HookHarnessProps): null {
  useSelectionAndFocusEvents(selectedNodeId);
  return null;
}

function mountHarness(): { root: Root; container: HTMLElement } {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const root = createRoot(container);
  return { root, container };
}

function renderWith(root: Root, selectedNodeId: string | null) {
  act(() => {
    root.render(React.createElement(HookHarness, { selectedNodeId }));
  });
}

describe('useSelectionAndFocusEvents (hysteresis)', () => {
  it('fires select immediately when selectedNodeId becomes non-null', () => {
    const { root, container } = mountHarness();
    renderWith(root, '42');

    expect(fetchSpy).toHaveBeenCalledTimes(1);
    const body = JSON.parse(fetchSpy.mock.calls[0][1].body as string);
    expect(body.kind).toBe('select');
    expect(body.target_node_id).toBe(42);

    act(() => root.unmount());
    document.body.removeChild(container);
  });

  it('fires focus exactly 1500 ms after a stable selection', () => {
    const { root, container } = mountHarness();
    renderWith(root, '42');
    expect(fetchSpy).toHaveBeenCalledTimes(1); // select

    // Just before 1500 ms — no focus yet.
    act(() => {
      vi.advanceTimersByTime(1499);
    });
    expect(fetchSpy).toHaveBeenCalledTimes(1);

    // Cross the 1500 ms boundary — focus fires.
    act(() => {
      vi.advanceTimersByTime(1);
    });
    expect(fetchSpy).toHaveBeenCalledTimes(2);
    const body = JSON.parse(fetchSpy.mock.calls[1][1].body as string);
    expect(body.kind).toBe('focus');
    expect(body.target_node_id).toBe(42);

    act(() => root.unmount());
    document.body.removeChild(container);
  });

  it('does NOT fire focus if the selection changes within 1500 ms', () => {
    const { root, container } = mountHarness();
    renderWith(root, '42');
    expect(fetchSpy).toHaveBeenCalledTimes(1); // select 42

    // Switch selection before the focus timer fires.
    act(() => {
      vi.advanceTimersByTime(1000);
    });
    renderWith(root, '99');
    expect(fetchSpy).toHaveBeenCalledTimes(2); // select 99

    // Advance past the original 1500 ms window — no focus for 42.
    act(() => {
      vi.advanceTimersByTime(600);
    });
    // No focus event for either node yet (still under 1500 ms for 99).
    expect(fetchSpy).toHaveBeenCalledTimes(2);

    // Now 99 stays stable for the rest of the 1500 ms window.
    act(() => {
      vi.advanceTimersByTime(1000);
    });
    expect(fetchSpy).toHaveBeenCalledTimes(3);
    const focusBody = JSON.parse(fetchSpy.mock.calls[2][1].body as string);
    expect(focusBody.kind).toBe('focus');
    expect(focusBody.target_node_id).toBe(99);

    act(() => root.unmount());
    document.body.removeChild(container);
  });

  it('does NOT fire focus if selection clears before 1500 ms', () => {
    const { root, container } = mountHarness();
    renderWith(root, '42');
    expect(fetchSpy).toHaveBeenCalledTimes(1); // select

    act(() => {
      vi.advanceTimersByTime(500);
    });
    renderWith(root, null);

    act(() => {
      vi.advanceTimersByTime(2000);
    });
    // Only the original `select` — no focus.
    expect(fetchSpy).toHaveBeenCalledTimes(1);

    act(() => root.unmount());
    document.body.removeChild(container);
  });

  it('does not re-fire select when the same id is set twice in a row', () => {
    const { root, container } = mountHarness();
    renderWith(root, '42');
    renderWith(root, '42');
    expect(fetchSpy).toHaveBeenCalledTimes(1);

    act(() => root.unmount());
    document.body.removeChild(container);
  });
});
