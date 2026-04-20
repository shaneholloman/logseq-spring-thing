/**
 * WorkspaceListView - /studio index route.
 *
 * Shows existing ContributorWorkspaces plus the "Create new" CTA.
 */

import React, { useEffect } from 'react';
import { LayoutPanelLeft, Plus } from 'lucide-react';
import { Button, EmptyState } from '../../design-system/components';
import { useStudioWorkspaceStore } from '../stores/studioWorkspaceStore';
import { navigateToStudioPath } from '../routes';

export function WorkspaceListView(): React.ReactElement {
  const workspaces = useStudioWorkspaceStore((s) => s.workspaces);
  const fetchWorkspaces = useStudioWorkspaceStore((s) => s.fetchWorkspaces);

  useEffect(() => {
    void fetchWorkspaces();
  }, [fetchWorkspaces]);

  return (
    <div
      data-testid="studio-workspace-list"
      className="h-full w-full flex flex-col p-6 gap-4 bg-[#000022]"
    >
      <header className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <LayoutPanelLeft className="w-5 h-5" aria-hidden />
          <h1 className="text-xl font-semibold">Contributor Studio</h1>
        </div>
        <Button
          variant="outline"
          size="sm"
          onClick={() => navigateToStudioPath('/studio/new')}
          className="gap-1.5"
        >
          <Plus className="w-4 h-4" aria-hidden /> New Workspace
        </Button>
      </header>

      {workspaces.length === 0 ? (
        <EmptyState
          title="No workspaces yet"
          description="Create your first workspace to start curating pod context, installing skills, and pairing with a scoped AI partner."
          action={
            <Button onClick={() => navigateToStudioPath('/studio/new')}>
              Create workspace
            </Button>
          }
        />
      ) : (
        <ul className="grid grid-cols-1 md:grid-cols-2 gap-3">
          {workspaces.map((ws) => (
            <li key={ws.id}>
              <button
                type="button"
                onClick={() => navigateToStudioPath(`/studio/${ws.id}`)}
                className="w-full text-left p-3 rounded border border-border hover:border-cyan-400/40"
              >
                <div className="font-medium text-sm">{ws.name}</div>
                <div className="text-xs text-muted-foreground mt-1">
                  Focus: {ws.focus.label || 'none'}
                </div>
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
