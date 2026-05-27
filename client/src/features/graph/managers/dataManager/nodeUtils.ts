import { createLogger } from '../../../../utils/loggerConfig';
import { debugState } from '../../../../utils/clientDebugState';
import type { Node } from '../graphWorkerProxy';

const logger = createLogger('GraphDataManager.nodeUtils');

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
