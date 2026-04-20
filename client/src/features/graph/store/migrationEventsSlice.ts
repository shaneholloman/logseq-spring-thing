/**
 * Migration events slice (ADR-048 - Sprint 3).
 *
 * Ring-buffer (size 50) of promotion events emitted by the backend migration
 * event stream. Powers `MigrationEventToast` and the amber->cyan filament pulse
 * in `GraphManager`. Supports a WebSocket transport with a polling fallback.
 */

import { create } from 'zustand';
import { apiFetch, ApiError } from '../../../utils/apiFetch';
import type { BridgePromotionEvent } from '../types/graphTypes';

/** Maximum number of events retained in memory. */
export const MIGRATION_EVENT_BUFFER_SIZE = 50;

interface EventsResponse {
  events: BridgePromotionEvent[];
}

export interface MigrationEventsState {
  /** Most recent events first. */
  events: BridgePromotionEvent[];
  /** Ids already ingested (dedupe guard). */
  seenIds: Set<string>;
  /** Timestamp of the latest event ingested. */
  lastEventAt: number | null;
  /** Fallback polling cursor (most recent seen id). */
  cursor: string | null;
  /** WebSocket connection state. */
  socketState: 'idle' | 'connecting' | 'open' | 'closed' | 'error';

  /** Ingest a single event (no-op if already seen). */
  ingest: (event: BridgePromotionEvent) => void;
  /** Bulk ingest (e.g. polling response). */
  ingestMany: (events: BridgePromotionEvent[]) => void;
  /** Mark connection state (used by the WebSocket subscriber). */
  setSocketState: (state: MigrationEventsState['socketState']) => void;
  /** Drain: clear all events (used by tests / logout). */
  clear: () => void;
  /** Fallback polling driver. */
  pollOnce: () => Promise<void>;
}

export const useMigrationEventsStore = create<MigrationEventsState>((set, get) => ({
  events: [],
  seenIds: new Set<string>(),
  lastEventAt: null,
  cursor: null,
  socketState: 'idle',

  ingest: (event) => {
    if (!event || !event.id) return;
    const state = get();
    if (state.seenIds.has(event.id)) return;
    const nextSeen = new Set(state.seenIds);
    nextSeen.add(event.id);
    const nextEvents = [event, ...state.events].slice(0, MIGRATION_EVENT_BUFFER_SIZE);
    // Trim the seen set if it outgrew the buffer to avoid unbounded growth.
    if (nextSeen.size > MIGRATION_EVENT_BUFFER_SIZE * 4) {
      const keep = new Set<string>();
      nextEvents.forEach((e) => keep.add(e.id));
      set({
        events: nextEvents,
        seenIds: keep,
        lastEventAt: Date.now(),
        cursor: event.id,
      });
      return;
    }
    set({
      events: nextEvents,
      seenIds: nextSeen,
      lastEventAt: Date.now(),
      cursor: event.id,
    });
  },

  ingestMany: (events) => {
    events.forEach((e) => get().ingest(e));
  },

  setSocketState: (socketState) => set({ socketState }),

  clear: () =>
    set({
      events: [],
      seenIds: new Set<string>(),
      lastEventAt: null,
      cursor: null,
    }),

  pollOnce: async () => {
    try {
      const { cursor } = get();
      const qs = cursor ? `?since=${encodeURIComponent(cursor)}` : '';
      const data = await apiFetch<EventsResponse>(`/api/bridge/events${qs}`);
      if (Array.isArray(data.events) && data.events.length > 0) {
        // Reverse so oldest is ingested first; ring buffer keeps newest on top.
        const ordered = [...data.events].reverse();
        get().ingestMany(ordered);
      }
    } catch (err: unknown) {
      // Silent: polling is best-effort. Errors surface via socketState.
      if (err instanceof ApiError) {
        set({ socketState: 'error' });
      }
    }
  },
}));
