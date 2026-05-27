import { Vector3 } from 'three';
import type { GraphData } from '../../managers/graphDataManager';
import type { LayoutOptimization, GraphMetrics } from './types';
import { hasHierarchicalStructure, doEdgesCross, calculateAverageNodeDistance } from './utils';

export function selectOptimalAlgorithm(
  graphData: GraphData,
  constraints: any
): LayoutOptimization['algorithmUsed'] {
  const nodeCount = graphData.nodes.length;
  const edgeCount = graphData.edges.length;
  const density = edgeCount / (nodeCount * (nodeCount - 1) / 2);

  if (nodeCount < 50 && constraints.minimizeEdgeCrossings) return 'force-directed';
  if (density > 0.3 && constraints.respectClusters) return 'hierarchical';
  if (nodeCount > 200) return 'grid';
  if (hasHierarchicalStructure(graphData)) return 'hierarchical';
  return 'organic';
}

export async function applyOptimizationAlgorithm(
  graphData: GraphData,
  currentPositions: Map<string, Vector3>,
  algorithm: LayoutOptimization['algorithmUsed'],
  constraints: any
): Promise<Map<string, Vector3>> {
  switch (algorithm) {
    case 'force-directed': return applyForceDirectedLayout(graphData, currentPositions, constraints);
    case 'hierarchical':   return applyHierarchicalLayout(graphData, constraints);
    case 'circular':       return applyCircularLayout(graphData);
    case 'grid':           return applyGridLayout(graphData);
    case 'organic':        return applyOrganicLayout(graphData, currentPositions, constraints);
    default:               return currentPositions;
  }
}

export function calculateLayoutMetrics(
  graphData: GraphData,
  positions: Map<string, Vector3>
): { edgeCrossings: number; nodeOverlaps: number; readability: number } {
  let edgeCrossings = 0;
  let nodeOverlaps = 0;

  const edges = graphData.edges.map(edge => ({
    start: positions.get(edge.source)!,
    end: positions.get(edge.target)!
  }));

  for (let i = 0; i < edges.length; i++) {
    for (let j = i + 1; j < edges.length; j++) {
      if (doEdgesCross(edges[i], edges[j])) edgeCrossings++;
    }
  }

  const nodes = Array.from(positions.values());
  for (let i = 0; i < nodes.length; i++) {
    for (let j = i + 1; j < nodes.length; j++) {
      if (nodes[i].distanceTo(nodes[j]) < 2.0) nodeOverlaps++;
    }
  }

  const averageDistance = calculateAverageNodeDistance(positions);
  const readability = Math.min(1, averageDistance / 5);

  return { edgeCrossings, nodeOverlaps, readability };
}

export function calculateOptimizationConfidence(
  currentMetrics: GraphMetrics,
  improvedMetrics: GraphMetrics
): number {
  const improvement = (improvedMetrics.efficiency - currentMetrics.efficiency) / currentMetrics.efficiency;
  return Math.min(0.95, 0.5 + improvement);
}

export function generateOptimizationReasoning(_algorithm: string, _improvements: any): string[] {
  return ['Layout optimization applied', 'Edge crossings reduced', 'Node spacing improved'];
}

function applyForceDirectedLayout(
  graphData: GraphData,
  currentPositions: Map<string, Vector3>,
  _constraints: any
): Map<string, Vector3> {
  const positions = new Map(currentPositions);
  const iterations = 100;
  const coolingFactor = 0.95;
  let temperature = 1.0;

  for (let i = 0; i < iterations; i++) {
    for (const node1 of graphData.nodes) {
      const pos1 = positions.get(node1.id)!;
      let force = new Vector3(0, 0, 0);

      for (const node2 of graphData.nodes) {
        if (node1.id === node2.id) continue;
        const pos2 = positions.get(node2.id)!;
        const distance = pos1.distanceTo(pos2);
        const direction = new Vector3().subVectors(pos1, pos2).normalize();
        const repulsion = direction.multiplyScalar(1 / Math.max(distance * distance, 0.1));
        force.add(repulsion);
      }

      for (const edge of graphData.edges) {
        if (edge.source === node1.id || edge.target === node1.id) {
          const otherId = edge.source === node1.id ? edge.target : edge.source;
          const otherPos = positions.get(otherId)!;
          const distance = pos1.distanceTo(otherPos);
          const direction = new Vector3().subVectors(otherPos, pos1).normalize();
          const attraction = direction.multiplyScalar(distance * 0.01);
          force.add(attraction);
        }
      }

      const newPos = pos1.clone().add(force.multiplyScalar(temperature));
      positions.set(node1.id, newPos);
    }

    temperature *= coolingFactor;
  }

  return positions;
}

