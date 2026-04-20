/**
 * WorkspaceBar - top fixed 48px bar.
 *
 * Layout: workspace selector, focus pill, share-state chip, automation status,
 * settings gear. Spec: surface-spec §11.
 */

import React from 'react';
import { Settings2, Target, CircleDot } from 'lucide-react';
import { Badge, Button } from '../../design-system/components';
import { useStudioWorkspaceStore } from '../stores/studioWorkspaceStore';
import { useStudioInboxUnreadCount } from '../stores/studioInboxStore';
import type { ShareState } from '../types';

const SHARE_TONE: Record<ShareState, string> = {
  Private: 'bg-slate-500/20 text-slate-300 border-slate-500/30',
  Team: 'bg-cyan-500/20 text-cyan-300 border-cyan-500/30',
  'Mesh-candidate': 'bg-amber-500/20 text-amber-300 border-amber-500/30',
  Mesh: 'bg-emerald-500/20 text-emerald-300 border-emerald-500/30',
  Retired: 'bg-muted text-muted-foreground border-border',
};

export interface WorkspaceBarProps {
  workspaceId: string | null;
}

export function WorkspaceBar({ workspaceId }: WorkspaceBarProps): React.ReactElement {
  const workspace = useStudioWorkspaceStore((s) =>
    s.workspaces.find((w) => w.id === workspaceId),
  );
  const unread = useStudioInboxUnreadCount();
  const shareState: ShareState = workspace?.shareState ?? 'Private';

  return (
    <header
      data-testid="studio-workspace-bar"
      aria-label="Contributor Studio workspace bar"
      className="h-12 shrink-0 flex items-center justify-between gap-3 px-3 border-b border-border bg-background/80"
    >
      <div className="flex items-center gap-3 min-w-0">
        <Button
          variant="ghost"
          size="sm"
          aria-haspopup="listbox"
          aria-label="Workspace selector"
          className="gap-2"
        >
          <span className="text-xs text-muted-foreground">Workspace</span>
          <span className="font-medium text-sm truncate max-w-[220px]">
            {workspace?.name ?? 'Untitled'}
          </span>
        </Button>

        <span
          data-testid="studio-focus-pill"
          className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full text-xs border border-border bg-muted/40 text-foreground"
          title="Current focus"
        >
          <Target className="w-3 h-3" aria-hidden />
          {workspace?.focus.label ?? 'No focus yet'}
        </span>

        <span
          data-testid="studio-share-chip"
          className={`inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full text-xs font-medium border ${SHARE_TONE[shareState]}`}
          title={`Share state: ${shareState}`}
        >
          <CircleDot className="w-3 h-3" aria-hidden />
          {shareState}
        </span>
      </div>

      <div className="flex items-center gap-2">
        <span
          className="inline-flex items-center gap-1 text-xs text-muted-foreground"
          aria-live="polite"
        >
          Auto:
          <Badge variant={unread > 0 ? 'destructive' : 'default'}>{unread}</Badge>
        </span>

        <Button
          variant="ghost"
          size="sm"
          aria-label="Workspace settings"
          className="gap-1"
        >
          <Settings2 className="w-4 h-4" aria-hidden />
        </Button>
      </div>
    </header>
  );
}
