# PRD-002: Enterprise Control Plane UI

**Status**: Proposed
**Author**: Architecture Agent
**Date**: 2026-04-14
**Priority**: P1 -- Required for enterprise pilot readiness
**Depends On**: ADR-040 (Enterprise Identity), ADR-041 (Broker Workbench), ADR-042 (Workflow Proposals), ADR-043 (KPI Lineage), ADR-044 (Connector Governance), ADR-045 (Policy Engine)

---

## Problem Statement

VisionClaw has five backend bounded contexts for enterprise governance (BC11 Broker Workbench, Workflow Proposals, KPI Lineage, Connector Governance, Policy Engine) with live REST and WebSocket endpoints. The client has no UI surfaces for any of them. Enterprise users -- Judgment Brokers, Transformation Leaders, Auditors -- cannot use the platform for its intended purpose without these surfaces.

The existing client is a full-screen 3D graph visualisation with a side-panel control system (DashboardControlPanel, PerformanceControlPanel, AgentControlPanel). That architecture works for graph exploration but does not accommodate data-dense enterprise workflows: filtered lists, case detail views, decision forms, KPI dashboards, configuration wizards.

This PRD specifies five enterprise control plane surfaces that consume the existing backend APIs and extend the existing design system (Radix UI v3 + Tailwind CSS v4 + Framer Motion) without replacing or degrading the graph visualisation experience.

## Goals

- Ship five enterprise surfaces: Broker Workbench, Workflow Studio, Mesh KPI Dashboard, Connector Management, Policy Console
- Extend, not replace, the existing design system and navigation model
- Maintain the VisionClaw dark-theme aesthetic (cosmic/crystalline/bioluminescent)
- Support real-time updates for time-sensitive surfaces (Broker Inbox, KPI values)
- Desktop-first, information-dense layouts suitable for enterprise power users
- Keyboard-navigable with command palette integration
- Progressive WASM enhancement for high-performance visualisation components

## Non-Goals

- Replacing the 3D graph visualisation or its existing control panels
- Building a mobile-native experience (responsive tablet support is in scope; phone is not)
- Implementing backend API changes (all endpoints referenced here are already live or defined in ADR-041 through ADR-045)
- Building real-time collaboration features (multi-broker concurrent editing)
- Implementing the automated Discovery Engine or Insight Ingestion Loop backend logic

---

## Technology Context

| Layer | Technology | Version | Notes |
|-------|-----------|---------|-------|
| Framework | React | 19 | Concurrent features available |
| Build | Vite | latest | Module federation not needed; single SPA |
| Language | TypeScript | strict mode | Generated types from Rust via `specta` |
| Design System | Radix UI | v3 (`@radix-ui/themes` 3.2.1) | 25+ components in `client/src/features/design-system/components/` |
| Styling | Tailwind CSS | v4.1.7 | HSL custom property tokens |
| State | Zustand + Immer | 5.x / 11.1.3 | Existing pattern: `create()` with `produce()` |
| 3D | React Three Fiber + Three.js | 9.5.0 / 0.183.0 | Embedded mini-graphs in Decision Canvas |
| Animation | Framer Motion | 12.23.26 | Existing animation presets in `design-system/animations.ts` |
| Icons | Lucide React | 0.562.0 | Consistent with existing panels |
| WASM | wasm-pack + scene-effects bridge | Custom | Zero-copy Float32Array pattern established |

---

## Navigation Architecture

### Current State

The client is a single-route application. `MainLayout.tsx` renders a full-screen `GraphCanvasWrapper` with an `IntegratedControlPanel` overlay. There is no router. Navigation between control panels happens via tabs within the side panel.

### Target State

Introduce a lightweight client-side router that preserves the current graph view as the default route and adds enterprise routes as peer top-level views. The 3D canvas does not render on enterprise routes (it is computationally expensive and irrelevant to tabular enterprise workflows).

```
/                       -> Graph Visualisation (current MainLayout)
/broker                 -> Broker Workbench
/broker/:caseId         -> Decision Canvas (full case detail)
/workflows              -> Workflow Studio
/workflows/:proposalId  -> Proposal Detail
/kpi                    -> Mesh KPI Dashboard
/kpi/:metric            -> KPI Drill-down
/connectors             -> Connector Management
/connectors/:id/config  -> Connector Configuration Wizard
/policy                 -> Policy Console
/policy/:ruleId         -> Rule Editor
```

### Left Sidebar

A persistent left sidebar replaces the current implicit navigation. It contains:

