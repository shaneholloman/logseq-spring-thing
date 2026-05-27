/**
 * Client-side graph analytics: anomaly scoring and community detection.
 *
 * These run only when the server has not populated analytics data in the binary
 * V3 protocol. Both algorithms write directly into a caller-supplied
 * Float32Array (analyticsBuffer) indexed as [clusterId, anomalyScore, communityId]
 * per node — zero allocation inside the hot path.
 */
import { workerLogger } from './logger';
import { GraphData } from './types';

/**
 * Compute anomaly scores using degree-based z-score outlier detection.
 * Writes to analyticsBuffer[i*3 + 1] for each node.
 */
export function computeAnomalyScores(analyticsBuffer: Float32Array, graphData: GraphData): void {
  if (graphData.nodes.length === 0) return;

  // Build degree map
  const degreeMap = new Map<string, number>();
  for (const node of graphData.nodes) {
    degreeMap.set(node.id, 0);
  }
  for (const edge of graphData.edges) {
    degreeMap.set(edge.source, (degreeMap.get(edge.source) ?? 0) + 1);
    degreeMap.set(edge.target, (degreeMap.get(edge.target) ?? 0) + 1);
  }

  // Compute mean and stddev
  const degrees = Array.from(degreeMap.values());
  const mean = degrees.reduce((a, b) => a + b, 0) / (degrees.length || 1);
  const variance = degrees.reduce((a, d) => a + (d - mean) ** 2, 0) / (degrees.length || 1);
  const stddev = Math.sqrt(variance) || 1; // avoid division by zero

  // Assign z-score normalized to 0–1
  for (let i = 0; i < graphData.nodes.length; i++) {
    const degree = degreeMap.get(graphData.nodes[i].id) ?? 0;
    const zScore = Math.abs(degree - mean) / stddev;
    // z-score of 3+ maps to 1.0
    analyticsBuffer[i * 3 + 1] = Math.min(zScore / 3, 1.0);
  }
}

/**
 * Compute communities using simplified Louvain modularity optimisation.
 * Writes to analyticsBuffer[i*3 + 2] for each node (1-based community ID).
 */
export function computeCommunities(analyticsBuffer: Float32Array, graphData: GraphData): void {
  if (graphData.nodes.length === 0) return;

  const n = graphData.nodes.length;
  const nodeIndex = new Map<string, number>();
  for (let i = 0; i < n; i++) {
    nodeIndex.set(graphData.nodes[i].id, i);
  }

  // Build adjacency (all weight=1 for unweighted)
  const adj: number[][] = Array.from({ length: n }, () => []);
  let totalEdges = 0;
  for (const edge of graphData.edges) {
    const si = nodeIndex.get(edge.source);
    const ti = nodeIndex.get(edge.target);
    if (si !== undefined && ti !== undefined && si !== ti) {
      adj[si].push(ti);
      adj[ti].push(si);
      totalEdges++;
    }
  }

  if (totalEdges === 0) {
    // No edges — each node is its own community
    for (let i = 0; i < n; i++) {
      analyticsBuffer[i * 3 + 2] = i + 1;
    }
    return;
  }

  const m2 = totalEdges * 2; // 2 * number of edges (each counted once above)
  const degree = adj.map(neighbors => neighbors.length);

  // Initialise: each node in its own community
  const community = new Array<number>(n);
  for (let i = 0; i < n; i++) community[i] = i;

  // Community total degree (sum of degrees of all nodes in community)
  const communityTotalDegree = new Float64Array(n);
  for (let i = 0; i < n; i++) {
    communityTotalDegree[i] = degree[i];
  }

  // Iterative optimisation (max 10 passes)
  const MAX_PASSES = 10;
  for (let pass = 0; pass < MAX_PASSES; pass++) {
    let moved = false;

    for (let i = 0; i < n; i++) {
      const currentComm = community[i];
      const ki = degree[i];
      if (ki === 0) continue; // isolated node

      // Count edges to each neighbouring community
      const neighborComms = new Map<number, number>();
      let edgesToCurrent = 0;
      for (const j of adj[i]) {
        const cj = community[j];
        neighborComms.set(cj, (neighborComms.get(cj) ?? 0) + 1);
        if (cj === currentComm) edgesToCurrent++;
      }

      // Try moving to best neighbour community
      let bestComm = currentComm;
      let bestGain = 0;

      // Modularity gain of removing i from current community
      const removeLoss = edgesToCurrent - (ki * (communityTotalDegree[currentComm] - ki)) / m2;

      for (const [comm, edgesToComm] of neighborComms) {
        if (comm === currentComm) continue;
        // Modularity gain of adding i to comm
        const addGain = edgesToComm - (ki * communityTotalDegree[comm]) / m2;
        const totalGain = addGain - removeLoss;
        if (totalGain > bestGain) {
          bestGain = totalGain;
          bestComm = comm;
        }
      }

      if (bestComm !== currentComm && bestGain > 1e-10) {
        communityTotalDegree[currentComm] -= ki;
        communityTotalDegree[bestComm] += ki;
        community[i] = bestComm;
        moved = true;
      }
    }

    if (!moved) break;
  }

  // Renumber communities to 1-based contiguous IDs
  const commMap = new Map<number, number>();
  let nextId = 1;
  for (let i = 0; i < n; i++) {
    const c = community[i];
    if (!commMap.has(c)) {
      commMap.set(c, nextId++);
    }
    analyticsBuffer[i * 3 + 2] = commMap.get(c)!;
  }
}

/**
 * Recompute all client-side analytics (anomaly + community detection).
 * Only runs when the server hasn't already populated analytics data (i.e., all
 * buffer entries are zero).
 */
export function recomputeAnalytics(analyticsBuffer: Float32Array, graphData: GraphData): void {
  // Check if server already populated analytics (any non-zero value in buffer)
  let hasServerData = false;
  for (let i = 0; i < analyticsBuffer.length; i += 3) {
    if (analyticsBuffer[i] > 0 || analyticsBuffer[i + 1] > 0 || analyticsBuffer[i + 2] > 0) {
      hasServerData = true;
      break;
    }
  }

  if (!hasServerData) {
    workerLogger.info('Computing client-side analytics (anomaly + community detection)');
    computeAnomalyScores(analyticsBuffer, graphData);
    computeCommunities(analyticsBuffer, graphData);
  }
}
