/**
 * InstalledSkills - /studio/:workspaceId/skills.
 */

import React from 'react';
import { EmptyState } from '../../design-system/components';
import { useStudioWorkspaceStore } from '../stores/studioWorkspaceStore';

export interface InstalledSkillsProps {
  workspaceId: string;
}

export function InstalledSkills({
  workspaceId,
}: InstalledSkillsProps): React.ReactElement {
  const skills = useStudioWorkspaceStore(
    (s) => s.workspaces.find((w) => w.id === workspaceId)?.installedSkills ?? [],
  );

  if (skills.length === 0) {
    return (
      <div data-testid="studio-installed-skills" className="p-4">
        <EmptyState
          title="No skills installed"
          description="Browse the Dojo to install your first skill."
        />
      </div>
    );
  }

  return (
    <div data-testid="studio-installed-skills" className="p-4 space-y-2">
      {skills.map((s) => (
        <div
          key={s.id}
          className="flex items-center justify-between p-2 rounded border border-border"
        >
          <div className="flex flex-col">
            <span className="text-sm font-medium">{s.name}</span>
            <span className="text-xs text-muted-foreground">
              v{s.version} - {s.scope}
            </span>
          </div>
          <span className="text-xs text-muted-foreground">
            pass {s.evalPassRate == null ? '--' : `${Math.round(s.evalPassRate * 100)}%`}
          </span>
        </div>
      ))}
    </div>
  );
}
