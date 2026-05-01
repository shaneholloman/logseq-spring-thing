/**
 * ADR-064: Typed Graph Schema -- client-side mirror of Rust enum values.
 *
 * Source of truth: crates/graph-cognition-core/src/{node_kind,edge_kind,edge_category}.rs
 *
 * All `repr(u8)` discriminant values match the Rust backend exactly.
 * Colours use Tailwind 400-weight tokens for light/dark theme compatibility.
 */

// ---------------------------------------------------------------------------
// NodeKind  (21 variants, 4 groups)
// ---------------------------------------------------------------------------

export enum NodeKind {
  // Code (5)
  Function          = 0,
  Module            = 1,
  Class             = 2,
  Interface         = 3,
  Variable          = 4,
  // Infrastructure (8)
  Service           = 10,
  Container         = 11,
  Database          = 12,
  Queue             = 13,
  Cache             = 14,
  Gateway           = 15,
  LoadBalancer      = 16,
  Cdn               = 17,
  // Domain (3)
  Entity            = 20,
  ValueObject       = 21,
  Aggregate         = 22,
  // Knowledge (5)
  Page              = 30,
  Block             = 31,
  Concept           = 32,
  OntologyClass     = 33,
  OntologyIndividual = 34,
}

// ---------------------------------------------------------------------------
// EdgeKind  (35 variants, 8 categories)
// ---------------------------------------------------------------------------

export enum EdgeKind {
  // Structural (5)
  Contains          = 0,
  InheritsFrom      = 1,
  Implements        = 2,
  ComposedOf        = 3,
  Nests             = 4,
  // Behavioral (4)
  Calls             = 10,
  Overrides         = 11,
  Triggers          = 12,
  Subscribes        = 13,
  // Data Flow (4)
  ReadsFrom         = 20,
  WritesTo          = 21,
  TransformsTo      = 22,
  Pipes             = 23,
  // Dependencies (4)
  DependsOn         = 30,
  Imports           = 31,
  Requires          = 32,
  Enables           = 33,
  // Semantic (5)
  SubClassOf        = 40,
  InstanceOf        = 41,
  EquivalentTo      = 42,
  DisjointWith      = 43,
  SameAs            = 44,
  // Infrastructure (4)
  DeploysTo         = 50,
  RoutesTo          = 51,
  ReplicatesTo      = 52,
  Monitors          = 53,
  // Domain (4)
  HasPart           = 60,
  BridgesTo         = 61,
  Fulfills          = 62,
  Constrains        = 63,
  // Knowledge (5)
  WikiLink          = 70,
  BlockRef          = 71,
  BlockParent       = 72,
  TaggedWith        = 73,
  CitedBy           = 74,
}

// ---------------------------------------------------------------------------
// EdgeCategory  (8 categories)
// ---------------------------------------------------------------------------

export enum EdgeCategory {
  Structural     = 'structural',
  Behavioral     = 'behavioral',
  DataFlow       = 'data_flow',
  Dependencies   = 'dependencies',
  Semantic       = 'semantic',
  Infrastructure = 'infrastructure',
  Domain         = 'domain',
  Knowledge      = 'knowledge',
}

// ---------------------------------------------------------------------------
// Display metadata types
// ---------------------------------------------------------------------------

export interface NodeKindMeta {
  label: string;
  group: string;
  color: string;
  icon: string;
}

export interface EdgeKindMeta {
  label: string;
  category: EdgeCategory;
  color: string;
}

export interface EdgeCategoryMeta {
  label: string;
  color: string;
  description: string;
}

// ---------------------------------------------------------------------------
// NODE_KIND_META  (21 entries)
// ---------------------------------------------------------------------------

