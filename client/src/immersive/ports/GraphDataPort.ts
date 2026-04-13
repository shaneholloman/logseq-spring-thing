import type { GraphData } from '../../features/graph/managers/graphWorkerProxy';

export interface GraphDataPort {
  /** Subscribe to graph data changes. Returns unsubscribe function. */
  onGraphDataChange(cb: (data: GraphData) => void): () => void;

  /** Subscribe to position updates. Returns unsubscribe function. */
  onPositionUpdate(cb: (positions: Float32Array) => void): () => void;

  /** Get current graph data */
  getGraphData(): Promise<GraphData | null>;

  /** Get numeric ID for a node string ID */
  getNodeNumericId(nodeId: string): number | undefined;

  /** Pin a node in the physics simulation */
  pinNode(numericId: number): void;

  /** Unpin a node */
  unpinNode(numericId: number): void;

  /** Update a node's position (user-driven drag) */
  updateNodePosition(numericId: number, position: { x: number; y: number; z: number }): void;
}