| Section | Icon | Route | Badge |
|---------|------|-------|-------|
| Graph | `Network` | `/` | -- |
| Broker | `Scale` | `/broker` | Open case count (live) |
| Workflows | `GitBranch` | `/workflows` | Pending review count |
| KPIs | `BarChart3` | `/kpi` | -- |
| Connectors | `Plug` | `/connectors` | Error count (if any) |
| Policy | `Shield` | `/policy` | -- |
| Settings | `Settings` | (existing panel) | -- |

The sidebar is 56px wide (icon-only) by default, expandable to 200px (icon + label) on hover or pin. It uses the existing dark theme colour tokens. Active route is indicated by an HSL accent highlight on the icon.

Badge values are driven by lightweight polling or WebSocket subscription counts from the respective stores. Badges use the existing `Badge` component with `variant="destructive"` for errors and `variant="default"` for counts.

### Command Palette Integration

The existing command palette (`Cmd+K`) gains enterprise commands registered by each feature module:

| Command | Action |
|---------|--------|
| `Go to Broker Inbox` | Navigate to `/broker` |
| `Go to Workflow Studio` | Navigate to `/workflows` |
| `Go to KPI Dashboard` | Navigate to `/kpi` |
| `Open Case {id}` | Navigate to `/broker/{id}` |
| `Open Proposal {id}` | Navigate to `/workflows/{id}` |
| `Search Cases...` | Open broker search |
| `Search Workflows...` | Open workflow search |

Commands are registered via the existing `CommandRegistry` pattern using the `Command` interface from `client/src/features/command-palette/types.ts`.

---

## Surface 1: Broker Workbench

**Route**: `/broker`
**Backend**: ADR-041 endpoints (`/api/broker/inbox`, `/api/broker/cases/{id}`, etc.)
**Role**: Broker (ADR-040)

### 1.1 Broker Inbox

The primary landing view. A filterable, sortable, keyboard-navigable list of open broker cases.

**Layout**: Full-width table with the following columns:

| Column | Source Field | Width | Sortable | Filterable |
|--------|-------------|-------|----------|------------|
| Priority | `priority` (0-100) | 64px | Yes (default: desc) | Yes (range slider) |
| Status | `status` | 96px | Yes | Yes (multi-select: open, under_review, delegated) |
| Category | `category` | 120px | Yes | Yes (multi-select: escalation, workflow_review, trust_alert, policy_exception, manual) |
| Title | `title` | flex | No | Yes (text search) |
| Source | `source_event_id` + type icon | 48px | No | Yes (by source type) |
| Age | computed from `created_at` | 96px | Yes | Yes (range: <1h, <24h, <7d, >7d) |
| Assigned | `assigned_broker` display name | 120px | Yes | Yes (assigned/unassigned) |

**Interactions**:
- Click row -> navigate to `/broker/{id}` (Decision Canvas)
- `Enter` on focused row -> same
- `c` key on focused row -> claim case (POST `/api/broker/cases/{id}/claim`)
- `r` key on focused row -> release case (POST `/api/broker/cases/{id}/release`)
- Filter controls in a collapsible header bar
- Sort by clicking column headers (tri-state: asc, desc, none)
- Pagination: 50 items per page, cursor-based (backend `?limit=50&cursor=...`)

**Real-time updates**: WebSocket subscription to `broker:new_case`, `broker:case_claimed`, `broker:case_updated`, `broker:priority_changed`. New cases appear at the top with a Framer Motion slide-down animation. Claimed cases update in-place with a brief highlight flash.

**Priority badges**: Colour-coded using HSL tokens:
- Critical (80-100): `--destructive` (red)
- High (60-79): `--warning` (amber)
- Medium (30-59): `--accent` (blue)
- Low (0-29): `--muted` (grey)

**Empty state**: When no cases exist, display a centered illustration with text: "No open cases. The mesh is running smoothly." with a CTA button for manual submission.

### 1.2 Decision Canvas

**Route**: `/broker/:caseId`

A full-screen case detail view optimised for broker decision-making. Three-column layout:

```
+-------------------+---------------------------+-------------------+
|                   |                           |                   |
|   Evidence Panel  |   Decision Centre         |   Graph Context   |
|   (320px fixed)   |   (flex, min 480px)       |   (360px fixed)   |
|                   |                           |                   |
+-------------------+---------------------------+-------------------+
```

**Evidence Panel (left)**:
- Provenance tree: collapsible tree view showing the bead lifecycle trail (ADR-034)
- Policy evaluations: list of `PolicyEvaluated` results for this case
- Similar past decisions: cards showing 3-5 semantically similar previous broker decisions with outcomes
- Agent activity: timeline of relevant agent actions from BC8
- Each evidence section is a collapsible `Collapsible` component (existing design system)

