/**
 * AIPartnerLane - right pane (380px default).
 *
 * Partner selector, injected context panel, chat transcript, inbox chip.
 * Transport: `/api/ws/studio` (agent C1) + `studio_run_skill` (agent X1).
 * Spec: surface-spec §8.
 */

import React from 'react';
import { Bot, Inbox as InboxIcon } from 'lucide-react';
import {
  Badge,
  Button,
  ScrollArea,
  EmptyState,
} from '../../design-system/components';
import { useStudioPartnerStore } from '../stores/studioPartnerStore';
import { useStudioInboxUnreadCount } from '../stores/studioInboxStore';
import { useStudioWorkspaceStore } from '../stores/studioWorkspaceStore';
import { navigateToStudioPath } from '../routes';

export interface AIPartnerLaneProps {
  workspaceId: string;
}

export function AIPartnerLane({ workspaceId }: AIPartnerLaneProps): React.ReactElement {
  const transcript = useStudioPartnerStore(
    (s) => s.transcriptsByWorkspaceId[workspaceId] ?? [],
  );
  const unread = useStudioInboxUnreadCount();
  const partner = useStudioWorkspaceStore(
    (s) => s.workspaces.find((w) => w.id === workspaceId)?.partnerSelection ?? null,
  );

  return (
    <aside
      data-testid="studio-ai-partner-lane"
      aria-label="AI partner lane"
      className="h-full w-full flex flex-col border-l border-border bg-background/40"
    >
      <header className="shrink-0 flex items-center justify-between gap-2 px-3 py-2 border-b border-border">
        <Button
          variant="ghost"
          size="sm"
          className="gap-1.5"
          aria-label="Partner selector"
        >
          <Bot className="w-4 h-4" aria-hidden />
          <span className="truncate text-xs">
            {partner?.label ?? 'Private AI'}
          </span>
        </Button>

        <button
          data-testid="studio-inbox-chip"
          type="button"
          onClick={() => navigateToStudioPath('/studio/inbox')}
          className="inline-flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground"
          aria-label={`Inbox, ${unread} unread`}
        >
          <InboxIcon className="w-3.5 h-3.5" aria-hidden />
          <Badge variant={unread > 0 ? 'destructive' : 'default'}>{unread}</Badge>
        </button>
      </header>

      <section
        data-testid="studio-context-panel"
        aria-label="Context panel"
        className="shrink-0 px-3 py-2 border-b border-border/60 text-xs text-muted-foreground"
      >
        Focus + skills + policies will render here when the Sensei bridge
        (agent X1) is wired.
      </section>

      <ScrollArea className="flex-1">
        <div
          role="log"
          aria-label="Chat transcript"
          aria-live="polite"
          className="h-full"
        >
        {transcript.length === 0 ? (
          <div className="p-4">
            <EmptyState
              title="No partner messages yet"
              description="Start a conversation or invoke a skill from the command palette."
            />
          </div>
        ) : (
          <ul className="p-3 space-y-2">
            {transcript.map((m) => (
              <li
                key={m.id}
                className="text-xs border border-border rounded px-2 py-1.5 bg-muted/30"
              >
                <span className="font-medium mr-2 text-muted-foreground">
                  {m.author}
                </span>
                {m.content}
              </li>
            ))}
          </ul>
        )}
        </div>
      </ScrollArea>
    </aside>
  );
}
