/**
 * WorkspaceCreateWizard - /studio/new.
 *
 * Three-step wizard (role + goals + collaborators) from surface-spec §12.
 * Thin stub until agent C1 wires the pod write path.
 */

import React from 'react';
import { EmptyState } from '../../design-system/components';

export function WorkspaceCreateWizard(): React.ReactElement {
  return (
    <div
      data-testid="studio-workspace-create-wizard"
      className="h-full w-full p-6 bg-[#000022]"
    >
      <h1 className="text-xl font-semibold mb-4">New Workspace</h1>
      <EmptyState
        title="Wizard scaffolded"
        description="ContributorProfile + default workspace wizard lands when backend agent C1 wires /api/studio/workspaces."
      />
    </div>
  );
}
