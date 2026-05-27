import { Vector3 } from 'three';
import type { GraphData } from '../../managers/graphDataManager';

export function generateCacheKey(...args: any[]): string {
  return JSON.stringify(args);
}

export function hasHierarchicalStructure(graphData: GraphData): boolean {
  const connectionCounts = new Map<string, number>();

  graphData.edges.forEach(edge => {
    connectionCounts.set(edge.source, (connectionCounts.get(edge.source) || 0) + 1);
    connectionCounts.set(edge.target, (connectionCounts.get(edge.target) || 0) + 1);
  });

  const counts = Array.from(connectionCounts.values());
  const avg = counts.reduce((sum, count) => sum + count, 0) / counts.length;
  return counts.some(count => count > avg * 3);
}

export function doEdgesCross(
  edge1: { start: Vector3; end: Vector3 },
  edge2: { start: Vector3; end: Vector3 }
): boolean {
  // Line segment intersection using cross-product orientation test (2D projection on XZ plane)
  const p1x = edge1.start.x, p1z = edge1.start.z;
  const p2x = edge1.end.x,   p2z = edge1.end.z;
  const p3x = edge2.start.x, p3z = edge2.start.z;
  const p4x = edge2.end.x,   p4z = edge2.end.z;

  const direction = (ax: number, az: number, bx: number, bz: number, cx: number, cz: number): number =>
    (bx - ax) * (cz - az) - (bz - az) * (cx - ax);

  const d1 = direction(p3x, p3z, p4x, p4z, p1x, p1z);
  const d2 = direction(p3x, p3z, p4x, p4z, p2x, p2z);
  const d3 = direction(p1x, p1z, p2x, p2z, p3x, p3z);
  const d4 = direction(p1x, p1z, p2x, p2z, p4x, p4z);

  return d1 * d2 < 0 && d3 * d4 < 0;
}

export function calculateAverageNodeDistance(positions: Map<string, Vector3>): number {
  const nodes = Array.from(positions.values());
  let totalDistance = 0;
  let count = 0;

  for (let i = 0; i < nodes.length; i++) {
    for (let j = i + 1; j < nodes.length; j++) {
      totalDistance += nodes[i].distanceTo(nodes[j]);
      count++;
    }
  }

  return count > 0 ? totalDistance / count : 0;
}
