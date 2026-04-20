/**
 * SkillDojo - /studio/:workspaceId/skills/dojo.
 *
 * Skill discovery + eval surface. Wire-up to BC19 (agent C2) + MCP
 * skill_install / skill_eval_run (agent X1) pending.
 */

import React from 'react';
import { EmptyState } from '../../design-system/components';
import { BookOpen } from 'lucide-react';

export interface SkillDojoProps {
  workspaceId: string;
}

export function SkillDojo({ workspaceId: _workspaceId }: SkillDojoProps): React.ReactElement {
  return (
    <div data-testid="studio-skill-dojo" className="p-6 h-full">
      <div className="flex items-center gap-2 mb-3">
        <BookOpen className="w-5 h-5" aria-hidden />
        <h2 className="text-lg font-semibold">Skill Dojo</h2>
      </div>
      <EmptyState
        title="Discovery scaffolded"
        description="Skill catalogue, eval matrix, and benchmark comparisons wire up with BC19 backend (agent C2)."
      />
    </div>
  );
}