**Decision Centre (middle)**:
- Case header: title, category badge, priority badge, status badge, age, assigned broker
- Summary: full case description with markdown rendering (existing `MarkdownRenderer` from design-system patterns)
- Suggested decision: hidden behind a "Show AI Suggestion" button to prevent automation bias (ADR-041). Displayed with a confidence percentage and reasoning. Uses a distinct visual treatment (dashed border, muted background) to distinguish from human-authored content.
- Decision actions (see 1.3 below)
- Decision history: if the case was previously claimed/released/delegated, show that trail

**Graph Context (right)**:
- Embedded React Three Fiber mini-graph showing the subgraph of nodes related to this case
- Uses the existing `GraphCanvasWrapper` component with a constrained viewport (no full-screen controls)
- Node selection in the mini-graph populates a tooltip with node metadata
- Click-through to the full graph view with the selected node highlighted: navigates to `/?focus={nodeId}`
- Fallback: if the case has no graph context (manual submissions), show "No graph context available" with the same empty-state treatment

### 1.3 Decision Actions

A sticky action bar at the bottom of the Decision Centre:

| Button | Variant | Keyboard | Requires |
|--------|---------|----------|----------|
| Approve | `default` (green accent) | `Ctrl+Enter` | Reasoning textarea (optional) |
| Reject | `destructive` | `Ctrl+Shift+R` | Reasoning textarea (required) |
| Amend | `outline` | `Ctrl+Shift+A` | Amendments JSON editor + reasoning |
| Delegate | `outline` | `Ctrl+Shift+D` | Delegate-to selector + reason |
| Promote as Workflow | `secondary` | `Ctrl+Shift+W` | Reasoning |
| Mark as Precedent | `secondary` | `Ctrl+Shift+P` | Scope selector + reasoning |

All actions POST to `/api/broker/cases/{id}/decide` with the appropriate action payload. On success, navigate back to `/broker` with a toast confirmation. On 409 (case already decided), show an error toast and refresh case state.

The reasoning textarea is a `Textarea` component (existing design system) with markdown preview toggle. Minimum 10 characters for reject actions.

### 1.4 Broker Timeline

**Route**: `/broker?tab=timeline` (tab within Broker Workbench)

A chronological list of the current broker's past decisions. Each entry shows:
- Decision date, case title, action taken, reasoning excerpt
- Outcome tracking: if the decision led to a deployed workflow, show the deployment status
- Link to full case detail

Filterable by action type, date range, and case category. Uses the same table component as the inbox.

---

## Surface 2: Workflow Studio

**Route**: `/workflows`
**Backend**: ADR-042 endpoints (`/api/workflows/proposals`, `/api/workflows/proposals/{id}`, etc.)
**Role**: Broker, Contributor (ADR-040)

### 2.1 Proposal List

A card grid (3 columns on desktop, 2 on tablet) of workflow proposals. Each card shows:

- Title
- Status badge: colour-coded by lifecycle stage
  - Draft: grey
  - Submitted: blue
  - Under Review: amber
  - Approved: green
  - Deployed: green with checkmark icon
  - Archived: muted with strikethrough
- Source indicator: discovery (robot icon) or manual (user icon)
- Risk score: horizontal progress bar (0-1.0), coloured red above 0.7, amber above 0.4, green below
- Owner: avatar + display name
- Last updated: relative time
- Version count badge

Cards use the existing `Card` component with hover elevation via Framer Motion `whileHover={{ y: -2 }}`.

**Filters**: Status multi-select, source type, owner, risk score range. Persistent in URL query params.

**Actions**:
- Click card -> navigate to `/workflows/{proposalId}`
- "New Proposal" button -> opens a creation dialog with title, description, source type fields

### 2.2 Proposal Detail

**Route**: `/workflows/:proposalId`

Two-column layout:

```
+---------------------------+-------------------+
|                           |                   |
|   Step Editor             |   Version History |
|   (flex, min 560px)       |   (320px fixed)   |
|                           |                   |
+---------------------------+-------------------+
```

**Step Editor (left)**:
- Visual representation of workflow steps as a vertical DAG
- Each step is a card showing: type icon, description, inputs/outputs, timeout/SLA
- Steps connected by directional lines (SVG or Canvas2D; WASM-accelerated DAG layout as progressive enhancement)
- Conditional steps show branching paths
- Edit mode (for draft status): drag to reorder, click to edit step details in a slide-over panel, "Add Step" button
- Read-only mode (for submitted/under_review): steps are not editable but inspectable

