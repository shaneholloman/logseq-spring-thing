import { graphDataManager } from '../../features/graph/managers/graphDataManager';
import { graphWorkerProxy } from '../../features/graph/managers/graphWorkerProxy';
import type { GraphData } from '../../features/graph/managers/graphWorkerProxy';
import type { GraphDataPort } from './GraphDataPort';

export class GraphDataAdapter implements GraphDataPort {
  onGraphDataChange(cb: (data: GraphData) => void): () => void {
    return graphDataManager.onGraphDataChange(cb);
  }

  onPositionUpdate(cb: (positions: Float32Array) => void): () => void {
    return graphDataManager.onPositionUpdate((positions: Float32Array) => {
      cb(positions instanceof Float32Array ? positions : new Float32Array(positions));
    });
  }

  getGraphData(): Promise<GraphData | null> {
    return graphDataManager.getGraphData();
  }

  getNodeNumericId(nodeId: string): number | undefined {
    return graphDataManager.nodeIdMap.get(nodeId);
  }

  pinNode(numericId: number): void {
    graphWorkerProxy.pinNode(numericId);
  }

  unpinNode(numericId: number): void {
    graphWorkerProxy.unpinNode(numericId);
  }

  updateNodePosition(numericId: number, position: { x: number; y: number; z: number }): void {
    graphWorkerProxy.updateUserDrivenNodePosition(numericId, position);
  }
}

export const graphDataPort = new GraphDataAdapter();
