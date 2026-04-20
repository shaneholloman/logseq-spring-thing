/**
 * useStudioCommands - registers the 15 Contributor Studio palette entries
 * listed in surface-spec §5. Commands are registered on mount of
 * ContributorStudioRoot and unregistered on unmount.
 */

import { useEffect } from 'react';
import {
  LayoutPanelLeft,
  FilePlus2,
  Repeat2,
  Share2,
  Sparkles,
  CalendarClock,
  Inbox,
  Check,
  X,
  ArrowUpFromLine,
  Waves,
  Activity,
  Crosshair,
  Send,
  Plus,
} from 'lucide-react';
import { commandRegistry } from '../../command-palette/CommandRegistry';
import type { Command } from '../../command-palette/types';
import { navigateToStudioPath } from '../routes';

const CATEGORY = 'Contributor Studio';

export function useStudioCommands(): void {
  useEffect(() => {
    const commands: Command[] = [
      {
        id: 'studio:open',
        title: 'Open Contributor Studio',
        category: CATEGORY,
        keywords: ['studio', 'contributor', 'open', 'go'],
        icon: LayoutPanelLeft,
        handler: () => navigateToStudioPath('/studio'),
      },
      {
        id: 'studio:new-workspace',
        title: 'New Workspace',
        category: CATEGORY,
        keywords: ['workspace', 'new', 'create'],
        icon: Plus,
        handler: () => navigateToStudioPath('/studio/new'),
      },
      {
        id: 'studio:switch-workspace',
        title: 'Switch Workspace\u2026',
        category: CATEGORY,
        keywords: ['workspace', 'switch', 'quick'],
        icon: Repeat2,
        handler: () => {
          // Wire-up: opens quick-switch modal (agent C1 surface follow-up).
          window.dispatchEvent(new CustomEvent('studio:switch-workspace'));
        },
      },
      {
        id: 'studio:share-artifact',
        title: 'Share Artifact\u2026',
        category: CATEGORY,
        keywords: ['share', 'intent', 'artifact', 'promote'],
        icon: Share2,
        handler: () => {
          window.dispatchEvent(new CustomEvent('studio:share-artifact'));
        },
      },
      {
        id: 'studio:run-skill',
        title: 'Run Skill\u2026',
        category: CATEGORY,
        keywords: ['skill', 'run', 'invoke'],
        icon: Sparkles,
        handler: () => {
          window.dispatchEvent(new CustomEvent('studio:run-skill'));
        },
      },
      {
        id: 'studio:new-automation',
        title: 'Schedule Automation\u2026',
        category: CATEGORY,
        keywords: ['automation', 'schedule', 'new', 'create'],
        icon: CalendarClock,
        handler: () => navigateToStudioPath('/studio/automations/new'),
      },
      {
        id: 'studio:inbox',
        title: 'Open Inbox',
        category: CATEGORY,
        keywords: ['inbox', 'review', 'unread'],
        icon: Inbox,
        handler: () => navigateToStudioPath('/studio/inbox'),
      },
      {
        id: 'studio:nudge-accept',
        title: 'Accept Nudge',
        category: CATEGORY,
        keywords: ['sensei', 'nudge', 'accept'],
        icon: Check,
        handler: () => {
          window.dispatchEvent(new CustomEvent('studio:accept-nudge'));
        },
      },
      {
        id: 'studio:nudge-dismiss',
        title: 'Dismiss Nudge',
        category: CATEGORY,
        keywords: ['sensei', 'nudge', 'dismiss'],
        icon: X,
        handler: () => {
          window.dispatchEvent(new CustomEvent('studio:dismiss-nudge'));
        },
      },
      {
        id: 'studio:promote-team',
        title: 'Promote to Team',
        category: CATEGORY,
        keywords: ['share', 'team', 'promote', 'private'],
        icon: ArrowUpFromLine,
        handler: () => {
          window.dispatchEvent(
            new CustomEvent('studio:promote', { detail: { target: 'Team' } }),
          );
        },
      },
      {
        id: 'studio:promote-mesh',
        title: 'Promote to Mesh',
        category: CATEGORY,
        keywords: ['share', 'mesh', 'promote', 'team'],
        icon: Waves,
        handler: () => {
          window.dispatchEvent(
            new CustomEvent('studio:promote', { detail: { target: 'Mesh' } }),
          );
        },
      },
      {
        id: 'studio:sensei-trace',
        title: 'Open Sensei Trace',
        category: CATEGORY,
        keywords: ['sensei', 'trace', 'debug'],
        icon: Activity,
        handler: () => {
          window.dispatchEvent(new CustomEvent('studio:open-sensei-trace'));
        },
      },
      {
        id: 'studio:pick-from-graph',
        title: 'Pick from Graph',
        category: CATEGORY,
        keywords: ['graph', 'pick', 'split', 'browse'],
        icon: Crosshair,
        handler: () => {
          window.dispatchEvent(new CustomEvent('studio:pick-from-graph'));
        },
      },
      {
        id: 'studio:send-to-studio',
        title: 'Send to Studio',
        category: CATEGORY,
        keywords: ['graph', 'send', 'focus', 'context'],
        icon: Send,
        handler: () => {
          window.dispatchEvent(new CustomEvent('studio:send-to-studio'));
        },
      },
      {
        id: 'studio:new-artifact',
        title: 'New Artifact',
        category: CATEGORY,
        keywords: ['artifact', 'new', 'create', 'draft'],
        icon: FilePlus2,
        handler: () => {
          window.dispatchEvent(new CustomEvent('studio:new-artifact'));
        },
      },
    ];

    commandRegistry.registerCommands(commands);

    return () => {
      for (const cmd of commands) {
        commandRegistry.unregisterCommand(cmd.id);
      }
    };
  }, []);
}

export const STUDIO_COMMAND_COUNT = 15;