**Version History (right)**:
- List of `WorkflowVersion` entries with version number, author, date, change summary
- Click version -> highlight diff against previous version in the step editor
- Diff view: added steps (green border), removed steps (red border with strikethrough), modified steps (amber border with field-level diff tooltip)
- Diff computation calls the application-layer diff endpoint (ADR-042)

**Action bar** (sticky bottom, visible only for appropriate roles/statuses):
- Draft: "Submit for Review" button
- Under Review (broker): Approve / Reject / Request Amendments buttons (mirrors broker decision actions)
- Approved (admin): "Deploy" button with scope selector (team / department / org-wide)
- Deployed: "Rollback" button with confirmation dialog

### 2.3 Pattern Library

**Route**: `/workflows?tab=patterns`

A catalogue of deployed `WorkflowPattern` entities. Each pattern card shows:
- Title, deployment scope, deployed date
- Usage count (adoption metric)
- Active version number
- "View" button -> navigates to the proposal detail for the active version

Sortable by usage count (most-adopted first), deployment date, and scope.

### 2.4 Promotion Flow

A visual pipeline indicator displayed at the top of each proposal detail view:

```
Draft -> Submitted -> Under Review -> Approved -> Deployed
  [o]       [o]          [*]           [ ]         [ ]
```

Each stage is a circle connected by lines. Completed stages are filled. The current stage pulses with a subtle Framer Motion animation (`animate={{ scale: [1, 1.1, 1] }}, transition: { repeat: Infinity, duration: 2 }`). Clicking a completed stage shows a tooltip with the actor and timestamp of that transition.

---

## Surface 3: Mesh KPI Dashboard

**Route**: `/kpi`
**Backend**: ADR-043 endpoints (`/api/mesh-metrics`, `/api/mesh-metrics/{kpi}`, etc.)
**Role**: Broker, Auditor, Transformation Leader (ADR-040)

### 3.1 KPI Cards

Four cards in a 2x2 grid (single row on wide desktop):

| KPI | Unit | Display | Colour Logic |
|-----|------|---------|-------------|
| Mesh Velocity | hours | Value + sparkline + confidence band | Green < 48h, amber 48-168h, red > 168h |
| Augmentation Ratio | percentage | Value + sparkline + target line | Green > 80%, amber 60-80%, red < 60% |
| Trust Variance | stddev | Value + sparkline + threshold line | Green < 0.15, amber 0.15-0.3, red > 0.3 |
| HITL Precision | percentage | Value + sparkline + target line | Green > 70%, amber 50-70%, red < 50% |

Each card contains:
- KPI name and icon
- Current value (large, prominent)
- Confidence indicator: filled circle (high), half-filled (medium), outline (low) -- derived from `confidence` field
- Sparkline: 30-point time series rendered as an inline SVG (pure TypeScript Canvas2D baseline; WASM `kpi-sparklines` module as progressive enhancement -- see ADR-047)
- Confidence band: translucent fill around the sparkline representing the confidence range
- Trend indicator: up/down/flat arrow with percentage change vs previous window

**Click interaction**: Click a KPI card to navigate to `/kpi/{metric}` drill-down view.

### 3.2 Time Window Selector

A segmented control (`Tabs` component) at the top of the dashboard:
- 24h | 7d | 30d | 90d
- Changing the window re-fetches all KPI data for the selected range
- Default: 7d
- Persisted in URL query param (`?window=7d`)

### 3.3 Dimension Slicing

A filter bar below the time selector:
- Team: multi-select dropdown
- Function: multi-select dropdown
- Workflow Type: multi-select dropdown
- Agent Type: multi-select dropdown

Dimension values are populated from the backend `/api/mesh-metrics/{kpi}/slice` endpoint. Selecting dimensions re-fetches KPI values filtered to those dimensions.

### 3.4 KPI Drill-down

**Route**: `/kpi/:metric`

Full-page detail for a single KPI:
- Large sparkline chart (expanded from card size) with zoom/pan
- Lineage table: list of source events that contributed to the current snapshot value
  - Columns: event type, event ID, timestamp, actor, contribution description
  - Click event -> navigate to the source entity (broker case, workflow proposal, etc.)
- Historical trend: line chart of metric values over time with confidence bands
- Export button: CSV download of the time series + lineage data via `/api/mesh-metrics/export`

### 3.5 Real-time Updates

The dashboard subscribes to `MetricSnapshotCreated` events via WebSocket. When a new snapshot arrives for a displayed KPI:
1. The value animates from old to new using Framer Motion's `useMotionValue` + `useTransform`
2. The sparkline appends the new point with a slide-left animation
3. The trend arrow updates

