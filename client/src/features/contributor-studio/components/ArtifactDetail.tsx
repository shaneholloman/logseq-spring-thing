/**
 * ArtifactDetail - /studio/:workspaceId/artifacts/:aid overlay body.
 *
 * Renders inside ContributorStudioRoot so the four panes remain visible
 * behind the modal-style detail.
 */

import React from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '../../design-system/components';

export interface ArtifactDetailProps {
  workspaceId: string;
  artifactId: string;
}

export function ArtifactDetail({
  workspaceId,
  artifactId,
}: ArtifactDetailProps): React.ReactElement {
  return (
    <div data-testid="studio-artifact-detail" className="p-4">
      <Card>
        <CardHeader>
          <CardTitle className="text-base">Artifact {artifactId}</CardTitle>
        </CardHeader>
        <CardContent className="text-sm text-muted-foreground">
          Workspace: {workspaceId}. Lineage + share-state affordances land
          with the BC18 read path (agent C1).
        </CardContent>
      </Card>
    </div>
  );
}
