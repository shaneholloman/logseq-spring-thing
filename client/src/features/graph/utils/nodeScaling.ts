import type { GraphVisualMode } from '../hooks/useGraphVisualState';
import type { Node as GraphNode } from '../managers/graphDataManager';
import type { GraphTypeVisualsSettings } from '../../settings/config/settings';

// Default scaling parameters matching the original hardcoded values.
// These are used when settings are not yet loaded or fields are missing.
const KG_DEFAULTS = { authorityScaleFactor: 0.5, connectionInfluence: 0.8, globalScaleMultiplier: 2.5 };
// Ontology coefficients. The ontology branch is now feature-driven (no global
// multiplier): scale is composed from real, available signals so it lands in the
// same world-unit ballpark as the KG branch (~2.5–19) without any magic factor.
//   - connectionInfluence  → weight on sqrt(degree); degree is universal (works
//                            for owl_class which carries no file_size).
//   - sizeInfluence        → weight on log(file_size+1) volume term (page /
//                            ontology_node only; absent for owl_class).
//   - hierarchyScaleFactor → optional depth refinement (only applied when depth>0).
//   - instanceCountInfluence → optional refinement (only applied when ic>0).
//   - minScale             → floor on the depth refinement factor.
const ONTO_DEFAULTS = {
  hierarchyScaleFactor: 0.15,
  minScale: 0.75,
  instanceCountInfluence: 0.1,
  connectionInfluence: 0.8,
  sizeInfluence: 0.9,
};
const AGENT_DEFAULTS = { workloadInfluence: 0.3, tokenRateInfluence: 100, tokenRateCap: 0.5 };

/**
 * Compute visual scale for a graph node based on its metadata and the active graph mode.
 *
 * Single source of truth for node scaling — both GemNodes (instanced mesh)
 * and GraphManager (edge offset computation) must call this function to stay in sync.
 *
 * Scaling parameters are read from `graphTypeVisuals` settings with hardcoded
 * fallback defaults that preserve the original visual behavior.
 */
export const computeNodeScale = (
  node: GraphNode,
  connectionCountMap: Map<string, number>,
  graphMode: GraphVisualMode,
  hierarchyMap?: Map<string, any>,
  visuals?: GraphTypeVisualsSettings,
): number => {
  const base = node.metadata?.size || 1.0;
  const id = String(node.id);

  if (graphMode === 'ontology') {
    // Feature-driven ontology scale — no global multiplier. Size is composed
    // from real signals present in this dataset, in priority order:
    //   base(1)
    //   + sqrt(degree)   * connInfluence   (universal — every node has a degree)
    //   + log(fileSize+1)* sizeInfluence   (page / ontology_node carry file_size)
    // then optionally refined (multiplicatively) by depth and instanceCount when
    // those signals are actually present (>0). This lands ontology nodes in the
    // same ~2.5–19 ballpark as the KG branch without any magic factor.
    const cfg = visuals?.ontology;
    const connInfl = (cfg as { connectionInfluence?: number } | undefined)?.connectionInfluence
      ?? ONTO_DEFAULTS.connectionInfluence;
    const sizeInfl = (cfg as { sizeInfluence?: number } | undefined)?.sizeInfluence
      ?? ONTO_DEFAULTS.sizeInfluence;
    const hsFactor = cfg?.hierarchyScaleFactor ?? ONTO_DEFAULTS.hierarchyScaleFactor;
    const minS = cfg?.minScale ?? ONTO_DEFAULTS.minScale;
    const icInfluence = cfg?.instanceCountInfluence ?? ONTO_DEFAULTS.instanceCountInfluence;

    // (b) connection degree — always available, universal.
    const degree = connectionCountMap.get(id) || 0;
    // (a) file_size — backend emits a byte-count string for page / ontology_node.
    const fileSizeRaw = node.metadata?.file_size;
    const fileSize = fileSizeRaw != null ? parseInt(String(fileSizeRaw), 10) : 0;
    const fileSizeTerm = fileSize > 0 ? Math.log(fileSize + 1) * sizeInfl : 0;

    // Core feature-driven size: base + degree term + volume term.
    let scale = base + Math.sqrt(degree) * connInfl + fileSizeTerm;

    // (c) optional refinements — only when the signal is actually present.
    const depth = hierarchyMap?.get(id)?.depth ?? (node.metadata?.depth ?? 0);
    if (depth > 0) scale *= Math.max(minS, 1.0 - depth * hsFactor);
    const ic = parseInt(node.metadata?.instanceCount || '0', 10);
    if (ic > 0) scale *= (1 + Math.log(ic + 1) * icInfluence);

    return scale;
  }

  if (graphMode === 'agent') {
    const cfg = visuals?.agent;
    const w = node.metadata?.workload ?? 0;
    const t = node.metadata?.tokenRate ?? 0;
    const wInfluence = cfg?.workloadInfluence ?? AGENT_DEFAULTS.workloadInfluence;
    const tDivisor = cfg?.tokenRateInfluence ?? AGENT_DEFAULTS.tokenRateInfluence;
    const tCap = cfg?.tokenRateCap ?? AGENT_DEFAULTS.tokenRateCap;
    return base * (1 + w * wInfluence + Math.min(t / tDivisor, tCap));
  }

  // Knowledge graph (default)
  // Scaling uses sqrt for stronger size distinction between hubs and leaves:
  //   degree 1  → scale ~2.5  (small)
  //   degree 10 → scale ~5.8  (medium)
  //   degree 50 → scale ~11.5 (large)
  //   degree 149→ scale ~19.0 (very large, ~7.6x smallest)
  const cfg = visuals?.knowledgeGraph;
  const auth = node.metadata?.authority ?? node.metadata?.authorityScore ?? 0;
  const cc = connectionCountMap.get(id) || 0;
  const connInfl = cfg?.connectionInfluence ?? KG_DEFAULTS.connectionInfluence;
  const authFactor = cfg?.authorityScaleFactor ?? KG_DEFAULTS.authorityScaleFactor;
  const globalMult = cfg?.globalScaleMultiplier ?? KG_DEFAULTS.globalScaleMultiplier;
  return base * (1 + Math.sqrt(cc) * connInfl) * (1 + auth * authFactor) * globalMult;
};
