/**
 * Migration Event Toast (ADR-048, Sprint 3).
 *
 * Subscribes to the migration event stream and renders short-lived toasts
 * for every KG->OWL promotion (Nostr kind 30100). Click dispatches a
 * `visionflow:migration-focus` CustomEvent that GraphManager consumes to
 * pan the camera + pulse the filament amber->cyan (ADR-048).
 *
 * Transport: WebSocket at `/api/bridge/events/ws` preferred; falls back to
 * `/api/bridge/events` polling every 10s if the socket cannot be opened.
 * Gated by `BRIDGE_EDGE_ENABLED`.
 */

import React, { useEffect, useMemo, useRef, useState } from 'react';
import { Button } from '../design-system/components';
import {
  useMigrationEventsStore,
  MIGRATION_EVENT_BUFFER_SIZE,
} from '../graph/store/migrationEventsSlice';
import { useFeatureFlag } from '../../services/featureFlags';
import type { BridgePromotionEvent } from '../graph/types/graphTypes';
import { createLogger } from '../../utils/loggerConfig';

const logger = createLogger('MigrationEventToast');

const POLL_INTERVAL_MS = 10_000;
const TOAST_TTL_MS = 3_000;
const MAX_VISIBLE = 3;

const FOCUS_EVENT = 'visionflow:migration-focus';

/** Build the absolute WS url from `window.location` (http->ws, https->wss). */
function buildWsUrl(path: string): string {
  if (typeof window === 'undefined') return path;
  const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
  return `${proto}//${window.location.host}${path}`;
}

interface ToastEntry {
  event: BridgePromotionEvent;
  shownAt: number;
}

// =============================================================================
// Public CustomEvent helper
// =============================================================================

export interface MigrationFocusDetail {
  from_kg: string;
  to_owl: string;
  edge_id?: string;
  event_id: string;
}

export function dispatchMigrationFocus(detail: MigrationFocusDetail): void {
  if (typeof window === 'undefined') return;
  window.dispatchEvent(new CustomEvent<MigrationFocusDetail>(FOCUS_EVENT, { detail }));
}

// =============================================================================
// Hook: drive the event stream (WS with polling fallback)
// =============================================================================

function useMigrationEventStream(enabled: boolean): void {
  const ingest = useMigrationEventsStore((s) => s.ingest);
  const pollOnce = useMigrationEventsStore((s) => s.pollOnce);
  const setSocketState = useMigrationEventsStore((s) => s.setSocketState);
  const socketRef = useRef<WebSocket | null>(null);
  const pollHandleRef = useRef<number | null>(null);

  useEffect(() => {
    if (!enabled) return;

    let cancelled = false;
    let retryTimer: number | null = null;

    function startPolling(): void {
      if (pollHandleRef.current !== null) return;
      void pollOnce();
      pollHandleRef.current = window.setInterval(() => {
        void pollOnce();
      }, POLL_INTERVAL_MS);
    }

    function stopPolling(): void {
      if (pollHandleRef.current !== null) {
        window.clearInterval(pollHandleRef.current);
        pollHandleRef.current = null;
      }
    }

    function connectWebSocket(): void {
      if (cancelled) return;
      setSocketState('connecting');
      let ws: WebSocket;
      try {
        ws = new WebSocket(buildWsUrl('/api/bridge/events/ws'));
      } catch (err: unknown) {
        logger.warn('[migration] WS construction failed, polling fallback', err);
        setSocketState('error');
        startPolling();
        return;
      }
      socketRef.current = ws;
      ws.onopen = () => {
        setSocketState('open');
        stopPolling();
      };
      ws.onmessage = (msg) => {
        try {
          const parsed = JSON.parse(msg.data) as BridgePromotionEvent;
          if (parsed && typeof parsed.id === 'string') {
            ingest(parsed);
          }
        } catch (err: unknown) {
          logger.debug('[migration] dropped malformed WS payload', err);
        }
      };
      ws.onerror = () => {
        setSocketState('error');
      };
      ws.onclose = () => {
        setSocketState('closed');
        socketRef.current = null;
        if (!cancelled) {
          // Fall back to polling until the socket reopens.
          startPolling();
          retryTimer = window.setTimeout(connectWebSocket, 15_000);
        }
      };
    }

    connectWebSocket();

    return () => {
      cancelled = true;
      stopPolling();
      if (retryTimer !== null) window.clearTimeout(retryTimer);
      if (socketRef.current) {
        try {
          socketRef.current.close();
        } catch {
          // Ignore - already closed.
        }
        socketRef.current = null;
      }
    };
  }, [enabled, ingest, pollOnce, setSocketState]);
}