export const NODE_KIND_META: Record<NodeKind, NodeKindMeta> = {
  // Code
  [NodeKind.Function]:           { label: 'Function',            group: 'Code',           color: '#60a5fa', icon: 'function' },
  [NodeKind.Module]:             { label: 'Module',              group: 'Code',           color: '#818cf8', icon: 'package' },
  [NodeKind.Class]:              { label: 'Class',               group: 'Code',           color: '#a78bfa', icon: 'box' },
  [NodeKind.Interface]:          { label: 'Interface',           group: 'Code',           color: '#c084fc', icon: 'layout' },
  [NodeKind.Variable]:           { label: 'Variable',            group: 'Code',           color: '#93c5fd', icon: 'variable' },
  // Infrastructure
  [NodeKind.Service]:            { label: 'Service',             group: 'Infrastructure', color: '#34d399', icon: 'server' },
  [NodeKind.Container]:          { label: 'Container',           group: 'Infrastructure', color: '#2dd4bf', icon: 'container' },
  [NodeKind.Database]:           { label: 'Database',            group: 'Infrastructure', color: '#4ade80', icon: 'database' },
  [NodeKind.Queue]:              { label: 'Queue',               group: 'Infrastructure', color: '#a3e635', icon: 'list-ordered' },
  [NodeKind.Cache]:              { label: 'Cache',               group: 'Infrastructure', color: '#facc15', icon: 'zap' },
  [NodeKind.Gateway]:            { label: 'Gateway',             group: 'Infrastructure', color: '#fbbf24', icon: 'shield' },
  [NodeKind.LoadBalancer]:       { label: 'Load Balancer',       group: 'Infrastructure', color: '#fb923c', icon: 'scale' },
  [NodeKind.Cdn]:                { label: 'CDN',                 group: 'Infrastructure', color: '#f97316', icon: 'globe' },
  // Domain
  [NodeKind.Entity]:             { label: 'Entity',              group: 'Domain',         color: '#f472b6', icon: 'diamond' },
  [NodeKind.ValueObject]:        { label: 'Value Object',        group: 'Domain',         color: '#fb7185', icon: 'tag' },
  [NodeKind.Aggregate]:          { label: 'Aggregate',           group: 'Domain',         color: '#e879f9', icon: 'layers' },
  // Knowledge
  [NodeKind.Page]:               { label: 'Page',                group: 'Knowledge',      color: '#38bdf8', icon: 'file-text' },
  [NodeKind.Block]:              { label: 'Block',               group: 'Knowledge',      color: '#7dd3fc', icon: 'square' },
  [NodeKind.Concept]:            { label: 'Concept',             group: 'Knowledge',      color: '#22d3ee', icon: 'lightbulb' },
  [NodeKind.OntologyClass]:      { label: 'Ontology Class',      group: 'Knowledge',      color: '#67e8f9', icon: 'git-branch' },
  [NodeKind.OntologyIndividual]: { label: 'Ontology Individual', group: 'Knowledge',      color: '#a5f3fc', icon: 'user' },
};

// ---------------------------------------------------------------------------
// EDGE_CATEGORY_META  (8 entries)
// ---------------------------------------------------------------------------

export const EDGE_CATEGORY_META: Record<EdgeCategory, EdgeCategoryMeta> = {
  [EdgeCategory.Structural]:     { label: 'Structural',     color: '#94a3b8', description: 'Containment, inheritance, and composition relationships' },
  [EdgeCategory.Behavioral]:     { label: 'Behavioral',     color: '#60a5fa', description: 'Function calls, overrides, and event subscriptions' },
  [EdgeCategory.DataFlow]:       { label: 'Data Flow',      color: '#34d399', description: 'Read, write, transform, and pipe operations' },
  [EdgeCategory.Dependencies]:   { label: 'Dependencies',   color: '#fbbf24', description: 'Import, require, and enablement relationships' },
  [EdgeCategory.Semantic]:       { label: 'Semantic',        color: '#c084fc', description: 'OWL/RDF class hierarchy and equivalence' },
  [EdgeCategory.Infrastructure]: { label: 'Infrastructure', color: '#fb923c', description: 'Deployment, routing, replication, and monitoring' },
  [EdgeCategory.Domain]:         { label: 'Domain',          color: '#f472b6', description: 'DDD aggregate parts, bridges, and constraints' },
  [EdgeCategory.Knowledge]:      { label: 'Knowledge',       color: '#22d3ee', description: 'Wiki links, block references, tags, and citations' },
};

// ---------------------------------------------------------------------------
// EDGE_KIND_META  (35 entries)
// ---------------------------------------------------------------------------

