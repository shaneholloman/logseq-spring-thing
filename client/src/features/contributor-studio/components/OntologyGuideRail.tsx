/**
 * OntologyGuideRail - left pane (320px default).
 *
 * Three Collapsible sections (Canonical Terms, Nearby Concepts, Applicable
 * Policies + Precedents) plus an Installed Skills sub-panel. Suggestions
 * arrive from the Sensei MCP bridge; rendering contract per surface-spec §6.
 */

import React from 'react';
import { BookOpenCheck, Compass, ShieldCheck, Wrench } from 'lucide-react';
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
  ScrollArea,
} from '../../design-system/components';
import { useSenseiStore } from '../stores/senseiStore';
import { useStudioWorkspaceStore } from '../stores/studioWorkspaceStore';

interface RailSectionProps {
  id: string;
  label: string;
  icon: React.ComponentType<{ className?: string }>;
  count: number;
  emptyLabel: string;
  children?: React.ReactNode;
}

function RailSection({
  id,
  label,
  icon: Icon,
  count,
  emptyLabel,
  children,
}: RailSectionProps): React.ReactElement {
  return (
    <Collapsible defaultOpen>
      <section
        data-testid={`studio-rail-section-${id}`}
        role="region"
        aria-labelledby={`studio-rail-${id}-hdr`}
        className="border-b border-border/60"
      >
        <CollapsibleTrigger asChild>
          <button
            id={`studio-rail-${id}-hdr`}
            className="w-full flex items-center justify-between px-3 py-2 hover:bg-muted/40"
            type="button"
          >
            <span className="flex items-center gap-2 text-xs uppercase tracking-wide text-muted-foreground">
              <Icon className="w-3.5 h-3.5" aria-hidden />
              {label}
            </span>
            <span className="text-[10px] text-muted-foreground">{count}</span>
          </button>
        </CollapsibleTrigger>
        <CollapsibleContent className="px-3 pb-2">
          {count === 0 ? (
            <p className="text-xs text-muted-foreground py-1.5">{emptyLabel}</p>
          ) : (
            children
          )}
        </CollapsibleContent>
      </section>
    </Collapsible>
  );
}

export interface OntologyGuideRailProps {
  workspaceId: string;
}

export function OntologyGuideRail({
  workspaceId,
}: OntologyGuideRailProps): React.ReactElement {
  const nudges = useSenseiStore(
    (s) => s.nudgesByWorkspaceId[workspaceId] ?? { terms: [], concepts: [], policies: [] },
  );
  const skills = useStudioWorkspaceStore(
    (s) => s.workspaces.find((w) => w.id === workspaceId)?.installedSkills ?? [],
  );

  return (
    <aside
      data-testid="studio-ontology-guide-rail"
      aria-label="Ontology guide rail"
      className="h-full w-full flex flex-col border-r border-border bg-background/40"
    >
      <ScrollArea className="flex-1">
        <RailSection
          id="terms"
          label="Canonical Terms"
          icon={BookOpenCheck}
          count={nudges.terms.length}
          emptyLabel="No suggestions for this focus."
        />
        <RailSection
          id="concepts"
          label="Nearby Concepts"
          icon={Compass}
          count={nudges.concepts.length}
          emptyLabel="No suggestions for this focus."
        />
        <RailSection
          id="policies"
          label="Applicable Policies + Precedents"
          icon={ShieldCheck}
          count={nudges.policies.length}
          emptyLabel="No suggestions for this focus."
        />
        <RailSection
          id="skills"
          label="Installed Skills"
          icon={Wrench}
          count={skills.length}
          emptyLabel="No skills installed yet."
        >
          <ul className="space-y-1.5 pt-1">
            {skills.map((sk) => (
              <li
                key={sk.id}
                className="flex items-center justify-between text-xs px-1.5 py-1 rounded hover:bg-muted/40"
              >
                <span className="truncate">{sk.name}</span>
                <span className="text-muted-foreground ml-2">v{sk.version}</span>
              </li>
            ))}
          </ul>
        </RailSection>
      </ScrollArea>
    </aside>
  );
}