Polling fallback: if WebSocket is unavailable, poll `/api/mesh-metrics` every 60 seconds.

---

## Surface 4: Connector Management

**Route**: `/connectors`
**Backend**: ADR-044 endpoints (`/api/connectors`, `/api/connectors/{id}`, etc.)
**Role**: Admin (ADR-040)

### 4.1 Connector List

A table of configured connectors:

| Column | Source | Width |
|--------|--------|-------|
| Name | `display_name` | flex |
| Type | `connector_type` + icon | 96px |
| Status | `status` | 96px (colour-coded: active=green, paused=amber, error=red, disabled=grey) |
| Last Sync | `sync_state.last_sync_at` | 128px (relative time) |
| Signals | count of Insights from this connector | 80px |
| Legal Review | `legal_review_mode` toggle | 80px |
| Actions | edit / sync / pause | 120px |

**Interactions**:
- Click row -> navigate to `/connectors/{id}/config`
- "Add Connector" button -> opens configuration wizard (4.2)
- "Sync Now" button -> POST `/api/connectors/{id}/sync`, shows toast on completion
- Pause/Resume toggle -> PUT `/api/connectors/{id}` with updated status

### 4.2 Configuration Wizard

**Route**: `/connectors/new` or `/connectors/{id}/config`

A multi-step wizard using the existing `Tabs` component as step indicators:

1. **Connector Type**: Select from available types (GitHub Issues/PRs initially). Each type shows a description and required permissions.
2. **Authentication**: OAuth2 flow or personal access token input. Validate credentials with POST to `/api/connectors/validate` (calls `ConnectorAdapter::validate_config`).
3. **Scope**: Configure which repositories/projects/channels to ingest. Dynamic form based on connector type. GitHub: owner input, repo multi-select, event type checkboxes, label/state filters.
4. **Privacy**: Select redaction policy (none/standard/aggressive/custom). If custom, provide regex patterns. Toggle legal review mode. Show explanation of what each policy redacts.
5. **Schedule**: Set sync interval (minutes). Preview first sync scope.
6. **Review & Create**: Summary of all settings. "Create Connector" button.

Each step validates before allowing progression. Back/Next navigation with keyboard (`Alt+Left` / `Alt+Right`).

### 4.3 Signal Feed

**Route**: `/connectors?tab=signals`

A chronological feed of recent discovery signals (Insights) from all connectors:
- Each signal shows: connector source icon, title, timestamp, redaction indicator (if any fields were redacted)
- Signals pending legal review are highlighted with an amber badge
- Click signal -> view full Insight detail (fields, provenance, linked graph entities)

### 4.4 Redaction Rule Editor

**Route**: `/connectors/{id}/config?tab=redaction`

Within the connector configuration view, a tab for managing custom redaction rules:
- List of active rules (regex pattern, rule name, enabled toggle)
- "Add Rule" form: name, regex pattern, test input field with live preview of redaction
- Test bench: paste sample text, see which rules match and what gets redacted
- Changes saved to the connector's `redaction_policy` configuration

---

## Surface 5: Policy Console

**Route**: `/policy`
**Backend**: ADR-045 endpoints (`/api/policy`, `/api/policy/{rule_id}`, etc.)
**Role**: Admin (ADR-040)

### 5.1 Rule List

A table of all policy rules:

| Column | Source | Width |
|--------|--------|-------|
| Rule ID | `rule_id` | 200px |
| Description | `description` | flex |
| Enabled | `enabled` | 64px (Switch toggle) |
| Last Evaluated | from evaluation log | 128px |
| Hit Rate | evaluations resulting in non-Allow outcomes / total evaluations | 96px |

Toggle switches use the existing `Switch` component. Toggling a rule fires PUT `/api/policy/{rule_id}` with `{ enabled: true|false }`. Changes are optimistically applied with rollback on error.

### 5.2 Rule Editor

**Route**: `/policy/:ruleId`

A form-based editor for policy rule configuration. The form fields are dynamically generated from the rule's configuration schema:

| Rule | Form Fields |
|------|-------------|
| `escalation_threshold` | Threshold slider (0-1.0), applies-to workflow type multi-select, action-on-breach select |
| `domain_ownership` | Domain-to-pubkey mapping table (add/remove rows) |
| `confidence_threshold` | Min confidence slider, action select (deny/escalate) |
| `separation_of_duty` | Applies-to action multi-select |
| `deployment_scope` | Max scope without escalation select (team/department/org-wide) |
| `rate_limit` | Max actions per agent input, window minutes input, action-on-breach select |

