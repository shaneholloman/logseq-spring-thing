/**
 * AutomationList - /studio/automations.
 */

import React from 'react';
import { CalendarClock, Plus } from 'lucide-react';
import { Button, EmptyState } from '../../design-system/components';
import { navigateToStudioPath } from '../routes';

export function AutomationList(): React.ReactElement {
  return (
    <div
      data-testid="studio-automation-list"
      className="h-full w-full p-6 bg-[#000022] flex flex-col gap-4"
    >
      <header className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <CalendarClock className="w-5 h-5" aria-hidden />
          <h1 className="text-xl font-semibold">Automations</h1>
        </div>
        <Button
          variant="outline"
          size="sm"
          onClick={() => navigateToStudioPath('/studio/automations/new')}
          className="gap-1.5"
        >
          <Plus className="w-4 h-4" aria-hidden /> New
        </Button>
      </header>

      <EmptyState
        title="No automations scheduled"
        description="Schedule headless skill runs to write artifacts to your pod. Wire-up lands with agent C5's AutomationOrchestrator."
      />
    </div>
  );
}
