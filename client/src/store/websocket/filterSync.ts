/**
 * filterSync.ts — Client filter state synchronization
 *
 * Handles: filter subscription, filter update messages, node filter
 * settings sync over WebSocket.
 */

import { createLogger } from '../../utils/loggerConfig';
import { useSettingsStore } from '../settingsStore';
import { graphDataManager } from '../../features/graph/managers/graphDataManager';
import type { FilterUpdateParams, FilterSnapshot } from './types';

const logger = createLogger('WebSocketStore');

// ── Encapsulated module-level state ────────────────────────────────────
let filterSubscriptionSet = false;
let filterUnsubscribers: (() => void)[] = [];
let lastFilterSnapshot: FilterSnapshot | null = null;

// ── State accessors (used by index.ts for _reset) ──

export function resetFilterState() {
  filterUnsubscribers.forEach(unsub => { try { unsub(); } catch (_) { /* ignore */ } });
  filterUnsubscribers = [];
  filterSubscriptionSet = false;
  lastFilterSnapshot = null;
}

export function cleanupFilterSubscriptions() {
  filterUnsubscribers.forEach(unsub => { try { unsub(); } catch (_) { /* ignore */ } });
  filterUnsubscribers = [];
  filterSubscriptionSet = false;
}

export function clearFilterSnapshot() {
  lastFilterSnapshot = null;
}

// ── Filter subscription setup ──────────────────────────────────────────

export function setupFilterSubscription(
  get: () => { isConnected: boolean; sendFilterUpdate: (filter: FilterUpdateParams) => void },
) {
  if (filterSubscriptionSet) return;
  filterSubscriptionSet = true;

  const filterPaths = [
    'nodeFilter.enabled',
    'nodeFilter.qualityThreshold',
    'nodeFilter.authorityThreshold',
    'nodeFilter.filterByQuality',
    'nodeFilter.filterByAuthority',
    'nodeFilter.filterMode',
  ] as const;

  filterPaths.forEach(path => {
    const store = useSettingsStore.getState();
    if (store.subscribe) {
      const unsub = store.subscribe(path as Parameters<typeof store.subscribe>[0], () => {
        handleFilterChange(get);
      });
      filterUnsubscribers.push(unsub);
    }
  });

  const zustandUnsub = useSettingsStore.subscribe((state) => {
    const nodeFilter = state.settings?.nodeFilter;
    const wsState = get();
    if (nodeFilter && wsState.isConnected) {
      const prev = lastFilterSnapshot;
      if (
        !prev ||
        prev.enabled !== nodeFilter.enabled ||
        prev.qualityThreshold !== nodeFilter.qualityThreshold ||
        prev.authorityThreshold !== nodeFilter.authorityThreshold ||
        prev.filterByQuality !== nodeFilter.filterByQuality ||
        prev.filterByAuthority !== nodeFilter.filterByAuthority ||
        prev.filterMode !== nodeFilter.filterMode
      ) {
        lastFilterSnapshot = {
          enabled: nodeFilter.enabled,
          qualityThreshold: nodeFilter.qualityThreshold,
          authorityThreshold: nodeFilter.authorityThreshold,
          filterByQuality: nodeFilter.filterByQuality,
          filterByAuthority: nodeFilter.filterByAuthority,
          filterMode: nodeFilter.filterMode,
        };
        wsState.sendFilterUpdate(lastFilterSnapshot);
      }
    }
  });
  filterUnsubscribers.push(zustandUnsub);

  logger.info('Filter subscription set up - changes will sync to server');
}

function handleFilterChange(
  get: () => { isConnected: boolean; sendFilterUpdate: (filter: FilterUpdateParams) => void },
) {
  const state = get();
  if (!state.isConnected) return;

  const nodeFilter = useSettingsStore.getState().settings?.nodeFilter;
  if (nodeFilter) {
    state.sendFilterUpdate({
      enabled: nodeFilter.enabled,
      qualityThreshold: nodeFilter.qualityThreshold,
      authorityThreshold: nodeFilter.authorityThreshold,
      filterByQuality: nodeFilter.filterByQuality,
      filterByAuthority: nodeFilter.filterByAuthority,
      filterMode: nodeFilter.filterMode,
    });
  }
}

// ── Force refresh ──────────────────────────────────────────────────────

export async function forceRefreshFilter(
  get: () => { isConnected: boolean; sendFilterUpdate: (filter: FilterUpdateParams) => void },
) {
  const state = get();
  if (!state.isConnected) {
    logger.warn('Cannot force refresh filter: WebSocket not connected');
    return;
  }

  const nodeFilter = useSettingsStore.getState().settings?.nodeFilter;
  if (nodeFilter) {
    lastFilterSnapshot = null;

    logger.info('[Refresh] Clearing local graph and requesting fresh filtered data', nodeFilter);

    await graphDataManager.setGraphData({ nodes: [], edges: [] });
    logger.info('[Refresh] Local graph cleared, awaiting server response...');

    state.sendFilterUpdate({
      enabled: nodeFilter.enabled,
      qualityThreshold: nodeFilter.qualityThreshold,
      authorityThreshold: nodeFilter.authorityThreshold,
      filterByQuality: nodeFilter.filterByQuality,
      filterByAuthority: nodeFilter.filterByAuthority,
      filterMode: nodeFilter.filterMode,
    });
  } else {
    logger.warn('No nodeFilter settings found in store');
  }
}
