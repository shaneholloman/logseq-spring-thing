/**
 * OntologyTabContent — the control-panel "Ontology" tab body. This is the JSX
 * render site that mounts the previously-orphaned ontology panels (PRD-018
 * WS-2/WS-4, ADR-099 D4):
 *   - OntologyPanel              (already wired — management/validation)
 *   - OntologyBrowser            (class/property tree — was orphaned)
 *   - InferencePanel             (reasoning report — was orphaned)
 *   - SparqlConsole              (read-only server-side SELECT — new)
 *   - OntologyExplorationControls (TBox/ABox, focus/isolate, graph_type, inferred toggle)
 *
 * Reuse-first: the panels are imported and mounted as-is; no rewrites. The
 * reasoning report is fetched once on mount (empty-safe) and passed to both the
 * InferencePanel (textual report) and the InferredEdges overlay (via the shared
 * useInferredEdgesStore) so the dashed-amber edges stay in sync with the list.
 *
 * Accessibility: the live ontology surface (previously zero a11y) now carries a
 * labelled region, a tablist with roving selection, and labelled sub-regions.
 */

import React, { useEffect, useState } from 'react';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/features/design-system/components/Tabs';
import { OntologyPanel } from './OntologyPanel';
import { OntologyBrowser } from './OntologyBrowser';
import { InferencePanel } from './InferencePanel';
import { SparqlConsole } from './SparqlConsole';
import { OntologyExplorationControls, type OntologyScope } from './OntologyExplorationControls';
import { OntologyForcesPanel } from './OntologyForcesPanel';
import { useInferredEdgesStore } from '../store/useInferredEdgesStore';

export function OntologyTabContent() {
  const [scope, setScope] = useState<OntologyScope>('both');

  const report = useInferredEdgesStore((s) => s.report);
  const reportLoading = useInferredEdgesStore((s) => s.loading);
  const refresh = useInferredEdgesStore((s) => s.refresh);

  // Pull the reasoning report once on mount (empty-safe if backend not live).
  useEffect(() => {
    refresh();
  }, [refresh]);

  return (
    <div
      className="space-y-4"
      role="region"
      aria-label="Ontology exploration"
    >
      <Tabs defaultValue="browse" className="w-full">
        <TabsList
          className="grid w-full grid-cols-5"
          role="tablist"
          aria-label="Ontology tools"
        >
          <TabsTrigger value="browse">Browse</TabsTrigger>
          <TabsTrigger value="reason">Reason</TabsTrigger>
          <TabsTrigger value="forces">Forces</TabsTrigger>
          <TabsTrigger value="query">Query</TabsTrigger>
          <TabsTrigger value="manage">Manage</TabsTrigger>
        </TabsList>

        <TabsContent value="browse" className="space-y-4" role="tabpanel" aria-label="Browse ontology">
          <OntologyExplorationControls scope={scope} onScopeChange={setScope} />
          {/* ABox-only scope hides the class/property schema tree. */}
          {scope !== 'abox' && <OntologyBrowser />}
        </TabsContent>

        <TabsContent value="reason" className="space-y-4" role="tabpanel" aria-label="Reasoning">
          <InferencePanel
            report={report}
            reportLoading={reportLoading}
            onRefreshReport={refresh}
          />
        </TabsContent>

        <TabsContent value="forces" className="space-y-4" role="tabpanel" aria-label="Ontology forces">
          <OntologyForcesPanel />
        </TabsContent>

        <TabsContent value="query" className="space-y-4" role="tabpanel" aria-label="SPARQL query">
          <SparqlConsole />
        </TabsContent>

        <TabsContent value="manage" className="space-y-4" role="tabpanel" aria-label="Ontology management">
          <OntologyPanel />
        </TabsContent>
      </Tabs>
    </div>
  );
}

export default OntologyTabContent;
