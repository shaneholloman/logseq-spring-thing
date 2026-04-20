/**
 * ContributorStudioRoot - route component for /studio/:workspaceId and any
 * sub-routes that render inside the four-pane shell.
 *
 * Registers the 15 palette commands, pulls workspace context, and composes
 * `WorkspaceBar + PaneLayout(OntologyGuideRail | WorkLane | AIPartnerLane) +
 * SessionMemoryBar`.
 *
 * Spec: surface-spec §17.
 */

import React, { useEffect } from 'react';
import { useStudioCommands } from '../hooks/useStudioCommands';
import { useStudioWorkspaceStore } from '../stores/studioWorkspaceStore';
import { useStudioContextStore } from '../stores/studioContextStore';
import { useSenseiStore } from '../stores/senseiStore';
import { useStudioInboxStore } from '../stores/studioInboxStore';
import { WorkspaceBar } from './WorkspaceBar';
import { PaneLayout } from './PaneLayout';
import { OntologyGuideRail } from './OntologyGuideRail';
import { WorkLane } from './WorkLane';
import { AIPartnerLane } from './AIPartnerLane';
import { SessionMemoryBar } from './SessionMemoryBar';

export interface ContributorStudioRootProps {
  workspaceId: string;
  children?: React.ReactNode;
}

export function ContributorStudioRoot({
  workspaceId,
  children,
}: ContributorStudioRootProps): React.ReactElement {
  const setActive = useStudioWorkspaceStore((s) => s.setActive);
  const fetchWorkspaces = useStudioWorkspaceStore((s) => s.fetchWorkspaces);
  const assembleContext = useStudioContextStore((s) => s.assembleContext);
  const loadNudges = useSenseiStore((s) => s.loadNudges);
  const fetchInbox = useStudioInboxStore((s) => s.fetchInbox);

  useStudioCommands();

  useEffect(() => {
    setActive(workspaceId || null);
    void fetchWorkspaces();
    void fetchInbox();
    if (workspaceId) {
      void assembleContext(workspaceId);
      void loadNudges(workspaceId);
    }
  }, [workspaceId, setActive, fetchWorkspaces, fetchInbox, assembleContext, loadNudges]);

  return (
    <div
      data-testid="studio-root"
      className="h-full w-full flex flex-col bg-[#000022] text-foreground"
    >
      <WorkspaceBar workspaceId={workspaceId || null} />

      <PaneLayout
        left={<OntologyGuideRail workspaceId={workspaceId} />}
        centre={<WorkLane workspaceId={workspaceId}>{children}</WorkLane>}
        right={<AIPartnerLane workspaceId={workspaceId} />}
        bottom={<SessionMemoryBar workspaceId={workspaceId} />}
      />
    </div>
  );
}

export default ContributorStudioRoot;
