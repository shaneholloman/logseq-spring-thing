/**
 * useTransientBeams
 *
 * Read-side hook for the embodied agent-action beams fed by the 0x23
 * AGENT_ACTION frame (decoded in store/websocket/binaryProtocol.ts and pushed
 * into transientBeamStore). Returns the live beam list plus a `prune` callback
 * that the render layer calls every frame to age out expired beams.
 *
 * The store is the single source of truth — this hook adds no local state, so
 * pushes from the websocket worker path and reads from the R3F render path
 * stay coherent without duplication.
 */

import { useCallback } from 'react';
import { useTransientBeamStore, TransientBeam } from '@/store/transientBeamStore';

export interface UseTransientBeamsResult {
  /** Current live beams (already FIFO-capped by the store). */
  beams: TransientBeam[];
  /** Drop beams past their TTL. Call once per frame from the render layer. */
  prune: () => void;
}

export function useTransientBeams(): UseTransientBeamsResult {
  const beams = useTransientBeamStore(state => state.beams);
  const pruneExpired = useTransientBeamStore(state => state.pruneExpired);

  const prune = useCallback(() => {
    pruneExpired();
  }, [pruneExpired]);

  return { beams, prune };
}

export default useTransientBeams;
