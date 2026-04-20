/**
 * Feature flags (ADR-049 / ADR-051).
 *
 * Fetches server-side feature flags from `GET /api/features` and exposes a
 * typed React hook + synchronous accessor. Gates Sprint-3 UI surfaces:
 *
 *   - BRIDGE_EDGE_ENABLED    - render bridge promotion filaments + broker inbox
 *   - VISIBILITY_TRANSITIONS - enable publish/unpublish controls + tombstones
 *   - URN_SOLID_ALIGNMENT    - show pod URLs + Solid-backed metadata
 *
 * Flags are cached in module scope and fetched once per session; a manual
 * `refreshFeatureFlags()` is exposed for tests and debug tooling.
 */

import { useEffect, useState } from 'react';
import { apiFetch, ApiError } from '../utils/apiFetch';

export type FeatureFlagKey =
  | 'BRIDGE_EDGE_ENABLED'
  | 'VISIBILITY_TRANSITIONS'
  | 'URN_SOLID_ALIGNMENT';

export type FeatureFlags = Record<FeatureFlagKey, boolean>;

const DEFAULT_FLAGS: FeatureFlags = {
  BRIDGE_EDGE_ENABLED: false,
  VISIBILITY_TRANSITIONS: false,
  URN_SOLID_ALIGNMENT: false,
};

interface FeatureFlagsResponse {
  flags?: Partial<Record<FeatureFlagKey, boolean>>;
}

let cached: FeatureFlags = { ...DEFAULT_FLAGS };
let inflight: Promise<FeatureFlags> | null = null;
let lastFetchedAt: number | null = null;
const listeners = new Set<(flags: FeatureFlags) => void>();

function normalise(
  incoming: Partial<Record<FeatureFlagKey, boolean>> | undefined,
): FeatureFlags {
  const next = { ...DEFAULT_FLAGS };
  if (!incoming) return next;
  (Object.keys(next) as FeatureFlagKey[]).forEach((k) => {
    if (typeof incoming[k] === 'boolean') {
      next[k] = incoming[k] as boolean;
    }
  });
  return next;
}

function notify(): void {
  listeners.forEach((l) => {
    try {
      l(cached);
    } catch {
      // Swallow listener errors - flag notifications must not break the UI.
    }
  });
}

/**
 * Fetch flags from the backend, cache, and notify subscribers.
 * Subsequent concurrent callers share the in-flight promise.
 */
export async function fetchFeatureFlags(): Promise<FeatureFlags> {
  if (inflight) return inflight;
  inflight = (async () => {
    try {
      const data = await apiFetch<FeatureFlagsResponse>('/api/features');
      cached = normalise(data.flags);
      lastFetchedAt = Date.now();
      notify();
      return cached;
    } catch (err: unknown) {
      // Fail closed to defaults on network / 4xx / 5xx errors.
      if (err instanceof ApiError) {
        cached = { ...DEFAULT_FLAGS };
        notify();
      }
      return cached;
    } finally {
      inflight = null;
    }
  })();
  return inflight;
}

/** Force a refresh, bypassing the in-flight cache. */
export async function refreshFeatureFlags(): Promise<FeatureFlags> {
  inflight = null;
  return fetchFeatureFlags();
}

/** Synchronous accessor - returns the last cached value (or defaults). */
export function getFeatureFlags(): FeatureFlags {
  return cached;
}

/** Timestamp of the last successful fetch, or null. */
export function getFeatureFlagsFetchedAt(): number | null {
  return lastFetchedAt;
}

/**
 * React hook that returns the live flags, fetching once on mount and
 * re-rendering when the cache changes.
 */
export function useFeatureFlags(): FeatureFlags {
  const [flags, setFlags] = useState<FeatureFlags>(cached);

  useEffect(() => {
    let active = true;
    const onChange = (next: FeatureFlags) => {
      if (active) setFlags(next);
    };
    listeners.add(onChange);
    if (lastFetchedAt === null) {
      void fetchFeatureFlags().then((f) => {
        if (active) setFlags(f);
      });
    }
    return () => {
      active = false;
      listeners.delete(onChange);
    };
  }, []);

  return flags;
}

/** Convenience: one-flag hook. */
export function useFeatureFlag(key: FeatureFlagKey): boolean {
  const flags = useFeatureFlags();
  return flags[key];
}
