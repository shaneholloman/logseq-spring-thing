/**
 * InboxView - /studio/inbox.
 *
 * Headless automation output review queue. Each item lands through
 * `/inbox/{agent-ns}/*.json` per surface-spec §8.4 + 03-pod-context §2.
 */

import React, { useEffect } from 'react';
import { Inbox as InboxIcon } from 'lucide-react';
import { EmptyState, Button, Badge } from '../../design-system/components';
import { useStudioInboxStore } from '../stores/studioInboxStore';

export function InboxView(): React.ReactElement {
  const items = useStudioInboxStore((s) => s.items);
  const fetchInbox = useStudioInboxStore((s) => s.fetchInbox);
  const markRead = useStudioInboxStore((s) => s.markRead);
  const ack = useStudioInboxStore((s) => s.ack);

  useEffect(() => {
    void fetchInbox();
  }, [fetchInbox]);

  return (
    <div
      data-testid="studio-inbox-view"
      className="h-full w-full p-6 bg-[#000022] flex flex-col gap-4"
    >
      <header className="flex items-center gap-2">
        <InboxIcon className="w-5 h-5" aria-hidden />
        <h1 className="text-xl font-semibold">Inbox</h1>
        <Badge variant="default">{items.length}</Badge>
      </header>

      {items.length === 0 ? (
        <EmptyState
          title="Inbox empty"
          description="Headless automations and delegated skill runs land here for review."
        />
      ) : (
        <ul className="space-y-2">
          {items.map((item) => (
            <li
              key={item.id}
              className="flex items-center justify-between gap-3 p-3 rounded border border-border"
            >
              <div className="min-w-0">
                <div className="font-medium text-sm truncate">{item.title}</div>
                <div className="text-xs text-muted-foreground truncate">
                  {item.summary}
                </div>
              </div>
              <div className="flex items-center gap-2 shrink-0">
                <Button variant="ghost" size="sm" onClick={() => markRead(item.id)}>
                  Mark read
                </Button>
                <Button variant="outline" size="sm" onClick={() => void ack(item.id)}>
                  Ack
                </Button>
              </div>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
