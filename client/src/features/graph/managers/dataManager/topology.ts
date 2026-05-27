import { createLogger } from '../../../../utils/loggerConfig';
import { debugState } from '../../../../utils/clientDebugState';
import { stringToU32 } from '../../../../types/idMapping';
import { startTransition } from 'react';
import type { GraphData, Node } from '../graphWorkerProxy';

const logger = createLogger('GraphDataManager.topology');

export type GraphDataChangeListener = (data: GraphData) => void;

/**
 * Cheap topology hash — collapses REST/WebSocket/retry duplicate deliveries.
 * Uses nodeCount + edgeCount + first/last node id (ADR-03 D5).
 */
export function topologyHash(g: GraphData): string {
  const nodes = g.nodes || [];
  const edges = g.edges || [];
  const first = nodes.length > 0 ? String(nodes[0].id) : '';
  const last  = nodes.length > 0 ? String(nodes[nodes.length - 1].id) : '';
  return `${nodes.length}-${edges.length}-${first}-${last}`;
}

/**
 * Build/rebuild both nodeIdMap (string→number) and reverseNodeIdMap (number→string)
 * from a validated node list.
 */
export function buildNodeIdMaps(
  nodes: Node[],
  nodeIdMap: Map<string, number>,
  reverseNodeIdMap: Map<number, string>,
): void {
  nodeIdMap.clear();
  reverseNodeIdMap.clear();

  for (const node of nodes) {
    const numericId = parseInt(node.id, 10);
    if (!isNaN(numericId) && numericId >= 0 && numericId <= 0xFFFFFFFF) {
      nodeIdMap.set(node.id, numericId);
      reverseNodeIdMap.set(numericId, node.id);
    } else {
      let mappedId = stringToU32(node.id);
      while (reverseNodeIdMap.has(mappedId) && reverseNodeIdMap.get(mappedId) !== node.id) {
        mappedId = (mappedId + 1) >>> 0;
      }
      nodeIdMap.set(node.id, mappedId);
      reverseNodeIdMap.set(mappedId, node.id);
    }
  }
}

/**
 * Insert or update a single node's entry in both maps.
 * Used by addNode without a full rebuild.
 */
export function upsertNodeIdEntry(
  node: Node,
  nodeIdMap: Map<string, number>,
  reverseNodeIdMap: Map<number, string>,
): void {
  const numericId = parseInt(node.id, 10);
  if (!isNaN(numericId)) {
    nodeIdMap.set(node.id, numericId);
    reverseNodeIdMap.set(numericId, node.id);
  } else {
    let mappedId = stringToU32(node.id);
    while (reverseNodeIdMap.has(mappedId) && reverseNodeIdMap.get(mappedId) !== node.id) {
      mappedId = (mappedId + 1) >>> 0;
    }
    nodeIdMap.set(node.id, mappedId);
    reverseNodeIdMap.set(mappedId, node.id);
  }
}

/**
 * ADR-03 D5 single delivery path.
 *
 * If the incoming topology hash matches the cached hash the call is a no-op.
 * Otherwise updates `lastGraphData` + `lastGraphDataHash` and fires all
 * subscribers via `queueMicrotask` (one notification per genuine change).
 *
 * Returns the new hash (or the existing hash when the call was a no-op) so
 * callers can update their state record.
 */
export function setDataAndNotify(
  incoming: GraphData,
  cachedHash: string | null,
  listeners: Set<GraphDataChangeListener>,
  onUpdate: (data: GraphData, hash: string) => void,
): void {
  const hash = topologyHash(incoming);
  if (hash === cachedHash) {
    return; // dedup short-circuit
  }

  onUpdate(incoming, hash);

  const snapshot = incoming;
  queueMicrotask(() => {
    listeners.forEach(listener => {
      try {
        startTransition(() => {
          listener(snapshot);
        });
      } catch (error) {
        logger.error('Error in graph data listener:', error);
      }
    });
  });
}

/** Deliver cached snapshot to a newly registered listener (deferred). */
export function replayToListener(
  listener: GraphDataChangeListener,
  lastGraphData: GraphData | null,
): void {
  if (!lastGraphData) return;
  const snapshot = lastGraphData;
  queueMicrotask(() => {
    try {
      listener(snapshot);
    } catch (error) {
      logger.error('Error in initial graph data listener:', error);
    }
  });
}