// =============================================================================
// Component
// =============================================================================

export function MigrationEventToast(): React.ReactElement | null {
  const enabled = useFeatureFlag('BRIDGE_EDGE_ENABLED');
  useMigrationEventStream(enabled);

  const events = useMigrationEventsStore((s) => s.events);
  const [queue, setQueue] = useState<ToastEntry[]>([]);
  const lastSeenIdRef = useRef<string | null>(null);

  // Push new events onto the toast queue.
  useEffect(() => {
    if (!enabled) return;
    if (events.length === 0) return;
    const latest = events[0];
    if (!latest || latest.id === lastSeenIdRef.current) return;

    // On first mount, lastSeenIdRef is null - do not flood the user with
    // historical events. Mark them all as seen instead.
    if (lastSeenIdRef.current === null) {
      lastSeenIdRef.current = latest.id;
      return;
    }

    // Collect every event newer than the last seen id.
    const fresh: BridgePromotionEvent[] = [];
    for (const e of events) {
      if (e.id === lastSeenIdRef.current) break;
      fresh.push(e);
    }
    if (fresh.length === 0) return;

    lastSeenIdRef.current = latest.id;
    const now = Date.now();
    setQueue((prev) =>
      [...fresh.map((event) => ({ event, shownAt: now })), ...prev].slice(
        0,
        MIGRATION_EVENT_BUFFER_SIZE,
      ),
    );
  }, [enabled, events]);

  // Auto-dismiss expired toasts.
  useEffect(() => {
    if (queue.length === 0) return;
    const handle = window.setInterval(() => {
      const now = Date.now();
      setQueue((prev) => prev.filter((t) => now - t.shownAt < TOAST_TTL_MS));
    }, 500);
    return () => window.clearInterval(handle);
  }, [queue.length]);

  const visible = useMemo(() => queue.slice(0, MAX_VISIBLE), [queue]);

  if (!enabled || visible.length === 0) return null;

  return (
    <div
      aria-label="Migration events"
      aria-live="polite"
      className="fixed bottom-4 right-4 z-[90] flex flex-col-reverse gap-2 max-w-sm pointer-events-none"
    >
      {visible.map(({ event }) => (
        <button
          type="button"
          key={event.id}
          onClick={() =>
            dispatchMigrationFocus({
              from_kg: event.from_kg,
              to_owl: event.to_owl,
              edge_id: event.edge_id,
              event_id: event.id,
            })
          }
          className="pointer-events-auto text-left rounded-lg border border-cyan-400/40 bg-gradient-to-br from-amber-500/15 via-background/95 to-cyan-500/15 backdrop-blur shadow-xl px-3 py-2 hover:border-cyan-300 transition-colors"
        >
          <div className="flex items-center justify-between gap-2">
            <span className="text-xs uppercase tracking-wide text-amber-300">
              Migration
            </span>
            <span className="text-[10px] text-muted-foreground">
              {new Date(event.at).toLocaleTimeString()}
            </span>
          </div>
          <div className="mt-1 text-sm font-medium truncate">
            {event.summary ?? `${event.from_kg} -> ${event.to_owl}`}
          </div>
          <div className="mt-0.5 text-[11px] text-muted-foreground">
            confidence {Math.round((event.confidence ?? 0) * 100)}%
          </div>
          <div className="mt-1 text-[10px] text-cyan-300">click to focus graph</div>
        </button>
      ))}
      {queue.length > MAX_VISIBLE && (
        <div className="pointer-events-auto text-[10px] text-muted-foreground self-end">
          +{queue.length - MAX_VISIBLE} more
          <Button
            variant="ghost"
            size="sm"
            className="ml-2 h-6 px-2 text-[10px]"
            onClick={() => setQueue([])}
          >
            clear
          </Button>
        </div>
      )}
    </div>
  );
}

export default MigrationEventToast;
