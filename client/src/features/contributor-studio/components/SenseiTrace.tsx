/**
 * SenseiTrace - /studio/:workspaceId/sensei.
 *
 * Renders the Sensei decision trace + opt-out settings.
 */

import React from 'react';
import { Activity } from 'lucide-react';
import { EmptyState } from '../../design-system/components';
import { useSenseiStore } from '../stores/senseiStore';

export interface SenseiTraceProps {
  workspaceId: string;
}

export function SenseiTrace({ workspaceId: _workspaceId }: SenseiTraceProps): React.ReactElement {
  const trace = useSenseiStore((s) => s.trace);

  return (
    <div data-testid="studio-sensei-trace" className="p-6 h-full flex flex-col gap-3">
      <div className="flex items-center gap-2">
        <Activity className="w-5 h-5" aria-hidden />
        <h2 className="text-lg font-semibold">Sensei Trace</h2>
      </div>

      {trace.length === 0 ? (
        <EmptyState
          title="No events yet"
          description="Accept or dismiss a Sensei suggestion to populate the trace."
        />
      ) : (
        <ul className="space-y-1.5">
          {trace.map((t) => (
            <li key={t.id} className="text-xs border border-border rounded p-2">
              <div className="font-medium">{t.label}</div>
              <div className="text-muted-foreground">{t.detail}</div>
              <div className="text-[10px] text-muted-foreground/80 mt-0.5">
                {t.at}
              </div>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
