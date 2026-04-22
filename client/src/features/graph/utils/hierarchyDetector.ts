/**
 * Edge-based hierarchy detector.
 *
 * Builds parent-child relationships from `hierarchical` edges (SUBCLASS_OF)
 * rather than inferring structure from node-ID path segments. This is the
 * correct approach for ontology graphs where node IDs are opaque u32 hashes.
 * For KG nodes with slash-separated path IDs the path heuristic still applies
 * as a fallback for nodes that receive no hierarchical edge.
 */

import type { Node, Edge } from '../managers/graphWorkerProxy';

export interface HierarchyNode extends Node {
  parentId?: string;
  depth: number;
  childIds: string[];
  isRoot: boolean;
}

/**
 * Build hierarchy from SUBCLASS_OF (hierarchical) edges, with path fallback.
 *
 * Edge direction: source SUBCLASS_OF target → source is the child, target is the parent.
 */
export function detectHierarchy(
  nodes: Node[],
  edges: Edge[] = [],
): Map<string, HierarchyNode> {
  const map = new Map<string, HierarchyNode>();

  // Seed every node as a root with no parent
  for (const n of nodes) {
    map.set(String(n.id), {
      ...n,
      parentId: undefined,
      depth: 0,
      childIds: [],
      isRoot: true,
    });
  }

  // Pass 1: build parent-child from hierarchical edges.
  // Recognise both the normalised "hierarchical" tag and the raw Neo4j relationship
  // type "SUBCLASS_OF" that the backend's COALESCE(r.edge_type, type(r)) fallback returns.
  for (const edge of edges) {
    const et = (edge.edgeType ?? (edge as unknown as Record<string, unknown>).edge_type ?? '') as string;
    const metaEt = (edge.metadata?.edge_type ?? edge.metadata?.relation_type ?? '') as string;
    const isHierarchical =
      et === 'hierarchical' || et === 'SUBCLASS_OF' || et.toLowerCase() === 'subclass_of' ||
      metaEt === 'hierarchical' || metaEt === 'SUBCLASS_OF';

    if (!isHierarchical) continue;

    const childId = String(edge.source);
    const parentId = String(edge.target);
    if (childId === parentId) continue;

    const child = map.get(childId);
    const parent = map.get(parentId);
    if (!child || !parent) continue;

    // Only assign a single parent (first hierarchical edge wins)
    if (child.isRoot) {
      child.parentId = parentId;
      child.isRoot = false;
    }
    if (!parent.childIds.includes(childId)) {
      parent.childIds.push(childId);
    }
  }

  // Pass 2: path-heuristic fallback for nodes with no hierarchical edge
  for (const [id, node] of map) {
    if (!node.isRoot) continue;
    const pathParts = id.split('/').filter(p => p.length > 0);
    if (pathParts.length < 2) continue;
    const parentPath = pathParts.slice(0, -1).join('/');
    const parent = map.get(parentPath);
    if (!parent) continue;
    node.parentId = parentPath;
    node.isRoot = false;
    if (!parent.childIds.includes(id)) {
      parent.childIds.push(id);
    }
  }

  // Pass 3: BFS from roots to assign depths
  const queue: Array<{ id: string; depth: number }> = [];
  for (const [id, node] of map) {
    if (node.isRoot) queue.push({ id, depth: 0 });
  }
  while (queue.length > 0) {
    const item = queue.shift()!;
    const node = map.get(item.id);
    if (!node) continue;
    node.depth = item.depth;
    for (const childId of node.childIds) {
      queue.push({ id: childId, depth: item.depth + 1 });
    }
  }

  return map;
}

export function getDescendants(
  nodeId: string,
  hierarchyMap: Map<string, HierarchyNode>,
): string[] {
  const node = hierarchyMap.get(nodeId);
  if (!node) return [];
  const descendants: string[] = [];
  const queue = [...node.childIds];
  while (queue.length > 0) {
    const childId = queue.shift()!;
    descendants.push(childId);
    const child = hierarchyMap.get(childId);
    if (child) queue.push(...child.childIds);
  }
  return descendants;
}

export function getAncestors(
  nodeId: string,
  hierarchyMap: Map<string, HierarchyNode>,
): string[] {
  const ancestors: string[] = [];
  let currentId: string | undefined = nodeId;
  while (currentId) {
    const node = hierarchyMap.get(currentId);
    if (!node?.parentId) break;
    ancestors.push(node.parentId);
    currentId = node.parentId;
  }
  return ancestors;
}

export function getRootNodes(hierarchyMap: Map<string, HierarchyNode>): string[] {
  const roots: string[] = [];
  for (const [id, node] of hierarchyMap) {
    if (node.isRoot) roots.push(id);
  }
  return roots;
}

export function getMaxDepth(hierarchyMap: Map<string, HierarchyNode>): number {
  let max = 0;
  for (const node of hierarchyMap.values()) {
    if (node.depth > max) max = node.depth;
  }
  return max;
}
