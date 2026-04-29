/**
 * Client-Side Expansion State Hook
 *
 * Manages per-client node expansion/collapse state.
 * NO server-side persistence - state is local to each client.
 */

import { useState, useCallback, useMemo } from 'react';
import type { HierarchyNode } from '../utils/hierarchyDetector';

export interface ExpansionState {
  /** Set of collapsed node IDs */
  collapsedNodes: Set<string>;

  /** Toggle expansion state for a node */
  toggleExpansion: (nodeId: string) => void;

  /** Check if a node is expanded (inverse of collapsed) */
  isExpanded: (nodeId: string) => boolean;

  /** Check if a node should be visible based on parent expansion */
  isVisible: (nodeId: string, parentId?: string) => boolean;

  /** Expand all nodes */
  expandAll: () => void;

  /** Collapse all nodes */
  collapseAll: (allNodeIds?: string[]) => void;

  /** Expand a node and all its ancestors */
  expandWithAncestors: (nodeId: string, ancestorIds: string[]) => void;

  /**
   * WebVOWL-style depth slider: collapse every parent at depth >= maxDepth.
   *
   * Semantic: nodes with `hierarchy.depth <= maxDepth` stay visible; deeper
   * descendants get hidden because their parent (at depth maxDepth or below
   * but still a parent of a hidden tier) is in `collapsedNodes`.
   *
   * To make `isVisible(child, parent)` correctly hide grandchildren, we collapse
   * EVERY parent at `depth >= maxDepth` — not just the boundary — so the chain
   * stays consistent without `isVisible` walking ancestors.
   *
   * Pass `Number.POSITIVE_INFINITY` (or any value >= max graph depth) to clear
   * the depth filter. Manual click-collapses persist independently — calling
   * this overwrites the entire set, so it should only be invoked when the
   * tier-depth setting changes, not on every render.
   */
  setCollapsedFromTierDepth: (
    maxDepth: number,
    hierarchyMap: Map<string, HierarchyNode>,
  ) => void;
}

/**
 * Hook for managing client-side node expansion state
 * @param defaultExpanded Whether nodes should be expanded by default (recommended: true)
 * @returns ExpansionState object with expansion controls
 */
export function useExpansionState(defaultExpanded: boolean = true): ExpansionState {
  // Store collapsed nodes (if defaultExpanded=true) or expanded nodes (if defaultExpanded=false)
  const [collapsedNodes, setCollapsedNodes] = useState<Set<string>>(new Set());

  const toggleExpansion = useCallback((nodeId: string) => {
    setCollapsedNodes(prev => {
      const next = new Set(prev);
      if (next.has(nodeId)) {
        next.delete(nodeId);
      } else {
        next.add(nodeId);
      }
      return next;
    });
  }, []);

  const isExpanded = useCallback((nodeId: string) => {
    // If defaultExpanded=true, node is expanded unless in collapsedNodes
    // If defaultExpanded=false, node is collapsed unless in collapsedNodes (which would be expandedNodes)
    return defaultExpanded ? !collapsedNodes.has(nodeId) : collapsedNodes.has(nodeId);
  }, [collapsedNodes, defaultExpanded]);

  const isVisible = useCallback((nodeId: string, parentId?: string) => {
    // Root nodes (no parent) are always visible
    if (!parentId) return true;

    // Child nodes are visible only if parent is expanded
    return isExpanded(parentId);
  }, [isExpanded]);

  const expandAll = useCallback(() => {
    setCollapsedNodes(new Set());
  }, []);

  const collapseAll = useCallback((allNodeIds?: string[]) => {
    if (defaultExpanded) {
      // If default is expanded, collapse all by adding all nodes to collapsed set
      if (allNodeIds) {
        setCollapsedNodes(new Set(allNodeIds));
      }
    } else {
      // If default is collapsed, just clear the expanded set
      setCollapsedNodes(new Set());
    }
  }, [defaultExpanded]);

  const expandWithAncestors = useCallback((nodeId: string, ancestorIds: string[]) => {
    setCollapsedNodes(prev => {
      const next = new Set(prev);
      // Remove node and all ancestors from collapsed set
      next.delete(nodeId);
      ancestorIds.forEach(ancestorId => next.delete(ancestorId));
      return next;
    });
  }, []);

  const setCollapsedFromTierDepth = useCallback(
    (maxDepth: number, hierarchyMap: Map<string, HierarchyNode>) => {
      // No-op when filter disabled (Infinity / sentinel-large value).
      if (!Number.isFinite(maxDepth) || maxDepth >= 999) {
        setCollapsedNodes(new Set());
        return;
      }
      const next = new Set<string>();
      for (const [id, h] of hierarchyMap) {
        // Only collapse PARENT nodes — leaves don't gate any children's visibility
        // and adding them just bloats the set. Cascading collapse works because
        // every parent at depth >= maxDepth ends up in the set.
        if (h.childIds.length > 0 && h.depth >= maxDepth) {
          next.add(id);
        }
      }
      setCollapsedNodes(next);
    },
    [],
  );

  return useMemo(() => ({
    collapsedNodes,
    toggleExpansion,
    isExpanded,
    isVisible,
    expandAll,
    collapseAll,
    expandWithAncestors,
    setCollapsedFromTierDepth,
  }), [collapsedNodes, toggleExpansion, isExpanded, isVisible, expandAll, collapseAll, expandWithAncestors, setCollapsedFromTierDepth]);
}
