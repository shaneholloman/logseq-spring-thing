/**
 * AutomationDetail - /studio/automations/:id.
 */

import React from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '../../design-system/components';

export interface AutomationDetailProps {
  automationId: string;
}

export function AutomationDetail({
  automationId,
}: AutomationDetailProps): React.ReactElement {
  return (
    <div
      data-testid="studio-automation-detail"
      className="p-6 h-full bg-[#000022]"
    >
      <Card>
        <CardHeader>
          <CardTitle className="text-base">Automation {automationId}</CardTitle>
        </CardHeader>
        <CardContent className="text-sm text-muted-foreground">
          Run history, schedule, budget, and delegated capability are rendered
          here once agent C5 wires AutomationOrchestratorActor.
        </CardContent>
      </Card>
    </div>
  );
}