Each form uses the existing design system components (Slider, Select, Input, Switch). Changes are saved with a "Save" button (PUT `/api/policy/{rule_id}`). A "Reset to Defaults" button restores factory values.

The TOML configuration backing is transparent to the user; they interact with form controls, not raw TOML.

### 5.3 Evaluation Log

**Route**: `/policy?tab=evaluations`

A table of recent policy evaluations:

| Column | Source | Width |
|--------|--------|-------|
| Timestamp | `evaluated_at` | 128px |
| Rule | `rule_id` | 160px |
| Outcome | `outcome` (Allow/Deny/Escalate/Warn) | 96px (colour-coded) |
| Action | `context_action` | 160px |
| Resource | `context_resource_id` (linked) | flex |
| Reasoning | `reasoning` (truncated) | flex |

Click row -> expand inline to show full reasoning and context. Click resource link -> navigate to the referenced entity (workflow proposal, broker case, etc.).

Filters: rule ID, outcome, date range, action type.

### 5.4 Policy Test Bench

**Route**: `/policy?tab=test`

A form for simulating policy evaluations against hypothetical contexts:
- Actor: select from known users/agents
- Action: select from `PolicyAction` enum
- Resource: select or enter a resource reference
- Metadata: JSON editor for additional context fields
- "Evaluate" button -> POST to a test evaluation endpoint (dry-run, no provenance event emitted)
- Results panel: shows all rule evaluations with outcomes and reasoning

---

## Design Principles

### Dark Theme Continuity

All enterprise surfaces inherit the VisionClaw dark theme. The background is `#000022` (from `MainLayout.tsx`). Text uses the existing HSL token hierarchy:
- `--foreground` for primary text
- `--muted-foreground` for secondary text
- `--accent` for interactive elements
- `--destructive` for error states and critical priority

No light theme is planned for initial release. All new components must work on dark backgrounds.

### Information Density

Enterprise users need data, not whitespace. Design guidelines:
- Table rows: 36px height (compact), 44px (comfortable). User toggle in sidebar settings.
- Card padding: 12px (vs typical 16-24px)
- Font size: 13px body, 11px metadata, 16px headings
- No decorative illustrations or hero sections on data views
- Collapsible sections default to expanded on desktop

### Cosmic Aesthetic

Subtle visual cues that maintain the VisionClaw identity:
- Card borders use a 1px `--border` with a subtle gradient shimmer on hover (CSS `border-image` with animated HSL hue rotation, duration 4s, subtle -- opacity 0.3)
- Priority badges use the bioluminescent glow effect: `box-shadow: 0 0 8px hsl(var(--priority-hue) 70% 50% / 0.3)`
- KPI sparklines use the crystalline colour palette (cyan-to-violet gradient)
- Loading states use the existing `LoadingSkeleton` with a gentle pulse

### Micro-interactions

All state transitions use Framer Motion with the presets from `client/src/features/design-system/animations.ts`:
- Panel open/close: `variants.slideLeft` (slide from right edge)
- Table row appear: `variants.slideDown` with staggered delay (10ms per row, max 20 rows)
- Badge status change: `variants.scale` with `transitions.spring.snappy`
- KPI value change: `useMotionValue` with spring interpolation
- Page transitions: cross-fade with `variants.scale` (150ms)

### Keyboard Navigation

Power users expect keyboard-first interaction:
- `Tab` / `Shift+Tab`: move between focusable elements
- `Arrow Up/Down`: navigate table rows, list items
- `Enter`: activate focused element (open detail view, submit form)
- `Escape`: close modal, cancel editing, navigate back
- `Cmd+K`: command palette
- Surface-specific shortcuts documented in a `?` help overlay per surface

### Accessibility

- All interactive elements have ARIA labels
- Table rows use `role="row"` with `aria-selected` for focused state
- Status badges have `aria-label` describing the status (not just colour)
- Focus indicators visible on all interactive elements (2px solid `--ring`)
- Screen reader announcements for real-time updates (`aria-live="polite"` region)

---

## State Management

### Store Architecture

Each enterprise surface gets its own Zustand store, following the existing pattern (`create()` with Immer `produce()`):

```
client/src/features/broker/store/brokerStore.ts
client/src/features/workflows/store/workflowStore.ts
client/src/features/kpi/store/kpiStore.ts
client/src/features/connectors/store/connectorStore.ts
client/src/features/policy/store/policyStore.ts
```

Stores are independent. Cross-surface navigation (e.g., clicking a case reference in the KPI lineage view) uses router navigation, not store coupling.

### Store Shape (representative)

