/**
 * WorkLane - centre pane (flex).
 *
 * Radix Tabs: Editor | Graph | Preview | Diff. Persisted per workspace.
 * Embedded graph reuses `WasmSceneEffects` (agent E1 owns bridge extensions).
 * Spec: surface-spec §7.
 */

import React, { useState } from 'react';
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
  EmptyState,
} from '../../design-system/components';
import { Edit3, Network, Eye, GitCompare } from 'lucide-react';

export interface WorkLaneProps {
  workspaceId: string;
  children?: React.ReactNode;
}

type WorkLaneTab = 'editor' | 'graph' | 'preview' | 'diff';

export function WorkLane({ workspaceId, children }: WorkLaneProps): React.ReactElement {
  const [tab, setTab] = useState<WorkLaneTab>('editor');

  if (children) {
    return (
      <section
        data-testid="studio-work-lane"
        aria-label="Work lane"
        className="h-full w-full flex flex-col bg-background"
      >
        {children}
      </section>
    );
  }

  return (
    <section
      data-testid="studio-work-lane"
      aria-label="Work lane"
      className="h-full w-full flex flex-col bg-background"
    >
      <Tabs
        value={tab}
        onValueChange={(v) => setTab(v as WorkLaneTab)}
        className="flex-1 flex flex-col"
      >
        <TabsList className="px-3 py-1 shrink-0">
          <TabsTrigger value="editor" className="gap-1.5">
            <Edit3 className="w-3.5 h-3.5" /> Editor
          </TabsTrigger>
          <TabsTrigger value="graph" className="gap-1.5">
            <Network className="w-3.5 h-3.5" /> Graph
          </TabsTrigger>
          <TabsTrigger value="preview" className="gap-1.5">
            <Eye className="w-3.5 h-3.5" /> Preview
          </TabsTrigger>
          <TabsTrigger value="diff" className="gap-1.5">
            <GitCompare className="w-3.5 h-3.5" /> Diff
          </TabsTrigger>
        </TabsList>

        <TabsContent value="editor" className="flex-1 p-4">
          <EmptyState
            title="Editor"
            description={`Markdown draft surface for workspace ${workspaceId}. Wire-up pending BC18 backend (agent C1).`}
          />
        </TabsContent>
        <TabsContent value="graph" className="flex-1 p-4">
          <EmptyState
            title="Embedded graph view"
            description="WasmSceneEffects reuses the /graph scene; agent E1 owns the bridge extensions."
          />
        </TabsContent>
        <TabsContent value="preview" className="flex-1 p-4">
          <EmptyState title="Artifact preview" description="Select an artifact to preview." />
        </TabsContent>
        <TabsContent value="diff" className="flex-1 p-4">
          <EmptyState
            title="ShareIntent diff"
            description="No active ShareIntent. Promote an artifact to populate."
          />
        </TabsContent>
      </Tabs>
    </section>
  );
}