export const EDGE_KIND_META: Record<EdgeKind, EdgeKindMeta> = {
  // Structural
  [EdgeKind.Contains]:      { label: 'Contains',       category: EdgeCategory.Structural,     color: '#94a3b8' },
  [EdgeKind.InheritsFrom]:  { label: 'Inherits From',  category: EdgeCategory.Structural,     color: '#94a3b8' },
  [EdgeKind.Implements]:    { label: 'Implements',      category: EdgeCategory.Structural,     color: '#94a3b8' },
  [EdgeKind.ComposedOf]:    { label: 'Composed Of',     category: EdgeCategory.Structural,     color: '#94a3b8' },
  [EdgeKind.Nests]:         { label: 'Nests',           category: EdgeCategory.Structural,     color: '#94a3b8' },
  // Behavioral
  [EdgeKind.Calls]:         { label: 'Calls',           category: EdgeCategory.Behavioral,     color: '#60a5fa' },
  [EdgeKind.Overrides]:     { label: 'Overrides',       category: EdgeCategory.Behavioral,     color: '#60a5fa' },
  [EdgeKind.Triggers]:      { label: 'Triggers',        category: EdgeCategory.Behavioral,     color: '#60a5fa' },
  [EdgeKind.Subscribes]:    { label: 'Subscribes',      category: EdgeCategory.Behavioral,     color: '#60a5fa' },
  // Data Flow
  [EdgeKind.ReadsFrom]:     { label: 'Reads From',      category: EdgeCategory.DataFlow,       color: '#34d399' },
  [EdgeKind.WritesTo]:      { label: 'Writes To',       category: EdgeCategory.DataFlow,       color: '#34d399' },
  [EdgeKind.TransformsTo]:  { label: 'Transforms To',   category: EdgeCategory.DataFlow,       color: '#34d399' },
  [EdgeKind.Pipes]:         { label: 'Pipes',            category: EdgeCategory.DataFlow,       color: '#34d399' },
  // Dependencies
  [EdgeKind.DependsOn]:     { label: 'Depends On',      category: EdgeCategory.Dependencies,   color: '#fbbf24' },
  [EdgeKind.Imports]:       { label: 'Imports',          category: EdgeCategory.Dependencies,   color: '#fbbf24' },
  [EdgeKind.Requires]:      { label: 'Requires',         category: EdgeCategory.Dependencies,   color: '#fbbf24' },
  [EdgeKind.Enables]:       { label: 'Enables',          category: EdgeCategory.Dependencies,   color: '#fbbf24' },
  // Semantic
  [EdgeKind.SubClassOf]:    { label: 'Subclass Of',     category: EdgeCategory.Semantic,       color: '#c084fc' },
  [EdgeKind.InstanceOf]:    { label: 'Instance Of',     category: EdgeCategory.Semantic,       color: '#c084fc' },
  [EdgeKind.EquivalentTo]:  { label: 'Equivalent To',   category: EdgeCategory.Semantic,       color: '#c084fc' },
  [EdgeKind.DisjointWith]:  { label: 'Disjoint With',   category: EdgeCategory.Semantic,       color: '#c084fc' },
  [EdgeKind.SameAs]:        { label: 'Same As',          category: EdgeCategory.Semantic,       color: '#c084fc' },
  // Infrastructure
  [EdgeKind.DeploysTo]:     { label: 'Deploys To',      category: EdgeCategory.Infrastructure, color: '#fb923c' },
  [EdgeKind.RoutesTo]:      { label: 'Routes To',       category: EdgeCategory.Infrastructure, color: '#fb923c' },
  [EdgeKind.ReplicatesTo]:  { label: 'Replicates To',   category: EdgeCategory.Infrastructure, color: '#fb923c' },
  [EdgeKind.Monitors]:      { label: 'Monitors',         category: EdgeCategory.Infrastructure, color: '#fb923c' },
  // Domain
  [EdgeKind.HasPart]:       { label: 'Has Part',        category: EdgeCategory.Domain,         color: '#f472b6' },
  [EdgeKind.BridgesTo]:     { label: 'Bridges To',      category: EdgeCategory.Domain,         color: '#f472b6' },
  [EdgeKind.Fulfills]:      { label: 'Fulfills',         category: EdgeCategory.Domain,         color: '#f472b6' },
  [EdgeKind.Constrains]:    { label: 'Constrains',       category: EdgeCategory.Domain,         color: '#f472b6' },
  // Knowledge
  [EdgeKind.WikiLink]:      { label: 'Wiki Link',       category: EdgeCategory.Knowledge,      color: '#22d3ee' },
  [EdgeKind.BlockRef]:      { label: 'Block Ref',       category: EdgeCategory.Knowledge,      color: '#22d3ee' },
  [EdgeKind.BlockParent]:   { label: 'Block Parent',    category: EdgeCategory.Knowledge,      color: '#22d3ee' },
  [EdgeKind.TaggedWith]:    { label: 'Tagged With',     category: EdgeCategory.Knowledge,      color: '#22d3ee' },
  [EdgeKind.CitedBy]:       { label: 'Cited By',        category: EdgeCategory.Knowledge,      color: '#22d3ee' },
};

// ---------------------------------------------------------------------------
// Lookup helpers
// ---------------------------------------------------------------------------

/** Reverse map from u8 discriminant to NodeKind. */
const _nodeKindById = new Map<number, NodeKind>(
  (Object.values(NodeKind).filter((v): v is number => typeof v === 'number') as NodeKind[])
    .map((k) => [k as number, k]),
);

/** Reverse map from u8 discriminant to EdgeKind. */
const _edgeKindById = new Map<number, EdgeKind>(
  (Object.values(EdgeKind).filter((v): v is number => typeof v === 'number') as EdgeKind[])
    .map((k) => [k as number, k]),
);

/**
 * Convert a `kind_id` u8 from the backend into the corresponding NodeKind.
 * Returns `undefined` for unrecognised values (forward-compatibility).
 */
export function nodeKindFromId(id: number): NodeKind | undefined {
  return _nodeKindById.get(id);
}

/**
 * Return the EdgeCategory for a given EdgeKind.
 */
export function edgeCategoryForKind(kind: EdgeKind): EdgeCategory {
  return EDGE_KIND_META[kind].category;
}

/**
 * Return all NodeKinds that belong to a display group name
 * (e.g. "Code", "Infrastructure", "Domain", "Knowledge").
 */
export function nodeKindsInGroup(group: string): NodeKind[] {
  return (Object.entries(NODE_KIND_META) as [string, NodeKindMeta][])
    .filter(([, meta]) => meta.group === group)
    .map(([key]) => Number(key) as NodeKind);
}

/**
 * Return all EdgeKinds that belong to a given EdgeCategory.
 */
export function edgeKindsInCategory(category: EdgeCategory): EdgeKind[] {
  return (Object.entries(EDGE_KIND_META) as [string, EdgeKindMeta][])
    .filter(([, meta]) => meta.category === category)
    .map(([key]) => Number(key) as EdgeKind);
}
