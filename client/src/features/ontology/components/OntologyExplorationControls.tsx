/**
 * OntologyExplorationControls — TBox/ABox scope toggle, per-class focus/isolate,
 * server-side graph_type population filter, and the inferred-edge toggle.
 *
 * Kept as a small sibling of OntologyBrowser (which is already >500 lines and
 * must not grow). It drives the shared ontology contribution store and emits
 * the established `visionclaw:search` event to focus/isolate a class subtree in
 * the 3D graph. The graph_type filter calls graphDataManager.setGraphTypeFilter
 * then re-fetches — the filtering happens SERVER-SIDE (PRD-018 WS-4); no
 * client-side layout solving is performed here.
 */

import React, { useCallback } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/features/design-system/components/Card';
import { Button } from '@/features/design-system/components/Button';
import { Badge } from '@/features/design-system/components/Badge';
import { Switch } from '@/features/design-system/components/Switch';
import { Label } from '@/features/design-system/components/Label';
import { Layers, Boxes, Crosshair, Filter, GitBranch } from 'lucide-react';
import { useOntologyContributionStore } from '../hooks/useOntologyStore';
import { useInferredEdgesStore } from '../store/useInferredEdgesStore';
import { graphDataManager } from '../../graph/managers/graphDataManager';
import type { GraphTypeFilter } from '../../graph/managers/dataManager/restClient';
import { createLogger } from '../../../utils/loggerConfig';

const logger = createLogger('OntologyExplorationControls');

/** TBox = schema (classes/properties); ABox = instance data. */
export type OntologyScope = 'tbox' | 'abox' | 'both';

interface OntologyExplorationControlsProps {
  scope: OntologyScope;
  onScopeChange: (scope: OntologyScope) => void;
  className?: string;
}

const POPULATIONS: Array<{ id: GraphTypeFilter; label: string }> = [
  { id: 'all', label: 'All' },
  { id: 'knowledge', label: 'Knowledge' },
  { id: 'ontology', label: 'Ontology' },
  { id: 'agent', label: 'Agent' },
];

export function OntologyExplorationControls({
  scope,
  onScopeChange,
  className,
}: OntologyExplorationControlsProps) {
  const selectedNode = useOntologyContributionStore((s) => s.selectedNode);
  const getClassByIri = useOntologyContributionStore((s) => s.getClassByIri);

  const showInferred = useInferredEdgesStore((s) => s.showInferred);
  const setShowInferred = useInferredEdgesStore((s) => s.setShowInferred);

  // Focus: highlight the selected class in the 3D graph via the established
  // search-driven selection path (no client-side solving — just selection).
  const handleFocus = useCallback(() => {
    if (!selectedNode) return;
    const cls = getClassByIri(selectedNode);
    const term = cls?.label || selectedNode;
    window.dispatchEvent(new CustomEvent('visionclaw:search', { detail: { query: term } }));
    logger.info(`Focus class: ${term}`);
  }, [selectedNode, getClassByIri]);

  // Isolate: request the ontology population server-side, then focus the class.
  const handleIsolate = useCallback(async () => {
    if (!selectedNode) return;
    graphDataManager.setGraphTypeFilter('ontology');
    try {
      await graphDataManager.fetchInitialData();
    } catch (err) {
      logger.warn('Isolate re-fetch failed:', err);
    }
    handleFocus();
  }, [selectedNode, handleFocus]);

  const handlePopulation = useCallback(async (filter: GraphTypeFilter) => {
    graphDataManager.setGraphTypeFilter(filter);
    try {
      await graphDataManager.fetchInitialData();
      logger.info(`Population filter applied server-side: ${filter ?? 'all'}`);
    } catch (err) {
      logger.warn('Population filter re-fetch failed:', err);
    }
  }, []);

  const activeFilter = graphDataManager.getGraphTypeFilter() ?? 'all';

  return (
    <Card className={className} role="region" aria-label="Ontology exploration controls">
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <GitBranch className="h-5 w-5" />
          Explore
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        {/* TBox / ABox scope */}
        <div className="space-y-2">
          <Label>Scope</Label>
          <div className="grid grid-cols-3 gap-2" role="group" aria-label="TBox ABox scope">
            <Button
              variant={scope === 'tbox' ? 'default' : 'outline'}
              size="sm"
              onClick={() => onScopeChange('tbox')}
              aria-pressed={scope === 'tbox'}
            >
              <Layers className="mr-1 h-4 w-4" /> TBox
            </Button>
            <Button
              variant={scope === 'abox' ? 'default' : 'outline'}
              size="sm"
              onClick={() => onScopeChange('abox')}
              aria-pressed={scope === 'abox'}
            >
              <Boxes className="mr-1 h-4 w-4" /> ABox
            </Button>
            <Button
              variant={scope === 'both' ? 'default' : 'outline'}
              size="sm"
              onClick={() => onScopeChange('both')}
              aria-pressed={scope === 'both'}
            >
              Both
            </Button>
          </div>
        </div>

        {/* Per-class focus / isolate */}
        <div className="space-y-2">
          <Label>Selected class</Label>
          <div className="flex items-center gap-2">
            <Badge variant="secondary" className="truncate max-w-[180px]">
              {selectedNode || 'none'}
            </Badge>
          </div>
          <div className="grid grid-cols-2 gap-2">
            <Button variant="outline" size="sm" onClick={handleFocus} disabled={!selectedNode}>
              <Crosshair className="mr-1 h-4 w-4" /> Focus
            </Button>
            <Button variant="outline" size="sm" onClick={handleIsolate} disabled={!selectedNode}>
              <Filter className="mr-1 h-4 w-4" /> Isolate
            </Button>
          </div>
        </div>

        {/* Server-side population filter (PRD-018 WS-4) */}
        <div className="space-y-2">
          <Label>Population (server-side filter)</Label>
          <div className="grid grid-cols-4 gap-1" role="group" aria-label="Graph population filter">
            {POPULATIONS.map((p) => (
              <Button
                key={String(p.id)}
                variant={activeFilter === p.id ? 'default' : 'outline'}
                size="sm"
                onClick={() => handlePopulation(p.id)}
                aria-pressed={activeFilter === p.id}
              >
                {p.label}
              </Button>
            ))}
          </div>
        </div>

        {/* Inferred-edge differentiation toggle (ADR-099 D4) */}
        <div className="flex items-center justify-between pt-2 border-t">
          <Label htmlFor="show-inferred">Show inferred edges (dashed amber)</Label>
          <Switch id="show-inferred" checked={showInferred} onCheckedChange={setShowInferred} />
        </div>
      </CardContent>
    </Card>
  );
}

export default OntologyExplorationControls;