```typescript
// brokerStore.ts
interface BrokerState {
  inbox: {
    cases: BrokerCase[];
    totalCount: number;
    cursor: string | null;
    filters: InboxFilters;
    sort: { field: string; direction: 'asc' | 'desc' };
    loading: boolean;
    error: string | null;
  };
  activeCase: {
    case: BrokerCaseDetail | null;
    evidence: CaseEvidence | null;
    similarDecisions: BrokerDecision[];
    loading: boolean;
    error: string | null;
  };
  timeline: {
    decisions: BrokerDecision[];
    loading: boolean;
    cursor: string | null;
  };
  // Actions
  fetchInbox: () => Promise<void>;
  fetchCase: (id: string) => Promise<void>;
  claimCase: (id: string) => Promise<void>;
  releaseCase: (id: string) => Promise<void>;
  submitDecision: (id: string, action: DecisionAction) => Promise<void>;
  setFilters: (filters: Partial<InboxFilters>) => void;
  setSort: (field: string, direction: 'asc' | 'desc') => void;
}
```

### WebSocket Integration

Real-time updates are handled by extending the existing WebSocket infrastructure (`client/src/store/websocket/`):
- New subscription channels: `broker:inbox`, `broker:case:{id}`, `kpi:snapshots`, `connector:{id}:sync`
- Channel subscriptions are managed per-surface: subscribe on mount, unsubscribe on unmount
- Incoming messages dispatch to the appropriate Zustand store via a channel-to-store router

---

## API Integration

All API calls use the existing fetch-based pattern from `client/src/api/`. New API modules:

```
client/src/api/brokerApi.ts      -> /api/broker/*
client/src/api/workflowApi.ts    -> /api/workflows/*
client/src/api/kpiApi.ts         -> /api/mesh-metrics/*
client/src/api/connectorApi.ts   -> /api/connectors/*
client/src/api/policyApi.ts      -> /api/policy/*
```

All endpoints require Nostr NIP-98 authentication (existing `nostrAuth` service) or OIDC session (ADR-040, when implemented). Unauthorized responses (401) redirect to the login screen. Forbidden responses (403) show a role-insufficient error message.

---

## File Structure

New feature modules follow the existing feature directory convention:

```
client/src/features/
  broker/
    components/
      BrokerInbox.tsx
      DecisionCanvas.tsx
      DecisionActions.tsx
      BrokerTimeline.tsx
      EvidencePanel.tsx
      GraphContextViewer.tsx
      PriorityBadge.tsx
      CaseRow.tsx
    hooks/
      useBrokerInbox.ts
      useBrokerCase.ts
      useBrokerWebSocket.ts
    store/
      brokerStore.ts
    types/
      broker.ts
    index.ts
  workflows/
    components/
      ProposalList.tsx
      ProposalDetail.tsx
      StepEditor.tsx
      VersionDiff.tsx
      PromotionFlow.tsx
      PatternLibrary.tsx
    hooks/
      useWorkflowProposals.ts
      useWorkflowVersions.ts
    store/
      workflowStore.ts
    types/
      workflow.ts
    index.ts
  kpi/
    components/
      KpiDashboard.tsx
      KpiCard.tsx
      KpiDrilldown.tsx
      SparklineChart.tsx
      ConfidenceIndicator.tsx
      TimeWindowSelector.tsx
      DimensionSlicers.tsx
    hooks/
      useKpiMetrics.ts
      useKpiWebSocket.ts
    store/
      kpiStore.ts
    types/
      kpi.ts
    index.ts
  connectors/
    components/
      ConnectorList.tsx
      ConnectorWizard.tsx
      SignalFeed.tsx
      RedactionRuleEditor.tsx
      SyncStatusIndicator.tsx
    hooks/
      useConnectors.ts
      useConnectorSync.ts
    store/
      connectorStore.ts
    types/
      connector.ts
    index.ts
  policy/
    components/
      PolicyRuleList.tsx
      PolicyRuleEditor.tsx
      EvaluationLog.tsx
      PolicyTestBench.tsx
    hooks/
      usePolicyRules.ts
      usePolicyEvaluations.ts
    store/
      policyStore.ts
    types/
      policy.ts
    index.ts
```

### Design System Extensions

New components added to `client/src/features/design-system/components/`:

