/**
 * Contributor Studio - route table + hash-route matcher.
 *
 * ADR-046 prescribes react-router-dom v7 createBrowserRouter. That dependency
 * has not yet landed (the current App.tsx still uses `useHashRoute`). This
 * module exposes the canonical route list as data so it can be consumed two
 * ways today and reused verbatim when the dependency lands:
 *
 *   1. Legacy hash-route dispatch via `matchStudioRoute(hashPath)` +
 *      `<ContributorStudioOutlet />` rendering inside App.tsx.
 *   2. Future react-router-dom migration: `STUDIO_ROUTE_TABLE` maps one-to-one
 *      onto the `children` array of a `createBrowserRouter()` entry.
 *
 * Lazy loading is preserved via `React.lazy()` + `<Suspense>` per ADR-046.
 */

import React, { lazy, Suspense } from 'react';
import { LoadingSpinner } from '../design-system/components';
import type { StudioRouteMatch } from './types';

// -----------------------------------------------------------------------------
// Lazy chunks (one per sub-surface per ADR-046 §Route-Level Code Splitting).
// -----------------------------------------------------------------------------

const ContributorStudioRoot = lazy(() =>
  import('./components/ContributorStudioRoot').then((m) => ({
    default: m.ContributorStudioRoot,
  })),
);

const WorkspaceCreateWizard = lazy(() =>
  import('./components/WorkspaceCreateWizard').then((m) => ({
    default: m.WorkspaceCreateWizard,
  })),
);

const WorkspaceListView = lazy(() =>
  import('./components/WorkspaceListView').then((m) => ({
    default: m.WorkspaceListView,
  })),
);

const ArtifactDetail = lazy(() =>
  import('./components/ArtifactDetail').then((m) => ({
    default: m.ArtifactDetail,
  })),
);

const InstalledSkills = lazy(() =>
  import('./components/InstalledSkills').then((m) => ({
    default: m.InstalledSkills,
  })),
);

const SkillDojo = lazy(() =>
  import('./components/SkillDojo').then((m) => ({ default: m.SkillDojo })),
);

const SenseiTrace = lazy(() =>
  import('./components/SenseiTrace').then((m) => ({ default: m.SenseiTrace })),
);

const AutomationList = lazy(() =>
  import('./components/AutomationList').then((m) => ({
    default: m.AutomationList,
  })),
);

const AutomationDetail = lazy(() =>
  import('./components/AutomationDetail').then((m) => ({
    default: m.AutomationDetail,
  })),
);

const AutomationCreateWizard = lazy(() =>
  import('./components/AutomationCreateWizard').then((m) => ({
    default: m.AutomationCreateWizard,
  })),
);

const InboxView = lazy(() =>
  import('./components/InboxView').then((m) => ({ default: m.InboxView })),
);

// -----------------------------------------------------------------------------
// Route table (data).
// -----------------------------------------------------------------------------

export type StudioRouteEntry = {
  id: string;
  path: string;
  pattern: RegExp;
  render: (match: StudioRouteMatch) => React.ReactElement;
};

const spinner = (
  <div className="flex items-center justify-center h-full w-full">
    <LoadingSpinner />
  </div>
);

const wrap = (el: React.ReactElement): React.ReactElement => (
  <Suspense fallback={spinner}>{el}</Suspense>
);

