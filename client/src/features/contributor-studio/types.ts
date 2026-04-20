/**
 * Contributor Studio - shared types.
 *
 * Mirrors the BC18/BC19 domain model fixed by ADR-057 at a surface level.
 * Server is authoritative; these are client-side projections only. See
 * `docs/design/2026-04-20-contributor-studio/01-contributor-studio-surface.md`.
 */

export type ShareState = 'Private' | 'Team' | 'Mesh-candidate' | 'Mesh' | 'Retired';

export type PartnerCategory =
  | 'private-ai'
  | 'team-ai'
  | 'human-collaborator'
  | 'scheduled-automation';

export type PartnerSelection = {
  category: PartnerCategory;
  id: string;
  label: string;
};

export type DistributionScope = 'Personal' | 'Team' | 'Mesh';

export type SkillRow = {
  id: string;
  name: string;
  version: string;
  scope: DistributionScope;
  evalPassRate: number | null;
  minModelTier: 1 | 2 | 3;
};

export type WorkspaceFocus = {
  nodeRef: string | null;
  label: string;
  lastUpdatedAt: string | null;
};

export type ContributorWorkspace = {
  id: string;
  name: string;
  focus: WorkspaceFocus;
  installedSkills: SkillRow[];
  shareState: ShareState;
  partnerSelection: PartnerSelection | null;
  createdAt: string;
};

export type SenseiSuggestion = {
  id: string;
  sectionId: 'terms' | 'concepts' | 'policies';
  label: string;
  rationale: string;
  confidence: number;
  sourceRef: string;
  type: 'term' | 'concept' | 'policy' | 'skill';
};

export type SenseiNudges = {
  terms: SenseiSuggestion[];
  concepts: SenseiSuggestion[];
  policies: SenseiSuggestion[];
};

export type InboxItem = {
  id: string;
  severity: 'low' | 'medium' | 'high';
  title: string;
  summary: string;
  disposition: 'pending' | 'accepted' | 'rejected' | 'archived';
  createdAt: string;
};

export type PartnerMessage = {
  id: string;
  sessionId: string;
  author: 'user' | 'partner' | 'tool';
  content: string;
  createdAt: string;
};

export type AutomationRow = {
  id: string;
  name: string;
  status: 'idle' | 'running' | 'errored' | 'inbox-unread';
  schedule: string;
  budgetTier: 1 | 2 | 3;
};

export type StudioRouteMatch = {
  path: string;
  workspaceId?: string;
  artifactId?: string;
  automationId?: string;
};