| Component | Purpose |
|-----------|---------|
| `DataTable.tsx` | Sortable, filterable, keyboard-navigable table (wraps Radix primitives) |
| `StatusIndicator.tsx` | Colour-coded dot + label for entity status |
| `Sparkline.tsx` | Inline SVG sparkline (TypeScript baseline, WASM-enhanced) |
| `StepIndicator.tsx` | Horizontal pipeline progress indicator |
| `FilterBar.tsx` | Collapsible filter controls container |
| `EmptyState.tsx` | Centered illustration + message + CTA for empty views |
| `KeyboardShortcutHint.tsx` | Inline keyboard shortcut badge (e.g., `Ctrl+Enter`) |
| `SidebarNav.tsx` | Left sidebar navigation container |

---

## Performance Requirements

| Metric | Target | Measurement |
|--------|--------|-------------|
| Broker Inbox load (50 cases) | < 2s | Time from route entry to first render with data |
| Decision Canvas load | < 3s | Time from route entry to all evidence panels populated |
| KPI Dashboard load | < 2s | Time from route entry to all 4 cards rendered with sparklines |
| Table sort/filter | < 100ms | Time from user action to re-render |
| WebSocket message to UI update | < 200ms | Time from WS message receipt to DOM update |
| Page transition | < 150ms | Time from route change to new view visible |
| WASM sparkline render (1000 points) | < 16ms | Single frame budget for sparkline re-render |
| Bundle size (enterprise surfaces) | < 200KB gzipped | Lazy-loaded chunk for all enterprise features |

Enterprise feature modules are code-split via React.lazy() at the route level. The 3D graph bundle (Three.js, R3F) is not loaded on enterprise routes unless the Decision Canvas is accessed (which embeds a mini-graph).

---

## Rollout Plan

### Phase 1: Navigation + Broker Workbench (4 weeks)
- Add react-router-dom
- Implement SidebarNav
- Implement Broker Inbox + Decision Canvas + Decision Actions
- WebSocket subscription for inbox updates
- Command palette integration for broker commands

### Phase 2: Workflow Studio + KPI Dashboard (3 weeks)
- Proposal List + Proposal Detail with step editor
- Version diff viewer
- KPI Dashboard with TypeScript sparklines
- Time window selector + dimension slicing

### Phase 3: Connectors + Policy (3 weeks)
- Connector list + configuration wizard
- Signal feed
- Policy rule list + editor
- Evaluation log + test bench

### Phase 4: WASM Enhancement + Polish (2 weeks)
- Compile kpi-sparklines, workflow-dag, broker-timeline WASM modules (ADR-047)
- Integrate WASM bridges with fallback detection
- Performance optimisation pass
- Keyboard navigation audit
- Accessibility audit

---

## Success Metrics

| Metric | Target | Measurement Method |
|--------|--------|--------------------|
| Broker decision throughput | 20+ decisions/day/broker | Count of BrokerDecisionMade events per broker per day |
| Mean time to decision | < 5 minutes | Median elapsed time from case claim to decision |
| Workflow proposals per month | 10+ | Count of WorkflowProposalSubmitted events |
| KPI dashboard weekly active users | 80% of enterprise users | Unique sessions on /kpi per week |
| Policy configuration changes | Admin self-service (no code deploy) | Count of PUT /api/policy/* calls |
| Connector sync success rate | > 99% | Successful syncs / total sync attempts |
| UI error rate | < 0.1% of page loads | Client error tracking |

---

## Open Questions

1. **Rich text vs markdown for broker reasoning**: The current design uses markdown in a Textarea. Should the reasoning field support a WYSIWYG editor for non-technical brokers?
2. **Offline support**: Should enterprise surfaces work offline with service worker caching? Probably not for initial release, but the store architecture should not preclude it.
3. **Notification preferences**: Brokers may want email/Slack notifications for new high-priority cases. This is a backend concern but the UI would need a preferences panel.
4. **Multi-language support**: Enterprise deployments in non-English organisations may need i18n. The current codebase has no i18n framework. Defer unless pilot feedback demands it.
5. **Audit export format**: CSV is specified for KPI export. Should the broker timeline and policy evaluation log also support CSV export for compliance teams?

---

## References

- ADR-040: Enterprise Identity Strategy
- ADR-041: Judgment Broker Workbench Architecture
- ADR-042: Workflow Proposal Object Model
- ADR-043: KPI Lineage Model
- ADR-044: Connector Governance and Privacy Boundaries
- ADR-045: Policy Engine Approach
- ADR-046: Enterprise UI Architecture (companion to this PRD)
- ADR-047: WASM Visualization Components (companion to this PRD)
- PRD-001: VisionFlow Data Pipeline Alignment
- `client/src/features/design-system/` (existing component library)
- `client/src/wasm/scene-effects-bridge.ts` (existing WASM bridge pattern)
- `client/src/features/command-palette/types.ts` (command registration interface)
