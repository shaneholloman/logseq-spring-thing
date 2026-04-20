/**
 * PaneLayout - four-pane shell (left | centre | right) + bottom rail.
 *
 * Uses react-resizable-panels (already in dependencies) rather than a new
 * SplitPane primitive. Pane widths/collapse state are mirrored into
 * `studioWorkspaceStore.layout` so the values persist when the backend write
 * path lands (agent C1).
 *
 * Spec: surface-spec §2, §17.
 */

import React from 'react';
import {
  Panel,
  PanelGroup,
  PanelResizeHandle,
} from 'react-resizable-panels';
import { useStudioWorkspaceStore } from '../stores/studioWorkspaceStore';

export interface PaneLayoutProps {
  left: React.ReactNode;
  centre: React.ReactNode;
  right: React.ReactNode;
  bottom?: React.ReactNode;
}

const HANDLE_CLASS =
  'w-1 bg-border/40 hover:bg-cyan-400/40 transition-colors cursor-col-resize';

export function PaneLayout({
  left,
  centre,
  right,
  bottom,
}: PaneLayoutProps): React.ReactElement {
  const layout = useStudioWorkspaceStore((s) => s.layout);
  const setLayout = useStudioWorkspaceStore((s) => s.setLayout);

  return (
    <div
      data-testid="studio-pane-layout"
      className="flex-1 min-h-0 flex flex-col"
    >
      <PanelGroup direction="horizontal" className="flex-1">
        <Panel
          id="studio-left"
          order={1}
          defaultSize={20}
          minSize={14}
          maxSize={32}
          collapsible
          onResize={(size) => setLayout({ leftWidth: Math.round(size * 16) })}
          className="min-w-0"
        >
          {left}
        </Panel>

        <PanelResizeHandle className={HANDLE_CLASS} aria-label="Resize guide rail" />

        <Panel id="studio-centre" order={2} minSize={40} className="min-w-0">
          {centre}
        </Panel>

        <PanelResizeHandle className={HANDLE_CLASS} aria-label="Resize partner lane" />

        <Panel
          id="studio-right"
          order={3}
          defaultSize={24}
          minSize={18}
          maxSize={36}
          collapsible
          onResize={(size) => setLayout({ rightWidth: Math.round(size * 16) })}
          className="min-w-0"
        >
          {right}
        </Panel>
      </PanelGroup>

      {bottom ? (
        <div
          data-testid="studio-pane-layout-bottom"
          aria-hidden={!layout.memoryBarExpanded && undefined}
        >
          {bottom}
        </div>
      ) : null}
    </div>
  );
}
