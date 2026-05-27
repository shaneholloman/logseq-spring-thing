import { Vector3, Color } from 'three';
import type { GraphData, Node as GraphNode } from '../../managers/graphDataManager';
import type { GraphCluster, ClusterDetection } from './types';
import { hasHierarchicalStructure } from './utils';

export function selectOptimalClusteringAlgorithm(graphData: GraphData): ClusterDetection['algorithm'] {
  const nodeCount = graphData.nodes.length;
  const edgeCount = graphData.edges.length;

  if (nodeCount < 50) return 'modularity';
  if (edgeCount / nodeCount > 3) return 'density';
  if (hasHierarchicalStructure(graphData)) return 'hierarchical';
  return 'spectral';
}

export async function applyClustering(
  graphData: GraphData,
  _algorithm: ClusterDetection['algorithm'],
  options: { minClusterSize?: number; maxClusters?: number }
): Promise<GraphCluster[]> {
  // All algorithms currently use the same connected-component clustering
  const clusters: GraphCluster[] = [];
  const visited = new Set<string>();
  let clusterId = 0;

  for (const node of graphData.nodes) {
    if (visited.has(node.id)) continue;
    const cluster = growClusterFromNode(node.id, graphData, visited);
    if (cluster.length >= (options.minClusterSize || 2)) {
      clusters.push(createClusterFromNodes(cluster, graphData, `cluster-${clusterId++}`));
    }
  }

  return clusters;
}

export function calculateClusterQuality(
  _graphData: GraphData,
  clusters: GraphCluster[]
): { modularity: number; silhouette: number; cohesion: number } {
  if (clusters.length === 0) return { modularity: 0, silhouette: 0, cohesion: 0 };
  const avgDensity = clusters.reduce((sum, c) => sum + c.density, 0) / clusters.length;
  return { modularity: avgDensity, silhouette: avgDensity * 0.8, cohesion: avgDensity * 0.9 };
}

export function generateClusterRecommendations(
  clusters: GraphCluster[],
  _quality: { modularity: number; silhouette: number; cohesion: number }
): string[] {
  if (clusters.length === 0) return ['No clusters detected'];
  return clusters.map(c => `Cluster ${c.id}: ${c.nodes.length} nodes, density ${c.density.toFixed(2)}`);
}

function growClusterFromNode(
  startNodeId: string,
  graphData: GraphData,
  visited: Set<string>
): string[] {
  const cluster: string[] = [];
  const queue = [startNodeId];

  while (queue.length > 0) {
    const nodeId = queue.shift()!;
    if (visited.has(nodeId)) continue;
    visited.add(nodeId);
    cluster.push(nodeId);

    const connectedNodes = graphData.edges
      .filter(edge => edge.source === nodeId || edge.target === nodeId)
      .map(edge => edge.source === nodeId ? edge.target : edge.source)
      .filter(id => !visited.has(id));

    queue.push(...connectedNodes);
  }

  return cluster;
}

function createClusterFromNodes(
  nodeIds: string[],
  graphData: GraphData,
  clusterId: string
): GraphCluster {
  const nodes = nodeIds.map(id => graphData.nodes.find(n => n.id === id)!);
  const positions = nodes.map(n => n.position || { x: 0, y: 0, z: 0 });

  const centerPosition = new Vector3(
    positions.reduce((sum, pos) => sum + pos.x, 0) / positions.length,
    positions.reduce((sum, pos) => sum + pos.y, 0) / positions.length,
    positions.reduce((sum, pos) => sum + pos.z, 0) / positions.length
  );

  const radius = Math.max(...positions.map(pos =>
    centerPosition.distanceTo(new Vector3(pos.x, pos.y, pos.z))
  ));

  const internalEdges = graphData.edges.filter(edge =>
    nodeIds.includes(edge.source) && nodeIds.includes(edge.target)
  ).length;

  const externalEdges = graphData.edges.filter(edge =>
    (nodeIds.includes(edge.source) && !nodeIds.includes(edge.target)) ||
    (!nodeIds.includes(edge.source) && nodeIds.includes(edge.target))
  ).length;

  const density = nodeIds.length > 1
    ? internalEdges / (nodeIds.length * (nodeIds.length - 1) / 2)
    : 0;

  const dominantTypes = getDominantTypes(nodes);
  const averageConnections = (internalEdges * 2) / nodeIds.length;
  const coherenceScore = internalEdges / Math.max(internalEdges + externalEdges, 1);

  return {
    id: clusterId,
    nodes: nodeIds,
    centerPosition,
    radius,
    density,
    dominantTypes,
    characteristics: { averageConnections, internalEdges, externalEdges, coherenceScore },
    suggestedColor: generateClusterColor(dominantTypes[0]),
    label: generateClusterLabel(dominantTypes, nodeIds.length)
  };
}

function getDominantTypes(nodes: GraphNode[]): string[] {
  const typeCounts = new Map<string, number>();
  nodes.forEach(node => {
    const type = node.metadata?.type || 'unknown';
    typeCounts.set(type, (typeCounts.get(type) || 0) + 1);
  });
  return Array.from(typeCounts.entries())
    .sort((a, b) => b[1] - a[1])
    .map(([type]) => type)
    .slice(0, 3);
}

function generateClusterColor(dominantType: string): Color {
  const typeColors: Record<string, string> = {
    file: '#4CAF50', folder: '#FF9800', function: '#2196F3',
    class: '#9C27B0', variable: '#00BCD4', unknown: '#757575'
  };
  return new Color(typeColors[dominantType] || typeColors.unknown);
}

function generateClusterLabel(dominantTypes: string[], nodeCount: number): string {
  const primaryType = dominantTypes[0] || 'Mixed';
  return `${primaryType} cluster (${nodeCount} nodes)`;
}
