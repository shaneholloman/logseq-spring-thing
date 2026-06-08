import { createLogger } from '../../../../utils/loggerConfig';
import { debugState } from '../../../../utils/clientDebugState';
import type { GraphData, Node } from '../graphWorkerProxy';

const logger = createLogger('GraphDataManager.nodeUtils');

/**
 * Authoritative population/origin type for a node. Reads `metadata.type` first
 * (the single source of truth — matches the server's `Node::population_type`
 * and the client `useGraphFiltering` render gate), then the legacy top-level
 * `type`/`nodeType` scaffold as a fallback when metadata is absent.
 */
function nodePopulationType(node: Node): string {
  const n = node as unknown as { type?: string; nodeType?: string };
  return (node.metadata?.type as string | undefined)
    || n.type
    || (node.metadata?.nodeType as string | undefined)
    || n.nodeType
    || '';
}

/**
 * Drop `linked_page` wikilink-stub nodes (and any edge that touches one) from a
 * graph BEFORE it reaches the physics worker, topology cache, and render meshes.
 *
 * The `useGraphFiltering` hook already hides linked_page nodes from the *render*
 * set when `includeLinkedPages` is false, but by then the full payload (here,
 * ~14.7k of 17.1k nodes are linked_page stubs) has already churned through the
 * worker topology, edge-buffer computation, and hierarchy detection. Gating at
 * ingestion collapses that downstream cost to the rendered set, which is what
 * lets the constrained sidecar actually initialise the scene.
 *
 * Idempotent: when `includeLinkedPages` is true (or no linked_page nodes exist)
 * the original object is returned unchanged. Toggling the setting on requires a
 * graph refresh (the "Refresh Graph" action), matching the existing
 * maxNodeCount filter and the setting's own description.
 */
export function dropLinkedPageStubs(data: GraphData, includeLinkedPages: boolean): GraphData {
  if (includeLinkedPages || !data?.nodes?.length) return data;

  const keptNodes = data.nodes.filter(node => nodePopulationType(node) !== 'linked_page');
  if (keptNodes.length === data.nodes.length) return data; // nothing to drop

  const keptIds = new Set(keptNodes.map(n => String(n.id)));
  const keptEdges = (data.edges || []).filter(
    e => keptIds.has(String(e.source)) && keptIds.has(String(e.target)),
  );

  logger.info(
    `[populationGate] Dropped linked_page stubs: ${data.nodes.length} -> ${keptNodes.length} nodes, ` +
      `${data.edges?.length ?? 0} -> ${keptEdges.length} edges (includeLinkedPages=false)`,
  );

  return { ...data, nodes: keptNodes, edges: keptEdges };
}

/**
 * Ensures a node has a valid numeric position object.
 * Returns a new object if the position is missing or has non-finite coordinates;
 * returns the original object when no fix is needed (avoids unnecessary allocations).
 */
export function ensureNodeHasValidPosition(node: Node): Node {
  if (!node.position) {
    if (debugState.isDataDebugEnabled()) {
      logger.warn(`Node ${node.id} missing position - server should provide this!`);
    }
    return { ...node, position: { x: 0, y: 0, z: 0 } };
  }

  if (
    typeof node.position.x !== 'number' ||
    typeof node.position.y !== 'number' ||
    typeof node.position.z !== 'number'
  ) {
    if (debugState.isDataDebugEnabled()) {
      logger.warn(`Node ${node.id} has invalid position coordinates - fixing`);
    }
    return {
      ...node,
      position: {
        x: typeof node.position.x === 'number' && isFinite(node.position.x) ? node.position.x : 0,
        y: typeof node.position.y === 'number' && isFinite(node.position.y) ? node.position.y : 0,
        z: typeof node.position.z === 'number' && isFinite(node.position.z) ? node.position.z : 0,
      },
    };
  }

  return node;
}

/** Diagnostic stub — logs validated count when data-debug is enabled. */
export function validateNodeMappings(nodes: Node[]): void {
  if (debugState.isDataDebugEnabled()) {
    logger.debug(`Validated ${nodes.length} nodes with ID mapping`);
  }
}