export const STUDIO_ROUTE_TABLE: StudioRouteEntry[] = [
  {
    id: 'studio:index',
    path: '/studio',
    pattern: /^\/studio\/?$/,
    render: () => wrap(<WorkspaceListView />),
  },
  {
    id: 'studio:new',
    path: '/studio/new',
    pattern: /^\/studio\/new\/?$/,
    render: () => wrap(<WorkspaceCreateWizard />),
  },
  {
    id: 'studio:automations',
    path: '/studio/automations',
    pattern: /^\/studio\/automations\/?$/,
    render: () => wrap(<AutomationList />),
  },
  {
    id: 'studio:automations:new',
    path: '/studio/automations/new',
    pattern: /^\/studio\/automations\/new\/?$/,
    render: () => wrap(<AutomationCreateWizard />),
  },
  {
    id: 'studio:automations:detail',
    path: '/studio/automations/:id',
    pattern: /^\/studio\/automations\/([^/]+)\/?$/,
    render: (m) => wrap(<AutomationDetail automationId={m.automationId ?? ''} />),
  },
  {
    id: 'studio:inbox',
    path: '/studio/inbox',
    pattern: /^\/studio\/inbox\/?$/,
    render: () => wrap(<InboxView />),
  },
  {
    id: 'studio:workspace:artifact',
    path: '/studio/:workspaceId/artifacts/:aid',
    pattern: /^\/studio\/([^/]+)\/artifacts\/([^/]+)\/?$/,
    render: (m) =>
      wrap(
        <ContributorStudioRoot workspaceId={m.workspaceId ?? ''}>
          <ArtifactDetail
            workspaceId={m.workspaceId ?? ''}
            artifactId={m.artifactId ?? ''}
          />
        </ContributorStudioRoot>,
      ),
  },
  {
    id: 'studio:workspace:skills:dojo',
    path: '/studio/:workspaceId/skills/dojo',
    pattern: /^\/studio\/([^/]+)\/skills\/dojo\/?$/,
    render: (m) =>
      wrap(
        <ContributorStudioRoot workspaceId={m.workspaceId ?? ''}>
          <SkillDojo workspaceId={m.workspaceId ?? ''} />
        </ContributorStudioRoot>,
      ),
  },
  {
    id: 'studio:workspace:skills',
    path: '/studio/:workspaceId/skills',
    pattern: /^\/studio\/([^/]+)\/skills\/?$/,
    render: (m) =>
      wrap(
        <ContributorStudioRoot workspaceId={m.workspaceId ?? ''}>
          <InstalledSkills workspaceId={m.workspaceId ?? ''} />
        </ContributorStudioRoot>,
      ),
  },
  {
    id: 'studio:workspace:sensei',
    path: '/studio/:workspaceId/sensei',
    pattern: /^\/studio\/([^/]+)\/sensei\/?$/,
    render: (m) =>
      wrap(
        <ContributorStudioRoot workspaceId={m.workspaceId ?? ''}>
          <SenseiTrace workspaceId={m.workspaceId ?? ''} />
        </ContributorStudioRoot>,
      ),
  },
  {
    id: 'studio:workspace',
    path: '/studio/:workspaceId',
    pattern: /^\/studio\/([^/]+)\/?$/,
    render: (m) =>
      wrap(<ContributorStudioRoot workspaceId={m.workspaceId ?? ''} />),
  },
];

// -----------------------------------------------------------------------------
// Matcher.
// -----------------------------------------------------------------------------

export function matchStudioRoute(path: string): {
  entry: StudioRouteEntry;
  match: StudioRouteMatch;
} | null {
  for (const entry of STUDIO_ROUTE_TABLE) {
    const m = entry.pattern.exec(path);
    if (!m) continue;

    const match: StudioRouteMatch = { path };
    switch (entry.id) {
      case 'studio:workspace':
        match.workspaceId = m[1];
        break;
      case 'studio:workspace:skills':
      case 'studio:workspace:skills:dojo':
      case 'studio:workspace:sensei':
        match.workspaceId = m[1];
        break;
      case 'studio:workspace:artifact':
        match.workspaceId = m[1];
        match.artifactId = m[2];
        break;
      case 'studio:automations:detail':
        match.automationId = m[1];
        break;
      default:
        break;
    }
    return { entry, match };
  }
  return null;
}

export function isStudioPath(path: string): boolean {
  return path === '/studio' || path.startsWith('/studio/');
}

// -----------------------------------------------------------------------------
// Navigation helpers. Uses the existing hash-route convention; swap to
// react-router's `useNavigate()` when ADR-046 migration lands.
// -----------------------------------------------------------------------------

export function navigateToStudioPath(path: string): void {
  if (typeof window === 'undefined') return;
  window.location.hash = path;
}

/**
 * Top-level renderer for the Studio URL space. Drop into App.tsx alongside
 * the existing enterprise dispatch branch.
 */
export function ContributorStudioOutlet({ path }: { path: string }): React.ReactElement {
  const result = matchStudioRoute(path);
  if (!result) {
    return (
      <div className="p-6 text-sm text-muted-foreground">
        Unknown Studio route: <code>{path}</code>
      </div>
    );
  }
  return result.entry.render(result.match);
}
