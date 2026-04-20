/**
 * AutomationCreateWizard - /studio/automations/new.
 */

import React from 'react';
import { EmptyState } from '../../design-system/components';

export function AutomationCreateWizard(): React.ReactElement {
  return (
    <div
      data-testid="studio-automation-create-wizard"
      className="p-6 h-full bg-[#000022]"
    >
      <h1 className="text-xl font-semibold mb-4">Schedule Automation</h1>
      <EmptyState
        title="Wizard scaffolded"
        description="Schedule, budget tier, and delegated-cap selection wire up with agent C5's backend."
      />
    </div>
  );
}
