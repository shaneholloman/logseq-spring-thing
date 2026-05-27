import { createLogger } from '../../../../utils/loggerConfig';
import type { GraphData } from '../graphWorkerProxy';

export type GraphDataChangeListener = (data: GraphData) => void;
export type PositionUpdateListener  = (positions: Float32Array) => void;

const logger = createLogger('GraphDataManager.listeners');

/**
 * Subscription registry for graph-data and position-update listeners.
 *
 * Uses Set semantics — duplicate registrations of the same callback are
 * coalesced automatically (ADR-03 D5).
 */
export class ListenerRegistry {
  private graphDataListeners:    Set<GraphDataChangeListener> = new Set();
  private positionUpdateListeners: Set<PositionUpdateListener> = new Set();

  /**
   * Register a graph-data change callback.
   *
   * If this is the first registration of `listener` and `lastGraphData` is
   * available, the snapshot is delivered via `queueMicrotask` so that
   * subscribe + initial-fire don't race within a single synchronous tick.
   *
   * Returns an unsubscribe function.
   */
  onGraphDataChange(
    listener: GraphDataChangeListener,
    lastGraphData: GraphData | null,
  ): () => void {
    const alreadyRegistered = this.graphDataListeners.has(listener);
    this.graphDataListeners.add(listener);

    if (!alreadyRegistered && lastGraphData) {
      const snapshot = lastGraphData;
      queueMicrotask(() => {
        try {
          listener(snapshot);
        } catch (error) {
          logger.error('Error in initial graph data listener:', error);
        }
      });
    }

    return () => { this.graphDataListeners.delete(listener); };
  }

  /** Register a position-update callback. Returns an unsubscribe function. */
  onPositionUpdate(listener: PositionUpdateListener): () => void {
    this.positionUpdateListeners.add(listener);
    return () => { this.positionUpdateListeners.delete(listener); };
  }

  /** Deliver a positions buffer to all registered position-update listeners. */
  notifyPositionUpdateListeners(positions: Float32Array): void {
    this.positionUpdateListeners.forEach(listener => {
      try {
        listener(positions);
      } catch (error) {
        logger.error('Error in position update listener:', error);
      }
    });
  }

  get graphDataSet(): Set<GraphDataChangeListener> {
    return this.graphDataListeners;
  }

  clear(): void {
    this.graphDataListeners.clear();
    this.positionUpdateListeners.clear();
  }
}
