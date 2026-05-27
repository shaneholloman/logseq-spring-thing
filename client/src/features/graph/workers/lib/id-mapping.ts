/**
 * Node ID hashing utilities for the graph worker.
 * Maps string node IDs to compact u32 wire IDs via quadratic probing.
 */
import { stringToU32 } from '../../../../types/idMapping';

const MAX_HASH_PROBES = 1000;

/**
 * Find a free slot in reverseNodeIdMap for the given nodeId using
 * quadratic probing to resolve collisions.
 * @throws if the probe limit is exceeded (hash collision storm)
 */
export function findFreeMappedId(nodeId: string, reverseNodeIdMap: Map<number, string>): number {
  let h = stringToU32(nodeId);
  let probe = 0;
  while (reverseNodeIdMap.has(h) && reverseNodeIdMap.get(h) !== nodeId && probe < MAX_HASH_PROBES) {
    probe += 1;
    h = (h + probe * probe) >>> 0;
  }
  if (reverseNodeIdMap.has(h) && reverseNodeIdMap.get(h) !== nodeId) {
    throw new Error(`Hash collision limit exceeded for node '${nodeId}'`);
  }
  return h;
}
