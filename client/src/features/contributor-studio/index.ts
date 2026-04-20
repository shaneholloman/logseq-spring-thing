/**
 * Contributor Studio feature module - public surface.
 *
 * Default export is the route-level outlet component for React.lazy().
 * Named exports expose the route table, matcher, stores, and hooks.
 */

export {
  ContributorStudioOutlet,
  STUDIO_ROUTE_TABLE,
  matchStudioRoute,
  isStudioPath,
  navigateToStudioPath,
} from './routes';
export type { StudioRouteEntry } from './routes';

export { useStudioCommands, STUDIO_COMMAND_COUNT } from './hooks/useStudioCommands';

export { useStudioWorkspaceStore } from './stores/studioWorkspaceStore';
export type { StudioWorkspaceState, PaneLayout } from './stores/studioWorkspaceStore';

export { useStudioContextStore } from './stores/studioContextStore';
export type { StudioContextState } from './stores/studioContextStore';

export {
  useStudioPartnerStore,
} from './stores/studioPartnerStore';
export type { StudioPartnerState } from './stores/studioPartnerStore';

export { useSenseiStore } from './stores/senseiStore';
export type { SenseiState, TraceEvent } from './stores/senseiStore';

export {
  useStudioInboxStore,
  useStudioInboxUnreadCount,
} from './stores/studioInboxStore';
export type { StudioInboxState } from './stores/studioInboxStore';

export type {
  ShareState,
  PartnerCategory,
  PartnerSelection,
  DistributionScope,
  SkillRow,
  WorkspaceFocus,
  ContributorWorkspace,
  SenseiSuggestion,
  SenseiNudges,
  InboxItem,
  PartnerMessage,
  AutomationRow,
  StudioRouteMatch,
} from './types';

import { ContributorStudioOutlet } from './routes';
export default ContributorStudioOutlet;
