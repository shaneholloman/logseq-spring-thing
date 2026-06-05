/**
 * Inferred-edges store — shared state between the InferencePanel (reasoning
 * report) and the R3F InferredEdges overlay (visual differentiation).
 *
 * The inferred named graph `urn:ngm:graph:ontology:inferred` (ADR-099) is the
 * source of edges that must render distinctly (dashed / amber). This store:
 *   - owns the `showInferred` toggle (default on),
 *   - caches the last reasoning report,
 *   - derives the renderable edge list (subject/object node ids).
 *
 * It does NO solving and NO layout. Positions come from the GPU node-position
 * SAB at render time; this store only supplies which (sourceId → targetId)
 * pairs are inferred so the overlay can draw them in the differentiated style.
 */

import { create } from 'zustand';
import {
  fetchReasoningReport,
  type ReasoningReport,
  type InferredTriple,
  EMPTY_REASONING_REPORT,
} from '../services/inferredAxiomsService';
import { createLogger } from '../../../utils/loggerConfig';

const logger = createLogger('InferredEdgesStore');

/** A renderable inferred edge — node ids resolve to positions in the SAB. */
export interface InferredEdge {
  sourceId: string;
  targetId: string;
  predicate: string;
  justification?: string;
}

interface InferredEdgesState {
  /** Toggle for rendering the differentiated inferred edges. */
  showInferred: boolean;
  /** True while a report fetch is in flight. */
  loading: boolean;
  /** Last fetched reasoning report (EMPTY until first successful load). */
  report: ReasoningReport;
  /** Edges with both endpoints resolvable to node ids (renderable subset). */
  inferredEdges: InferredEdge[];

  setShowInferred: (show: boolean) => void;
  toggleShowInferred: () => void;
  /** Pull the latest inferred axioms from the server (empty-safe). */
  refresh: () => Promise<void>;
}

/** Derive renderable edges from a report: only triples whose endpoints carry
 *  node ids can be drawn (the rest remain in the textual report only). */
function deriveEdges(triples: InferredTriple[]): InferredEdge[] {
  const edges: InferredEdge[] = [];
  for (const t of triples) {
    if (t.sourceNodeId && t.targetNodeId) {
      edges.push({
        sourceId: t.sourceNodeId,
        targetId: t.targetNodeId,
        predicate: t.predicate,
        justification: t.justification,
      });
    }
  }
  return edges;
}

export const useInferredEdgesStore = create<InferredEdgesState>((set) => ({
  showInferred: true,
  loading: false,
  report: EMPTY_REASONING_REPORT,
  inferredEdges: [],

  setShowInferred: (show) => set({ showInferred: show }),
  toggleShowInferred: () => set((s) => ({ showInferred: !s.showInferred })),

  refresh: async () => {
    set({ loading: true });
    try {
      const report = await fetchReasoningReport();
      set({
        report,
        inferredEdges: deriveEdges(report.triples),
        loading: false,
      });
    } catch (err) {
      // fetchReasoningReport is itself empty-safe, but guard the store too.
      logger.debug('Inferred-edges refresh failed; keeping empty state');
      set({ report: EMPTY_REASONING_REPORT, inferredEdges: [], loading: false });
    }
  },
}));
