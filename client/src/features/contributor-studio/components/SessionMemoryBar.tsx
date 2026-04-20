/**
 * SessionMemoryBar - bottom horizontal rail (48-96px).
 *
 * Three rotating item classes (episodic highlights, recent artifacts, open
 * ShareIntents). Read-only. Spec: surface-spec §10.
 */

import React from 'react';
import { ScrollArea } from '../../design-system/components';
import { useStudioInboxStore } from '../stores/studioInboxStore';
import { useSenseiStore } from '../stores/senseiStore';

export interface SessionMemoryBarProps {
  workspaceId: string;
}

export function SessionMemoryBar({
  workspaceId: _workspaceId,
}: SessionMemoryBarProps): React.ReactElement {
  const recent = useStudioInboxStore((s) => s.recentArtifactIds);
  const trace = useSenseiStore((s) => s.trace.slice(0, 6));

  return (
    <footer
      data-testid="studio-session-memory-bar"
      aria-label="Session memory bar"
      className="shrink-0 h-12 border-t border-border bg-background/60"
    >
      <ScrollArea className="h-full">
        <ul className="flex items-center gap-2 h-full px-3">
          {trace.length === 0 && recent.length === 0 ? (
            <li className="text-xs text-muted-foreground">No recent activity.</li>
          ) : null}
          {trace.map((t) => (
            <li
              key={t.id}
              className="shrink-0 inline-flex items-center gap-1.5 px-2 py-1 rounded-full text-xs border border-border bg-muted/30"
              title={t.detail}
            >
              <span className="w-1.5 h-1.5 rounded-full bg-cyan-400" aria-hidden />
              {t.label}
            </li>
          ))}
          {recent.map((id) => (
            <li
              key={id}
              className="shrink-0 inline-flex items-center gap-1.5 px-2 py-1 rounded-full text-xs border border-border bg-muted/30"
            >
              <span className="w-1.5 h-1.5 rounded-full bg-emerald-400" aria-hidden />
              Artifact {id}
            </li>
          ))}
        </ul>
      </ScrollArea>
    </footer>
  );
}
