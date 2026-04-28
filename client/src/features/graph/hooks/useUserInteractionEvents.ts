/**
 * Phase 3 of ADR-059 — emit user_interaction events to the server when
 * the user focuses / selects / hovers / drags a graph node.
 *
 * Server route: POST /api/v1/agent-events/user-interaction
 *
 * - focus  : >1.5s dwell hysteresis on a single node (camera-centred)
 * - select : single click / tap
 * - hover  : raycast hover ≥250 ms
 * - drag   : interactive grab in progress
 *
 * Events are transient (not persisted to Neo4j) and tied to the
 * current session UUID.
 */

import { useEffect, useRef } from 'react';

type Kind = 'focus' | 'select' | 'hover' | 'drag';

interface UserInteractionEvent {
  version: 1;
  type: 'user_interaction';
  kind: Kind;
  session_id: string;
  session_pubkey?: string;
  target_node_id: number;
  target_urn?: string;
  duration_ms: number;
  timestamp: number;
}

const ENDPOINT = '/api/v1/agent-events/user-interaction';

let _sessionId: string | null = null;
function sessionId(): string {
  if (_sessionId) return _sessionId;
  _sessionId = (globalThis.crypto?.randomUUID?.() as string) ||
    `sess-${Date.now()}-${Math.random().toString(36).slice(2)}`;
  return _sessionId;
}

async function postEvent(evt: UserInteractionEvent): Promise<void> {
  try {
    await fetch(ENDPOINT, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(evt),
      credentials: 'include',
      keepalive: true,
    });
  } catch {
    // Phase 1 best-effort; agent-events is non-critical telemetry.
  }
}

export function emitUserInteraction(
  kind: Kind,
  targetNodeId: number,
  durationMs: number,
  opts: { sessionPubkey?: string; targetUrn?: string } = {}
): void {
  const numericId = Number(targetNodeId);
  if (!Number.isFinite(numericId) || numericId < 0) return;
  const evt: UserInteractionEvent = {
    version: 1,
    type: 'user_interaction',
    kind,
    session_id: sessionId(),
    session_pubkey: opts.sessionPubkey,
    target_node_id: numericId >>> 0,
    target_urn: opts.targetUrn,
    duration_ms: Math.max(1, Math.min(durationMs, 60_000)),
    timestamp: Date.now(),
  };
  postEvent(evt);
}

/**
 * Convenience hook: emits `select` whenever `selectedNodeId` changes
 * to a non-null value, and `focus` after the same node has been
 * selected for ≥1500 ms (hysteresis-debounced).
 */
export function useSelectionAndFocusEvents(
  selectedNodeId: string | null,
  opts: { sessionPubkey?: string } = {}
): void {
  const lastSelectedRef = useRef<string | null>(null);
  const focusTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    if (focusTimerRef.current) {
      clearTimeout(focusTimerRef.current);
      focusTimerRef.current = null;
    }
    if (!selectedNodeId) {
      lastSelectedRef.current = null;
      return;
    }
    if (selectedNodeId === lastSelectedRef.current) return;
    lastSelectedRef.current = selectedNodeId;

    const idNum = Number(selectedNodeId);
    if (!Number.isFinite(idNum)) return;

    emitUserInteraction('select', idNum, 250, { sessionPubkey: opts.sessionPubkey });

    focusTimerRef.current = setTimeout(() => {
      if (lastSelectedRef.current === selectedNodeId) {
        emitUserInteraction('focus', idNum, 1500, { sessionPubkey: opts.sessionPubkey });
      }
    }, 1500);

    return () => {
      if (focusTimerRef.current) {
        clearTimeout(focusTimerRef.current);
        focusTimerRef.current = null;
      }
    };
  }, [selectedNodeId, opts.sessionPubkey]);
}

/**
 * Lightweight hover-dwell tracker. Call `onHoverEnter(nodeId)` and
 * `onHoverLeave()` from pointer handlers; emits `hover` if dwell ≥250ms.
 */
export function createHoverTracker(opts: { sessionPubkey?: string } = {}) {
  let activeNodeId: number | null = null;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let enteredAt = 0;

  return {
    onHoverEnter(nodeId: number) {
      if (activeNodeId === nodeId) return;
      this.onHoverLeave();
      activeNodeId = nodeId;
      enteredAt = Date.now();
      timer = setTimeout(() => {
        if (activeNodeId === nodeId) {
          emitUserInteraction('hover', nodeId, Date.now() - enteredAt, opts);
        }
      }, 250);
    },
    onHoverLeave() {
      if (timer) { clearTimeout(timer); timer = null; }
      activeNodeId = null;
    },
  };
}

/**
 * Drag tracker — emit a single `drag` event with the actual grab duration
 * once the user releases.
 */
export function createDragTracker(opts: { sessionPubkey?: string } = {}) {
  let dragNodeId: number | null = null;
  let startedAt = 0;
  return {
    onDragStart(nodeId: number) {
      dragNodeId = nodeId;
      startedAt = Date.now();
    },
    onDragEnd() {
      if (dragNodeId == null) return;
      const dur = Date.now() - startedAt;
      emitUserInteraction('drag', dragNodeId, dur, opts);
      dragNodeId = null;
    },
  };
}
