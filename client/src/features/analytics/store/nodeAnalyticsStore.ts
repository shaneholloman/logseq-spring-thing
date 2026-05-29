import { getActualNodeId } from '../../../types/binaryProtocol';
import type { BinaryNodeData } from '../../../types/binaryProtocol';
import { useSettingsStore } from '../../../store/settingsStore';

/**
 * Main-thread per-node analytics store (ADR-03 D7 "Phase 5").
 *
 * The graph worker no longer owns analytics. cluster_id / anomaly_score /
 * community_id ride every V3 position frame (wire offsets 36/40/44) and are
 * decoded on the main thread in the websocket receive path. This singleton
 * ingests them keyed by masked node id, and exposes a render-index-aligned
 * Float32Array (stride 3: [clusterId, anomalyScore, communityId]) that
 * GemNodes and ClusterHulls consume — exactly the contract the removed
 * graphWorkerProxy.getAnalyticsBuffer() used to satisfy.
 */

interface AnalyticsTriple {
  cluster: number;
  anomaly: number;
  community: number;
}

class NodeAnalyticsStore {
  private byMaskedId = new Map<number, AnalyticsTriple>();
  private version = 0;
  private hasAnalytics = false;

  // Render-index-aligned buffer cache (rebuilt on version/map change only).
  private cachedBuffer: Float32Array | null = null;
  private cachedMapRef: Map<string, number> | null = null;
  private cachedVersion = -1;
  private cachedSize = -1;

  /**
   * Ingest a decoded V3 frame. No-op (cheap) when no analytics consumer is
   * active and no analytics have ever been seen, so the common case (no
   * clustering run) costs only a settings read, not a full node scan.
   */
  ingest(nodes: BinaryNodeData[]): void {
    if (!this.hasAnalytics && !NodeAnalyticsStore.anyConsumerEnabled()) return;

    let sawAnalytics = false;
    for (const n of nodes) {
      const cluster = n.clusterId ?? 0;
      const anomaly = n.anomalyScore ?? 0;
      const community = n.communityId ?? 0;
      if (cluster > 0 || community > 0 || anomaly > 0.0001) sawAnalytics = true;

      const key = getActualNodeId(n.nodeId);
      let t = this.byMaskedId.get(key);
      if (!t) {
        t = { cluster: 0, anomaly: 0, community: 0 };
        this.byMaskedId.set(key, t);
      }
      t.cluster = cluster;
      t.anomaly = anomaly;
      t.community = community;
    }

    if (sawAnalytics) this.hasAnalytics = true;
    this.version++;
  }

  hasData(): boolean {
    return this.hasAnalytics;
  }

  /**
   * Return a Float32Array indexed by render array position (the same index
   * space as nodePositionsRef / nodeIdToIndexMap), stride 3. Returns null when
   * no analytics have been observed so callers transparently fall back to their
   * domain heuristics. Cached by (map identity, version) to keep the per-tick
   * call cheap.
   */
  getIndexedBuffer(nodeIdToIndexMap: Map<string, number>): Float32Array | null {
    if (!this.hasAnalytics || nodeIdToIndexMap.size === 0) return null;

    if (
      this.cachedBuffer &&
      this.cachedMapRef === nodeIdToIndexMap &&
      this.cachedVersion === this.version &&
      this.cachedSize === nodeIdToIndexMap.size
    ) {
      return this.cachedBuffer;
    }

    let maxIndex = 0;
    nodeIdToIndexMap.forEach((idx) => {
      if (idx > maxIndex) maxIndex = idx;
    });

    const buf = new Float32Array((maxIndex + 1) * 3);
    nodeIdToIndexMap.forEach((idx, idStr) => {
      const numeric = parseInt(idStr, 10);
      if (Number.isNaN(numeric)) return;
      const t = this.byMaskedId.get(getActualNodeId(numeric));
      if (!t) return;
      const base = idx * 3;
      buf[base] = t.cluster;
      buf[base + 1] = t.anomaly;
      buf[base + 2] = t.community;
    });

    this.cachedBuffer = buf;
    this.cachedMapRef = nodeIdToIndexMap;
    this.cachedVersion = this.version;
    this.cachedSize = nodeIdToIndexMap.size;
    return buf;
  }

  clear(): void {
    this.byMaskedId.clear();
    this.hasAnalytics = false;
    this.version++;
    this.cachedBuffer = null;
    this.cachedMapRef = null;
    this.cachedVersion = -1;
    this.cachedSize = -1;
  }

  /** Dev-only snapshot for diagnostics. */
  stats(): {
    hasAnalytics: boolean;
    nodeCount: number;
    clusteredCount: number;
    distinctClusters: number;
    version: number;
  } {
    let clusteredCount = 0;
    const distinct = new Set<number>();
    this.byMaskedId.forEach((t) => {
      if (t.cluster > 0) {
        clusteredCount++;
        distinct.add(t.cluster);
      }
    });
    return {
      hasAnalytics: this.hasAnalytics,
      nodeCount: this.byMaskedId.size,
      clusteredCount,
      distinctClusters: distinct.size,
      version: this.version,
    };
  }

  private static anyConsumerEnabled(): boolean {
    const s = useSettingsStore.getState().settings as
      | { qualityGates?: { showClusters?: boolean; showAnomalies?: boolean; showCommunities?: boolean }; visualisation?: { clusterHulls?: { enabled?: boolean } } }
      | undefined;
    const qg = s?.qualityGates;
    return Boolean(
      qg?.showClusters ||
        qg?.showAnomalies ||
        qg?.showCommunities ||
        s?.visualisation?.clusterHulls?.enabled,
    );
  }
}

export const nodeAnalyticsStore = new NodeAnalyticsStore();

if (import.meta.env.DEV && typeof window !== 'undefined') {
  (window as unknown as { __nodeAnalyticsStore?: NodeAnalyticsStore }).__nodeAnalyticsStore =
    nodeAnalyticsStore;
}
