/**
 * SolidGraphViewService
 *
 * Graph view management methods extracted from SolidPodService to reduce file
 * size (CRITICAL-6 tech debt). All methods delegate to the SolidPodService
 * singleton — this module is a thin facade for import convenience.
 *
 * Consumers can import from here directly or continue importing from
 * SolidPodService (which re-exports everything for backward compatibility).
 */

import solidPodService from './SolidPodService';

// Re-export the singleton's graph view methods as standalone functions
// so new code can import { saveGraphView } from './SolidGraphViewService'

export async function saveGraphView(
  name: string,
  viewData: {
    camera?: { x: number; y: number; z: number; fov?: number };
    filters?: Record<string, unknown>;
    physics?: Record<string, unknown>;
    clusters?: Record<string, unknown>;
    pinnedNodes?: number[];
    nodeTypeVisibility?: Record<string, boolean>;
  }
): Promise<boolean> {
  return solidPodService.saveGraphView(name, viewData);
}

export async function loadGraphView(
  name: string
): Promise<Record<string, unknown> | null> {
  return solidPodService.loadGraphView(name);
}

export async function listGraphViews(): Promise<string[]> {
  return solidPodService.listGraphViews();
}

export async function deleteGraphView(name: string): Promise<boolean> {
  return solidPodService.deleteGraphView(name);
}

export function subscribeToGraphViewChanges(
  callback: (viewName: string) => void
): () => void {
  return solidPodService.subscribeToGraphViewChanges(callback);
}