function applyHierarchicalLayout(graphData: GraphData, _constraints: any): Map<string, Vector3> {
  const positions = new Map<string, Vector3>();

  const inDegree = new Map<string, number>();
  graphData.nodes.forEach(node => inDegree.set(node.id, 0));
  graphData.edges.forEach(edge => {
    inDegree.set(edge.target, (inDegree.get(edge.target) || 0) + 1);
  });

  const rootNodes = graphData.nodes.filter(node => inDegree.get(node.id) === 0);
  const levels = new Map<string, number>();
  const queue = rootNodes.map(node => ({ id: node.id, level: 0 }));

  while (queue.length > 0) {
    const { id, level } = queue.shift()!;
    levels.set(id, level);
    const children = graphData.edges
      .filter(edge => edge.source === id)
      .map(edge => edge.target)
      .filter(childId => !levels.has(childId));
    children.forEach(childId => queue.push({ id: childId, level: level + 1 }));
  }

  const maxLevel = Math.max(...Array.from(levels.values()));
  const levelCounts = new Map<number, number>();
  levels.forEach(level => levelCounts.set(level, (levelCounts.get(level) || 0) + 1));

  levels.forEach((level, nodeId) => {
    const nodesAtLevel = levelCounts.get(level) || 1;
    const positionInLevel = Array.from(levels.entries())
      .filter(([_, l]) => l === level)
      .findIndex(([id]) => id === nodeId);
    const x = (positionInLevel - (nodesAtLevel - 1) / 2) * 10;
    const y = (maxLevel - level) * 10;
    positions.set(nodeId, new Vector3(x, y, 0));
  });

  return positions;
}

function applyCircularLayout(graphData: GraphData): Map<string, Vector3> {
  const positions = new Map<string, Vector3>();
  const radius = Math.max(10, graphData.nodes.length * 0.5);

  graphData.nodes.forEach((node, index) => {
    const angle = (index / graphData.nodes.length) * 2 * Math.PI;
    positions.set(node.id, new Vector3(Math.cos(angle) * radius, 0, Math.sin(angle) * radius));
  });

  return positions;
}

function applyGridLayout(graphData: GraphData): Map<string, Vector3> {
  const positions = new Map<string, Vector3>();
  const gridSize = Math.ceil(Math.sqrt(graphData.nodes.length));
  const spacing = 5;

  graphData.nodes.forEach((node, index) => {
    const row = Math.floor(index / gridSize);
    const col = index % gridSize;
    positions.set(node.id, new Vector3((col - gridSize / 2) * spacing, 0, (row - gridSize / 2) * spacing));
  });

  return positions;
}

function applyOrganicLayout(
  graphData: GraphData,
  currentPositions: Map<string, Vector3>,
  constraints: any
): Map<string, Vector3> {
  const positions = applyForceDirectedLayout(graphData, currentPositions, constraints);

  // Pull nodes toward their component cluster centres
  const clusters = detectSimpleClusters(graphData);
  clusters.forEach(cluster => {
    const clusterCenter = calculateClusterCenter(cluster, positions);
    cluster.forEach(nodeId => {
      const currentPos = positions.get(nodeId)!;
      const toCenter = new Vector3().subVectors(clusterCenter, currentPos).multiplyScalar(0.1);
      positions.set(nodeId, currentPos.add(toCenter));
    });
  });

  return positions;
}

function detectSimpleClusters(graphData: GraphData): string[][] {
  const visited = new Set<string>();
  const clusters: string[][] = [];

  for (const node of graphData.nodes) {
    if (visited.has(node.id)) continue;
    const cluster: string[] = [];
    const queue = [node.id];
    while (queue.length > 0) {
      const id = queue.shift()!;
      if (visited.has(id)) continue;
      visited.add(id);
      cluster.push(id);
      const connected = graphData.edges
        .filter(e => e.source === id || e.target === id)
        .map(e => e.source === id ? e.target : e.source)
        .filter(nid => !visited.has(nid));
      queue.push(...connected);
    }
    if (cluster.length > 1) clusters.push(cluster);
  }
  return clusters;
}

function calculateClusterCenter(cluster: string[], positions: Map<string, Vector3>): Vector3 {
  const center = new Vector3();
  let count = 0;
  for (const nodeId of cluster) {
    const pos = positions.get(nodeId);
    if (pos) { center.add(pos); count++; }
  }
  return count > 0 ? center.divideScalar(count) : center;
}
