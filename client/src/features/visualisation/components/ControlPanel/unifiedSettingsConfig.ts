/**
 * Unified Settings Configuration
 *
 * Reorganized tab structure with proper basic/advanced mode gating.
 * All settings are categorized by feature area with isAdvanced flags.
 */

import type { SectionConfig } from './types';
import {
  Eye, Sparkles, BarChart3, Gauge, Settings,
  Bot, Code, Network, Shield, Database
} from 'lucide-react';
// @ts-ignore - These icons exist in lucide-react but types may be outdated
import { Atom, Glasses } from 'lucide-react';

export interface UnifiedTabConfig {
  id: string;
  label: string;
  icon: typeof Eye;
  description: string;
  buttonKey?: string;
  isAdvanced?: boolean;
  isPowerUserOnly?: boolean;
}

// ============================================================================
// TAB DEFINITIONS - New unified structure
// ============================================================================

export const UNIFIED_TABS: UnifiedTabConfig[] = [
  // Basic Tabs (always visible)
  {
    id: 'graph',
    label: 'Graph',
    icon: Network,
    description: 'Node, edge, and label visualization settings',
    buttonKey: '1'
  },
  {
    id: 'physics',
    label: 'Physics',
    icon: Atom,
    description: 'Simulation and force-directed layout controls',
    buttonKey: '2'
  },
  {
    id: 'effects',
    label: 'Effects',
    icon: Sparkles,
    description: 'Visual effects, glow, and animations',
    buttonKey: '3'
  },
  {
    id: 'analytics',
    label: 'Analytics',
    icon: BarChart3,
    description: 'Metrics, filtering, and clustering',
    buttonKey: '4'
  },
  {
    id: 'quality',
    label: 'Quality',
    icon: Gauge,
    description: 'GPU, performance, and quality gates',
    buttonKey: '5'
  },
  {
    id: 'system',
    label: 'System',
    icon: Settings,
    description: 'Network, authentication, and system settings',
    buttonKey: '6'
  },
  // Advanced Tabs
  {
    id: 'xr',
    label: 'XR',
    icon: Glasses,
    description: 'VR/AR immersive settings',
    buttonKey: '7',
    isAdvanced: true
  },
  {
    id: 'ai',
    label: 'AI',
    icon: Bot,
    description: 'RAGFlow, Perplexity, and AI integrations',
    buttonKey: '8',
    isAdvanced: true,
    isPowerUserOnly: true
  },
  {
    id: 'developer',
    label: 'Dev',
    icon: Code,
    description: 'Debug tools and developer options',
    buttonKey: '9',
    isAdvanced: true,
    isPowerUserOnly: true
  },
  {
    id: 'solid',
    label: 'Pod',
    icon: Database,
    description: 'Solid Pod file browser and settings',
    buttonKey: '0'
  }
];

// ============================================================================
// SETTINGS DEFINITIONS - With proper isAdvanced flags
// ============================================================================

export const UNIFIED_SETTINGS_CONFIG: Record<string, SectionConfig> = {
  // -------------------------------------------------------------------------
  // GRAPH TAB - Basic visualization
  // -------------------------------------------------------------------------
  graph: {
    title: 'Graph Visualization',
    fields: [
      // Nodes - Basic
      { key: 'nodeColor', label: 'Node Color', type: 'color', path: 'visualisation.graphs.logseq.nodes.baseColor', description: 'Base color for nodes (used when colour scheme is "base")' },
      { key: 'colorScheme', label: 'Node colour by', type: 'select', options: ['type', 'domain', 'base', 'community', 'cluster', 'centrality', 'sssp'], path: 'visualisation.graphs.logseq.nodes.colorScheme', description: 'How nodes are coloured: "type"/"domain"/"base" are semantic; "community" by Louvain partition, "cluster" by DBSCAN cluster, "centrality" by PageRank (blue→red ramp), "sssp" by graph distance. Analytic modes fall through to "type" for nodes the server left without that signal.' },
      { key: 'sizeScheme', label: 'Node size by', type: 'select', options: ['degree', 'fileSize', 'hybrid'], path: 'visualisation.graphs.logseq.nodes.sizeScheme', description: 'How nodes are sized: "degree" by connection count, "fileSize" by content byte-size, "hybrid" combines both' },
      { key: 'nodeSize', label: 'Node Size', type: 'slider', min: 0.1, max: 1, step: 0.05, path: 'visualisation.graphs.logseq.nodes.nodeSize', description: 'Global size gain (per-node magnitude comes from degree + content size)' },
      { key: 'perNodeGlow', label: 'Per-node glow (authority/degree)', type: 'toggle', path: 'visualisation.graphs.logseq.nodes.perNodeGlow', description: 'When on, per-node emissive (from the metadata texture) drives glow; when off, nodes use a uniform glow' },
      // Nodes - Advanced
      { key: 'enableMetadataShape', label: 'Metadata Shape', type: 'toggle', path: 'visualisation.graphs.logseq.nodes.enableMetadataShape', description: 'Shape based on metadata', isAdvanced: true },

      // Node Type Visibility
      { key: 'showKnowledge', label: 'Knowledge Nodes', type: 'toggle', path: 'visualisation.graphs.logseq.nodes.nodeTypeVisibility.knowledge', description: 'Show knowledge graph nodes' },
      { key: 'showOntology', label: 'Ontology Nodes', type: 'toggle', path: 'visualisation.graphs.logseq.nodes.nodeTypeVisibility.ontology', description: 'Show ontology nodes' },
      { key: 'showAgents', label: 'Agent Nodes', type: 'toggle', path: 'visualisation.graphs.logseq.nodes.nodeTypeVisibility.agent', description: 'Show agent nodes' },

      // Edges - Basic
      { key: 'edgeColor', label: 'Edge Color', type: 'color', path: 'visualisation.graphs.logseq.edges.color', description: 'Base color for edges' },
      { key: 'edgeWidth', label: 'Edge Thickness', type: 'slider', min: 0.02, max: 0.5, step: 0.01, path: 'visualisation.graphs.logseq.edges.baseWidth', description: 'Cylinder radius of edges (1:1 — the slider value is the tube radius)' },
      { key: 'edgeOpacity', label: 'Edge Opacity', type: 'slider', min: 0, max: 0.3, step: 0.005, path: 'visualisation.graphs.logseq.edges.opacity', description: 'Per-edge alpha. Dense graphs overlap many edges, so values above ~0.2 read as solid — the useful range lives at the bottom.' },
      { key: 'colorByType', label: 'Colour edges by relationship type', type: 'toggle', path: 'visualisation.graphs.logseq.edges.colorByType', description: 'Colour each edge by its relationship type (11 edge types) instead of the single base colour above' },
      { key: 'widthByWeight', label: 'Edge width by weight', type: 'toggle', path: 'visualisation.graphs.logseq.edges.widthByWeight', description: 'Scale edge width by edge weight instead of using a uniform base width' },

      // Knowledge Graph Mode - Basic
      { key: 'kgEdgeColor', label: 'KG Edge Color', type: 'color', path: 'visualisation.graphTypeVisuals.knowledgeGraph.edgeColor', description: 'Edge color for knowledge graph mode' },
      { key: 'ontologyEdgeColor', label: 'Ontology Edge Color', type: 'color', path: 'visualisation.graphTypeVisuals.ontology.edgeColor', description: 'Edge color for ontology mode', isAdvanced: true },
      { key: 'ringTintByClass', label: 'Tint ontology rings by class', type: 'toggle', path: 'visualisation.graphTypeVisuals.ontology.ringTintByClass', description: 'Tint each ontology node\'s orbital rings by its class instead of a uniform ring colour' },

      // Labels - Basic
      { key: 'enableLabels', label: 'Show Labels', type: 'toggle', path: 'visualisation.graphs.logseq.labels.enableLabels', description: 'Display node labels' },
      { key: 'labelSize', label: 'Label Size', type: 'slider', min: 0.05, max: 3.0, step: 0.05, path: 'visualisation.graphs.logseq.labels.desktopFontSize', description: 'Font size for labels' },
      { key: 'labelColor', label: 'Label Color', type: 'color', path: 'visualisation.graphs.logseq.labels.textColor', description: 'Color of label text' },
      { key: 'showMetadata', label: 'Show Metadata', type: 'toggle', path: 'visualisation.graphs.logseq.labels.showMetadata', description: 'Show domain, links, and quality info under labels' },
      { key: 'labelStandoff', label: 'Label Standoff', type: 'slider', min: -1.0, max: 3.0, step: 0.05, path: 'visualisation.graphs.logseq.labels.textPadding', description: 'Gap between node surface and label' },
      // Labels - Advanced
      { key: 'labelOutlineColor', label: 'Outline Color', type: 'color', path: 'visualisation.graphs.logseq.labels.textOutlineColor', description: 'Label outline color', isAdvanced: true },
      { key: 'labelOutlineWidth', label: 'Outline Width', type: 'slider', min: 0, max: 0.01, step: 0.001, path: 'visualisation.graphs.logseq.labels.textOutlineWidth', description: 'Label outline width', isAdvanced: true },
      { key: 'labelDistanceThreshold', label: 'Label Draw Distance', type: 'slider', min: 0, max: 2000, step: 25, path: 'visualisation.graphs.logseq.labels.labelDistanceThreshold', description: 'Max camera distance for label visibility' },
      { key: 'maxLabelWidth', label: 'Max Label Width', type: 'slider', min: 2, max: 20, step: 0.5, path: 'visualisation.graphs.logseq.labels.maxLabelWidth', description: 'Maximum text wrapping width', isAdvanced: true },

      // Rendering - Basic
      { key: 'ambientLight', label: 'Ambient Light', type: 'slider', min: 0, max: 2, step: 0.1, path: 'visualisation.rendering.ambientLightIntensity', description: 'Overall scene brightness' },
      { key: 'directionalLight', label: 'Direct Light', type: 'slider', min: 0, max: 2, step: 0.1, path: 'visualisation.rendering.directionalLightIntensity', description: 'Directional light intensity' },
      // Rendering - Advanced (Phase 6 ADR-04 renderer-architectural controls)
      { key: 'maxEdgesCeiling', label: 'Max Edges Ceiling', type: 'slider', min: 1024, max: 262144, step: 1024, path: 'visualisation.rendering.maxEdgesCeiling', description: 'Hard cap on dynamically-grown edge instance capacity (Phase 6)', isAdvanced: true },
      { key: 'softwareFallback', label: 'Software WebGL Fallback', type: 'select', options: ['auto', 'force-on', 'force-off'], path: 'visualisation.rendering.softwareFallback', description: 'Behaviour on software-rendered WebGL contexts (SwiftShader/llvmpipe)', isAdvanced: true },
      { key: 'labelLayoutEvery', label: 'Label Layout Cadence (frames)', type: 'slider', min: 1, max: 10, step: 1, path: 'visualisation.rendering.labelLayoutEvery', description: 'Frames between full label re-layout passes', isAdvanced: true },

      // Selection Highlighting - Basic
      { key: 'selectionHighlightColor', label: 'Selection Color', type: 'color', path: 'visualisation.interaction.selectionHighlightColor', description: 'Edge color when node is selected' },

      // Analytics Overlays (ADR-031 D6) — server-computed structure on top of the
      // base graph. Cluster hulls wrap each group's nodes in a translucent volume;
      // anomaly highlighting recolours outlier nodes red.
      { key: 'clusterHulls', label: 'Cluster Hulls', type: 'toggle', path: 'visualisation.clusterHulls.enabled', description: 'Draw a translucent convex hull around each server-provided cluster (DBSCAN) or community (Louvain) group' },
      { key: 'clusterHullOpacity', label: 'Hull Opacity', type: 'slider', min: 0, max: 0.5, step: 0.01, path: 'visualisation.clusterHulls.opacity', description: 'Translucency of cluster hull volumes' },
      { key: 'clusterHullMax', label: 'Max Hulls', type: 'slider', min: 1, max: 64, step: 1, path: 'visualisation.clusterHulls.maxHulls', description: 'Cap on the number of hulls drawn — the N largest groups are kept so dense graphs stay legible' },
      { key: 'showAnomalies', label: 'Highlight Anomalies', type: 'toggle', path: 'qualityGates.showAnomalies', description: 'Recolour outlier nodes (LOF anomaly score) red regardless of the colour scheme' },
      { key: 'clusterHullCommunityFallback', label: 'Community Hull Fallback', type: 'toggle', path: 'visualisation.clusterHulls.communityFallback', description: 'When the server provides no DBSCAN clusters, draw hulls around Louvain communities. Off by default — communities optimise modularity not spatial locality, so their hulls overlap; the cleaner community signal is "Node colour by → community".', isAdvanced: true },
      { key: 'clusterHullSpatialFallback', label: 'Spatial Hull Fallback', type: 'toggle', path: 'visualisation.clusterHulls.spatialFallback', description: 'When the server provides no cluster or community structure, fabricate hulls from spatial proximity instead of showing none', isAdvanced: true }
    ]
  },

  // -------------------------------------------------------------------------
  // PHYSICS TAB - Simulation controls
  // -------------------------------------------------------------------------
  physics: {
    title: 'Physics Simulation',
    fields: [
      // ===================================================================
      // Core Forces — the dominant spring/repulsion/gravity terms.
      // Values written raw to the backend (no client-side scaling).
      // ===================================================================
      { key: 'springK', group: 'Core Forces', label: 'Spring Strength', type: 'slider', min: 0, max: 100, step: 0.5, path: 'visualisation.graphs.logseq.physics.springK', description: 'Edge spring constant for Hooke mode (default 15). In the default LinLog mode the per-population multipliers below govern spring strength.' },
      { key: 'springKKnowledge', group: 'Core Forces', label: 'Spring: Knowledge', type: 'slider', min: 0, max: 10, step: 0.1, path: 'visualisation.graphs.logseq.physics.springKKnowledge', description: 'Spring strength multiplier for knowledge-graph nodes — live in both LinLog and Hooke modes (default 1.0 = baseline).' },
      { key: 'springKOntology', group: 'Core Forces', label: 'Spring: Ontology', type: 'slider', min: 0, max: 10, step: 0.1, path: 'visualisation.graphs.logseq.physics.springKOntology', description: 'Spring strength multiplier for ontology (OWL) nodes (default 1.0 = baseline).' },
      { key: 'springKAgent', group: 'Core Forces', label: 'Spring: Agent', type: 'slider', min: 0, max: 10, step: 0.1, path: 'visualisation.graphs.logseq.physics.springKAgent', description: 'Spring strength multiplier for agent nodes (default 1.0 = baseline).' },
      { key: 'repelK', group: 'Core Forces', label: 'Repulsion', type: 'slider', min: 0, max: 3000, step: 10, path: 'visualisation.graphs.logseq.physics.repelK', description: 'Node repulsion constant (default 1200)' },
      { key: 'restLength', group: 'Core Forces', label: 'Node Spacing', type: 'slider', min: 1, max: 200, step: 1, path: 'visualisation.graphs.logseq.physics.restLength', description: 'Spring rest length — small = dense, large = spread (default 80)' },
      { key: 'centerGravityK', group: 'Core Forces', label: 'Cluster Tightness', type: 'slider', min: 0, max: 1.0, step: 0.01, path: 'visualisation.graphs.logseq.physics.centerGravityK', description: 'Pull towards center — higher values tightly cluster the graph (default 0.05)' },
      { key: 'gravity', group: 'Core Forces', label: 'Gravity', type: 'slider', min: 0, max: 0.01, step: 0.0001, path: 'visualisation.graphs.logseq.physics.gravity', description: 'Center-pull force — affects how loosely-connected nodes drift (default 0.0001)' },
      { key: 'maxForce', group: 'Core Forces', label: 'Max Force', type: 'slider', min: 1, max: 2000, step: 5, path: 'visualisation.graphs.logseq.physics.maxForce', description: 'Maximum force per node (default 1000)' },
      { key: 'maxVelocity', group: 'Core Forces', label: 'Max Velocity', type: 'slider', min: 1, max: 500, step: 1, path: 'visualisation.graphs.logseq.physics.maxVelocity', description: 'Maximum node speed (default 100)' },

      // ===================================================================
      // Simulation — integration cadence and convergence behaviour.
      // ===================================================================
      { key: 'enabled', group: 'Simulation', label: 'Physics Enabled', type: 'toggle', path: 'visualisation.graphs.logseq.physics.enabled', description: 'Enable physics simulation' },
      { key: 'resetLayout', group: 'Simulation', label: 'Reset Layout', type: 'action-button', action: 'reset_layout', description: 'Re-randomize all positions and reset physics to safe defaults — use when the graph has exploded or become unresponsive' },
      { key: 'autoBalance', group: 'Simulation', label: 'Auto Balance', type: 'toggle', path: 'visualisation.graphs.logseq.physics.autoBalance', description: 'Adaptive force balancing' },
      { key: 'dt', group: 'Simulation', label: 'Time Step', type: 'slider', min: 0.001, max: 0.1, step: 0.001, path: 'visualisation.graphs.logseq.physics.dt', description: 'Simulation time step (default 0.016)' },
      { key: 'iterations', group: 'Simulation', label: 'Iterations', type: 'slider', min: 0, max: 2000, step: 10, path: 'visualisation.graphs.logseq.physics.iterations', description: 'Solver iterations per frame — more = finer resolution (default 50)' },
      { key: 'warmupIterations', group: 'Simulation', label: 'Warmup Iterations', type: 'slider', min: 0, max: 500, step: 10, path: 'visualisation.graphs.logseq.physics.warmupIterations', description: 'Initial stabilization iterations (default 100)' },
      { key: 'coolingRate', group: 'Simulation', label: 'Cooling Rate', type: 'slider', min: 0, max: 0.01, step: 0.0005, path: 'visualisation.graphs.logseq.physics.coolingRate', description: 'Simulated annealing rate (default 0.001)' },
      { key: 'globalSpeed', group: 'Simulation', label: 'Global Speed', type: 'slider', min: 0, max: 5, step: 0.01, path: 'visualisation.graphs.logseq.physics.globalSpeed', description: 'FA2 base integration speed (default 0.5)' },
      { key: 'damping', group: 'Simulation', label: 'Damping', type: 'slider', min: 0.01, max: 1.0, step: 0.01, path: 'visualisation.graphs.logseq.physics.damping', description: 'Velocity damping — lower = more energy, higher = faster settle (default 0.85)' },

      // ===================================================================
      // Repulsion & Spacing — short-range separation and grid resolution.
      // ===================================================================
      { key: 'maxRepulsionDist', group: 'Repulsion & Spacing', label: 'Max Repulsion Dist', type: 'slider', min: 10, max: 800, step: 10, path: 'visualisation.graphs.logseq.physics.maxRepulsionDist', description: 'Maximum repulsion range — larger affects more distant nodes (default 400, sized to the ~400-unit graph envelope)' },
      { key: 'separationRadius', group: 'Repulsion & Spacing', label: 'Separation Radius', type: 'slider', min: 0, max: 50, step: 0.1, path: 'visualisation.graphs.logseq.physics.separationRadius', description: 'Minimum node separation — tiny for dense, large for spacing (default ~2.12)' },
      { key: 'gridCellSize', group: 'Repulsion & Spacing', label: 'Grid Cell Size', type: 'slider', min: 1, max: 200, step: 1, path: 'visualisation.graphs.logseq.physics.gridCellSize', description: 'Spatial grid cell size — larger for spread-out graphs (default 50)' },
      { key: 'repulsionSofteningEpsilon', group: 'Repulsion & Spacing', label: 'Repulsion Epsilon', type: 'slider', min: 0, max: 0.01, step: 0.0001, path: 'visualisation.graphs.logseq.physics.repulsionSofteningEpsilon', description: 'Softening for close nodes (default 0.0001)' },

      // ===================================================================
      // Bounds — bounding box containment.
      // ===================================================================
      { key: 'enableBounds', group: 'Bounds', label: 'Enable Bounds', type: 'toggle', path: 'visualisation.graphs.logseq.physics.enableBounds', description: 'Constrain nodes to a bounding box' },
      { key: 'boundsSize', group: 'Bounds', label: 'Bounds Size', type: 'slider', min: 100, max: 2000, step: 50, path: 'visualisation.graphs.logseq.physics.boundsSize', description: 'Half-extent of the soft bounding cube per axis — the graph settles within ~this radius (default 400)' },
      { key: 'boundaryDamping', group: 'Bounds', label: 'Boundary Damping', type: 'slider', min: 0, max: 1.0, step: 0.01, path: 'visualisation.graphs.logseq.physics.boundaryDamping', description: 'Velocity damping when nodes approach boundary (default 0.95)' },

      // ===================================================================
      // Layout Forces (FA2 / dual-graph) — ForceAtlas2 and disc layout.
      // ===================================================================
      { key: 'linLogMode', group: 'Layout Forces', label: 'LinLog Mode', type: 'toggle', path: 'visualisation.graphs.logseq.physics.linLogMode', description: 'Logarithmic attraction (modularity-preserving) vs linear Hooke springs' },
      { key: 'scalingRatio', group: 'Layout Forces', label: 'FA2 Scaling Ratio', type: 'slider', min: 0.5, max: 100, step: 0.5, path: 'visualisation.graphs.logseq.physics.scalingRatio', description: 'ForceAtlas2 repulsion scaling — higher spreads degree-heavy nodes further (default 10)' },
      { key: 'adaptiveSpeed', group: 'Layout Forces', label: 'Adaptive Speed', type: 'toggle', path: 'visualisation.graphs.logseq.physics.adaptiveSpeed', description: 'Per-node adaptive convergence speed (reduces oscillation)' },
      { key: 'ssspAlpha', group: 'Layout Forces', label: 'SSSP Alpha', type: 'slider', min: 0, max: 5, step: 0.1, path: 'visualisation.graphs.logseq.physics.ssspAlpha', description: 'Single-source shortest-path force weighting (default 1.5)' },
      { key: 'graphSeparationX', group: 'Layout Forces', label: 'Graph Separation', type: 'slider', min: 0, max: 400, step: 25, path: 'visualisation.graphs.logseq.physics.graphSeparationX', description: 'Separation between the knowledge and ontology graphs — the depth gap between the two facing discs (0 = merged/overlapping, ~250 = clearly separated, default 250). Use with Disc Flatten to make them face one another.' },
      { key: 'axisCompressionZ', group: 'Layout Forces', label: 'Disc Flatten', type: 'slider', min: 0, max: 1.0, step: 0.05, path: 'visualisation.graphs.logseq.physics.axisCompressionZ', description: 'Flatten KG + ontology into two discs that face one another across the gap (0 = full 3D blobs, 1 = flat facing discs, default 0.9). Agents stay 3D as bridges.' },

      // ===================================================================
      // Constraints — ontology constraint ramp and clustering coefficients.
      // ===================================================================
      { key: 'constraintRampFrames', group: 'Constraints', label: 'Constraint Ramp', type: 'slider', min: 0, max: 300, step: 5, path: 'visualisation.graphs.logseq.physics.constraintRampFrames', description: 'Frames over which ontology constraints ramp up after a change (default 60)' },
      { key: 'constraintMaxForcePerNode', group: 'Constraints', label: 'Constraint Max Force', type: 'slider', min: 1, max: 2000, step: 5, path: 'visualisation.graphs.logseq.physics.constraintMaxForcePerNode', description: 'Per-node cap on ontology constraint forces (default 50)' },
      { key: 'clusterStrength', group: 'Constraints', label: 'Cluster Strength', type: 'slider', min: 0, max: 0.02, step: 0.0005, path: 'visualisation.graphs.logseq.physics.clusterStrength', description: 'Raw cluster cohesion coefficient (default 0.002)' },
      { key: 'temperature', group: 'Constraints', label: 'Temperature', type: 'slider', min: 0, max: 5, step: 0.05, path: 'visualisation.graphs.logseq.physics.temperature', description: 'Simulation temperature (energy) — higher = more movement (default 1.0)' },

      // ===================================================================
      // Semantic & Layout Forces — routed to the quality-gates / semantic
      // endpoints (NOT the physics endpoint), but conceptually physics.
      // ===================================================================
      { key: 'layoutMode', group: 'Semantic & Layout Forces', label: 'Layout Mode', type: 'select', options: ['force-directed', 'dag-topdown', 'dag-radial', 'dag-leftright', 'type-clustering'], path: 'qualityGates.layoutMode', description: 'Graph layout algorithm — force-directed uses spring/repulsion, DAG modes add hierarchical layout, type-clustering groups by node type' },
      { key: 'ontologyPhysics', group: 'Semantic & Layout Forces', label: 'Ontology Forces', type: 'toggle', path: 'qualityGates.ontologyPhysics', description: 'Enable OWL ontology-derived constraint forces in the physics simulation' },
      { key: 'ontologyStrength', group: 'Semantic & Layout Forces', label: 'Ontology Strength', type: 'slider', min: 0, max: 1, step: 0.05, path: 'qualityGates.ontologyStrength', description: 'Global strength of ontology constraint forces (lower = gentler, higher = stricter)', isAdvanced: true },
      { key: 'semanticForces', group: 'Semantic & Layout Forces', label: 'Semantic Layout Forces', type: 'toggle', path: 'qualityGates.semanticForces', description: 'Enable DAG hierarchy layout and type-based clustering forces' },
      { key: 'dagLevelAttraction', group: 'Semantic & Layout Forces', label: 'DAG Level Attraction', type: 'slider', min: 0, max: 2.0, step: 0.05, path: 'qualityGates.dagLevelAttraction', description: 'How strongly nodes pull toward their hierarchy level', isAdvanced: true },
      { key: 'dagSiblingRepulsion', group: 'Semantic & Layout Forces', label: 'DAG Sibling Repulsion', type: 'slider', min: 0, max: 2.0, step: 0.05, path: 'qualityGates.dagSiblingRepulsion', description: 'How strongly same-level nodes spread apart', isAdvanced: true },
      { key: 'typeClusterAttraction', group: 'Semantic & Layout Forces', label: 'Type Cluster Attraction', type: 'slider', min: 0, max: 2.0, step: 0.05, path: 'qualityGates.typeClusterAttraction', description: 'How strongly same-type nodes group together', isAdvanced: true },
      { key: 'typeClusterRadius', group: 'Semantic & Layout Forces', label: 'Type Cluster Radius', type: 'slider', min: 10, max: 500, step: 10, path: 'qualityGates.typeClusterRadius', description: 'Target radius for type-based cluster zones', isAdvanced: true },

      // ===================================================================
      // Smooth Movement — client-side tweening / interpolation (local only).
      // ===================================================================
      { key: 'tweeningEnabled', group: 'Smooth Movement', label: 'Smooth Node Movement', type: 'toggle', path: 'visualisation.graphs.logseq.tweening.enabled', description: 'Smoothly animate nodes toward server positions instead of snapping instantly' },
      { key: 'tweeningLerpBase', group: 'Smooth Movement', label: 'Node Animation Speed', type: 'slider', min: 0.0001, max: 0.15, step: 0.001, path: 'visualisation.graphs.logseq.tweening.lerpBase', description: 'How quickly nodes reach their target positions (lower = faster, higher = smoother)' },
      { key: 'tweeningMaxDivergence', group: 'Smooth Movement', label: 'Maximum Node Jump', type: 'slider', min: 1, max: 100, step: 1, path: 'visualisation.graphs.logseq.tweening.maxDivergence', description: 'Distance threshold above which nodes snap instantly instead of animating' },
      { key: 'tweeningSnapThreshold', group: 'Smooth Movement', label: 'Snap Distance', type: 'slider', min: 0.01, max: 1.0, step: 0.01, path: 'visualisation.graphs.logseq.tweening.snapThreshold', description: 'Distance below which nodes snap to their target (sub-pixel precision)', isAdvanced: true }
    ]
  },

  // -------------------------------------------------------------------------
  // EFFECTS TAB - Visual effects
  // -------------------------------------------------------------------------
  effects: {
    title: 'Visual Effects',
    fields: [
      // Renderer toggle
      { key: 'webgpuRenderer', label: 'WebGPU Renderer', type: 'action-button', action: 'toggle-webgpu', description: 'Switch between WebGPU (TSL materials) and WebGL renderer. Page reloads on change.' },
      { key: 'rendererInfo', label: 'Renderer Info', type: 'readonly', path: 'rendererCapabilities', description: 'Active renderer backend and GPU info' },

      // Scene Effects (WASM) - Basic
      { key: 'sceneEffectsEnabled', label: 'Scene Effects', type: 'toggle', path: 'visualisation.sceneEffects.enabled', description: 'Enable WASM ambient effects' },
      { key: 'particleCount', label: 'Particle Count', type: 'slider', min: 64, max: 512, step: 32, path: 'visualisation.sceneEffects.particleCount', description: 'Number of ambient dust particles' },
      { key: 'particleOpacity', label: 'Particle Opacity', type: 'slider', min: 0, max: 1, step: 0.05, path: 'visualisation.sceneEffects.particleOpacity', description: 'Brightness of ambient particles' },
      { key: 'particleDrift', label: 'Particle Drift', type: 'slider', min: 0, max: 2, step: 0.1, path: 'visualisation.sceneEffects.particleDrift', description: 'Drift speed of particles', isAdvanced: true },

      // Energy Wisps (WASM) - Basic
      { key: 'wispsEnabled', label: 'Energy Wisps', type: 'toggle', path: 'visualisation.sceneEffects.wispsEnabled', description: 'Ephemeral glowing orbs that drift and fade' },
      { key: 'wispCount', label: 'Wisp Count', type: 'slider', min: 8, max: 128, step: 8, path: 'visualisation.sceneEffects.wispCount', description: 'Number of energy wisps' },
      { key: 'wispOpacity', label: 'Wisp Opacity', type: 'slider', min: 0, max: 1, step: 0.05, path: 'visualisation.sceneEffects.wispOpacity', description: 'Brightness of wisps' },
      { key: 'wispDriftSpeed', label: 'Wisp Speed', type: 'slider', min: 0, max: 3, step: 0.1, path: 'visualisation.sceneEffects.wispDriftSpeed', description: 'How fast wisps drift', isAdvanced: true },

      // Atmosphere (WASM) - Basic
      { key: 'fogEnabled', label: 'Atmosphere', type: 'toggle', path: 'visualisation.sceneEffects.fogEnabled', description: 'Nebula background texture' },
      { key: 'fogOpacity', label: 'Atmosphere Opacity', type: 'slider', min: 0, max: 0.15, step: 0.01, path: 'visualisation.sceneEffects.fogOpacity', description: 'Intensity of nebula background' },
      { key: 'atmosphereResolution', label: 'Atmosphere Detail', type: 'slider', min: 64, max: 256, step: 32, path: 'visualisation.sceneEffects.atmosphereResolution', description: 'Texture resolution (higher = more detail)', isAdvanced: true },

      // Bloom/Glow - Basic
      { key: 'glow', label: 'Bloom Glow', type: 'toggle', path: 'visualisation.glow.enabled', description: 'Enable bloom post-processing' },
      { key: 'glowIntensity', label: 'Glow Intensity', type: 'slider', min: 0, max: 1.5, step: 0.05, path: 'visualisation.glow.intensity', description: 'Brightness of bloom glow' },
      { key: 'glowRadius', label: 'Glow Radius', type: 'slider', min: 0, max: 1.0, step: 0.05, path: 'visualisation.glow.radius', description: 'Size of glow spread' },
      { key: 'glowThreshold', label: 'Glow Threshold', type: 'slider', min: 0, max: 1, step: 0.01, path: 'visualisation.glow.threshold', description: 'Minimum brightness for glow', isAdvanced: true },

      // Gem Material - Advanced
      { key: 'gemIor', label: 'Gem IOR', type: 'slider', min: 1.0, max: 3.0, step: 0.01, path: 'visualisation.gemMaterial.ior', description: 'Index of refraction for gem nodes', isAdvanced: true },
      { key: 'gemTransmission', label: 'Gem Transmission', type: 'slider', min: 0, max: 1, step: 0.01, path: 'visualisation.gemMaterial.transmission', description: 'Light transmission through gems', isAdvanced: true },
      { key: 'gemClearcoat', label: 'Gem Clearcoat', type: 'slider', min: 0, max: 1, step: 0.01, path: 'visualisation.gemMaterial.clearcoat', description: 'Clearcoat intensity on gems', isAdvanced: true },
      { key: 'gemClearcoatRoughness', label: 'Clearcoat Rough', type: 'slider', min: 0, max: 0.5, step: 0.01, path: 'visualisation.gemMaterial.clearcoatRoughness', description: 'Clearcoat roughness', isAdvanced: true },
      { key: 'gemEmissiveIntensity', label: 'Gem Emissive', type: 'slider', min: 0, max: 2, step: 0.05, path: 'visualisation.gemMaterial.emissiveIntensity', description: 'Emissive glow intensity of gems', isAdvanced: true },
      { key: 'gemIridescence', label: 'Gem Iridescence', type: 'slider', min: 0, max: 1, step: 0.05, path: 'visualisation.gemMaterial.iridescence', description: 'Rainbow sheen intensity', isAdvanced: true },

      // Embedding Cloud - Basic
      { key: 'embeddingCloudEnabled', label: 'Embedding Cloud', type: 'toggle', path: 'visualisation.embeddingCloud.enabled', description: 'Show RuVector embedding point cloud' },
      { key: 'embeddingCloudScale', label: 'Cloud Scale', type: 'slider', min: 0.5, max: 20, step: 0.5, path: 'visualisation.embeddingCloud.cloudScale', description: 'Overall scale of embedding cloud' },
      { key: 'embeddingPointSize', label: 'Point Size', type: 'slider', min: 0.5, max: 25, step: 0.5, path: 'visualisation.embeddingCloud.pointSize', description: 'Size of embedding points' },
      { key: 'embeddingOpacity', label: 'Cloud Opacity', type: 'slider', min: 0, max: 1, step: 0.05, path: 'visualisation.embeddingCloud.opacity', description: 'Transparency of embedding points' },
      { key: 'embeddingRotation', label: 'Rotation Speed', type: 'slider', min: 0, max: 0.005, step: 0.0001, path: 'visualisation.embeddingCloud.rotationSpeed', description: 'Auto-rotation speed', isAdvanced: true },

      // Animation - Basic
      { key: 'nodeAnimations', label: 'Node Animations', type: 'toggle', path: 'visualisation.animations.enableNodeAnimations', description: 'Enable node animations' },
      { key: 'pulseEnabled', label: 'Pulse Effect', type: 'toggle', path: 'visualisation.animations.pulseEnabled', description: 'Pulsing effect on nodes' },
      { key: 'pulseSpeed', label: 'Pulse Speed', type: 'slider', min: 0.1, max: 2, step: 0.1, path: 'visualisation.animations.pulseSpeed', description: 'Speed of pulse', isAdvanced: true },
      { key: 'pulseStrength', label: 'Pulse Strength', type: 'slider', min: 0.1, max: 2, step: 0.1, path: 'visualisation.animations.pulseStrength', description: 'Intensity of pulse', isAdvanced: true },
      { key: 'selectionWave', label: 'Selection Wave', type: 'toggle', path: 'visualisation.animations.selectionWaveEnabled', description: 'Wave effect on selection', isAdvanced: true },
      { key: 'waveSpeed', label: 'Wave Speed', type: 'slider', min: 0.1, max: 2, step: 0.1, path: 'visualisation.animations.waveSpeed', description: 'Speed of selection wave', isAdvanced: true }
    ]
  },

  // -------------------------------------------------------------------------
  // ANALYTICS TAB - Metrics and clustering (filtering moved to Quality tab)
  // -------------------------------------------------------------------------
  analytics: {
    title: 'Analytics & Metrics',
    fields: [
      // Clustering - Advanced
      { key: 'clusteringAlgorithm', label: 'Clustering Algorithm', type: 'select', options: ['none', 'kmeans', 'spectral', 'louvain', 'dbscan'], path: 'analytics.clustering.algorithm', description: 'Algorithm for clustering', isAdvanced: true },
      { key: 'clusterCount', label: 'Cluster Count', type: 'slider', min: 2, max: 20, step: 1, path: 'analytics.clustering.clusterCount', description: 'Number of clusters', isAdvanced: true },
      { key: 'clusterResolution', label: 'Resolution', type: 'slider', min: 0.1, max: 2, step: 0.1, path: 'analytics.clustering.resolution', description: 'Clustering resolution', isAdvanced: true },
      { key: 'clusterIterations', label: 'Cluster Iterations', type: 'slider', min: 10, max: 100, step: 5, path: 'analytics.clustering.iterations', description: 'Algorithm iterations', isAdvanced: true },

      // Cluster Hulls - Visual hull rendering around detected clusters
      { key: 'clusterHullsEnabled', label: 'Cluster Hulls', type: 'toggle', path: 'visualisation.clusterHulls.enabled', description: 'Show translucent hull around clusters' },
      { key: 'clusterHullsOpacity', label: 'Hull Opacity', type: 'slider', min: 0.01, max: 0.5, step: 0.01, path: 'visualisation.clusterHulls.opacity', description: 'Transparency of cluster hulls' },
      { key: 'clusterHullsMaxHulls', label: 'Max Hulls', type: 'slider', min: 4, max: 64, step: 1, path: 'visualisation.clusterHulls.maxHulls', description: 'Largest N spatial clusters to outline' }
    ]
  },

  // -------------------------------------------------------------------------
  // QUALITY TAB - Node filtering, GPU, and performance
  // -------------------------------------------------------------------------
  quality: {
    title: 'Quality & Filtering',
    fields: [
      // Node Filtering - Basic (moved from Analytics)
      { key: 'filterEnabled', label: 'Enable Filtering', type: 'toggle', path: 'nodeFilter.enabled', description: 'Enable node filtering' },
      { key: 'includeLinkedPages', label: 'Include Linked Pages', type: 'toggle', path: 'nodeFilter.includeLinkedPages', description: 'Show wikilink stub nodes (32K linked_page nodes). Disable for highest-quality view showing only fully-authored pages.' },
      { key: 'filterByQuality', label: 'Filter by Quality', type: 'toggle', path: 'nodeFilter.filterByQuality', description: 'Use quality score for filtering' },
      { key: 'qualityThreshold', label: 'Quality Threshold', type: 'slider', min: 0, max: 1, step: 0.05, path: 'nodeFilter.qualityThreshold', description: 'Minimum quality score (0-1)' },
      { key: 'filterByAuthority', label: 'Filter by Authority', type: 'toggle', path: 'nodeFilter.filterByAuthority', description: 'Use authority score for filtering' },
      { key: 'authorityThreshold', label: 'Authority Threshold', type: 'slider', min: 0, max: 1, step: 0.05, path: 'nodeFilter.authorityThreshold', description: 'Minimum authority score (0-1)' },
      { key: 'filterMode', label: 'Filter Mode', type: 'select', options: ['or', 'and'], path: 'nodeFilter.filterMode', description: 'How to combine filters (and = both, or = either)', isAdvanced: true },
      { key: 'refreshGraph', label: 'Refresh Graph', type: 'action-button', action: 'refresh_graph', description: 'Apply filter changes and reload graph' },

      // GPU Settings - Basic
      { key: 'autoAdjust', label: 'Auto-Adjust Quality', type: 'toggle', path: 'qualityGates.autoAdjust', description: 'Automatic quality scaling' },
      { key: 'minFpsThreshold', label: 'Min FPS Threshold', type: 'slider', min: 15, max: 60, step: 5, path: 'qualityGates.minFpsThreshold', description: 'Minimum acceptable FPS' },
      { key: 'maxNodeCount', label: 'Max Node Count', type: 'slider', min: 1000, max: 500000, step: 5000, path: 'qualityGates.maxNodeCount', description: 'Maximum nodes to render (set high to show all)' },

      // Visualization - Basic
      { key: 'showClusters', label: 'Show Clusters', type: 'toggle', path: 'qualityGates.showClusters', description: 'Color-coded node groups' },
      { key: 'showAnomalies', label: 'Show Anomalies', type: 'toggle', path: 'qualityGates.showAnomalies', description: 'Highlight outliers' },
      { key: 'showCommunities', label: 'Show Communities', type: 'toggle', path: 'qualityGates.showCommunities', description: 'Louvain communities', isAdvanced: true },

      // Advanced Features
      { key: 'gnnPhysics', label: 'GNN-Enhanced Physics', type: 'toggle', path: 'qualityGates.gnnPhysics', description: 'Graph Neural Network weights', isAdvanced: true, isPowerUserOnly: true },
      { key: 'ruvectorEnabled', label: 'RuVector Integration', type: 'toggle', path: 'qualityGates.ruvectorEnabled', description: 'HNSW similarity search', isAdvanced: true, isPowerUserOnly: true }
    ]
  },

  // -------------------------------------------------------------------------
  // SYSTEM TAB - Network and auth
  // -------------------------------------------------------------------------
  system: {
    title: 'System Settings',
    fields: [
      // Auth - Basic
      { key: 'nostr', label: 'Nostr Login', type: 'nostr-button', path: 'auth.nostr', description: 'Connect with Nostr' },
      { key: 'authEnabled', label: 'Auth Enabled', type: 'toggle', path: 'auth.enabled', description: 'Enable authentication', isAdvanced: true },
      { key: 'authRequired', label: 'Auth Required', type: 'toggle', path: 'auth.required', description: 'Require authentication', isAdvanced: true, isPowerUserOnly: true },

      // System - Basic
      { key: 'customBackendURL', label: 'Custom Backend URL', type: 'text', path: 'system.customBackendUrl', description: 'Override backend URL', isAdvanced: true, isPowerUserOnly: true }
    ]
  },

  // -------------------------------------------------------------------------
  // XR TAB - VR/AR settings (advanced)
  // -------------------------------------------------------------------------
  xr: {
    title: 'XR/AR Settings',
    isAdvanced: true,
    fields: [
      // Core XR - Basic within tab
      { key: 'xrEnabled', label: 'XR Mode', type: 'toggle', path: 'xr.enabled', description: 'Enable XR features' },
      { key: 'xrQuality', label: 'XR Quality', type: 'select', options: ['Low', 'Medium', 'High'], path: 'xr.quality', description: 'Rendering quality' },
      { key: 'xrRenderScale', label: 'XR Render Scale', type: 'slider', min: 0.5, max: 2, step: 0.1, path: 'xr.renderScale', description: 'Resolution scale' },

      // Hand Tracking
      { key: 'handTracking', label: 'Hand Tracking', type: 'toggle', path: 'xr.enableHandTracking', description: 'Enable hand input' },
      { key: 'enableHaptics', label: 'Haptics', type: 'toggle', path: 'xr.enableHaptics', description: 'Haptic feedback' }
    ]
  },

  // -------------------------------------------------------------------------
  // AI TAB - AI integrations (advanced, power user)
  // -------------------------------------------------------------------------
  ai: {
    title: 'AI Integrations',
    isAdvanced: true,
    isPowerUserOnly: true,
    fields: [
      // Perplexity
      { key: 'perplexityModel', label: 'Perplexity Model', type: 'text', path: 'perplexity.model', description: 'Model selection' },
      { key: 'perplexityMaxTokens', label: 'Max Tokens', type: 'slider', min: 100, max: 4096, step: 100, path: 'perplexity.maxTokens', description: 'Maximum response tokens' },
      { key: 'perplexityTemperature', label: 'Temperature', type: 'slider', min: 0, max: 2, step: 0.1, path: 'perplexity.temperature', description: 'Response randomness' },

      // Kokoro TTS
      { key: 'kokoroApiUrl', label: 'Kokoro API URL', type: 'text', path: 'kokoro.apiUrl', description: 'TTS endpoint' },
      { key: 'kokoroVoice', label: 'Default Voice', type: 'text', path: 'kokoro.defaultVoice', description: 'Voice selection' },
      { key: 'kokoroSpeed', label: 'Speech Speed', type: 'slider', min: 0.5, max: 2, step: 0.1, path: 'kokoro.defaultSpeed', description: 'Playback speed' }
    ]
  },

  // -------------------------------------------------------------------------
  // DEVELOPER TAB - Debug tools (advanced, power user)
  // -------------------------------------------------------------------------
  developer: {
    title: 'Developer Tools',
    isAdvanced: true,
    isPowerUserOnly: true,
    fields: [
      // Logging
      { key: 'enableDebug', label: 'Debug Mode', type: 'toggle', path: 'system.debug.enabled', description: 'Enable debug mode' },
      { key: 'enableDataDebug', label: 'Data Debug', type: 'toggle', path: 'system.debug.enableDataDebug', description: 'Log data operations' },
      { key: 'enableWebsocketDebug', label: 'WebSocket Debug', type: 'toggle', path: 'system.debug.enableWebsocketDebug', description: 'Log WebSocket traffic' },
      { key: 'logBinaryHeaders', label: 'Log Binary Headers', type: 'toggle', path: 'system.debug.logBinaryHeaders', description: 'Log binary message headers' },
      { key: 'logFullJson', label: 'Log Full JSON', type: 'toggle', path: 'system.debug.logFullJson', description: 'Log complete JSON payloads' },
      { key: 'enablePhysicsDebug', label: 'Physics Debug', type: 'toggle', path: 'system.debug.enablePhysicsDebug', description: 'Physics visualization' },
      { key: 'enableNodeDebug', label: 'Node Debug', type: 'toggle', path: 'system.debug.enableNodeDebug', description: 'Node state logging' },
      { key: 'enableShaderDebug', label: 'Shader Debug', type: 'toggle', path: 'system.debug.enableShaderDebug', description: 'Shader debugging' },
      { key: 'enableMatrixDebug', label: 'Matrix Debug', type: 'toggle', path: 'system.debug.enableMatrixDebug', description: 'Matrix transformations' },
      { key: 'enablePerformanceDebug', label: 'Performance Debug', type: 'toggle', path: 'system.debug.enablePerformanceDebug', description: 'Performance metrics' }
    ]
  }
};

// Helper to filter fields based on mode and permissions
export function filterSettingsFields(
  fields: SectionConfig['fields'],
  advancedMode: boolean,
  isPowerUser: boolean
): SectionConfig['fields'] {
  return fields.filter(field => {
    // Hide advanced fields in basic mode
    if (field.isAdvanced && !advancedMode) return false;
    // Hide power user fields from non-power users
    if (field.isPowerUserOnly && !isPowerUser) return false;
    return true;
  });
}

// Helper to filter tabs based on mode and permissions
export function filterTabs(
  tabs: UnifiedTabConfig[],
  advancedMode: boolean,
  isPowerUser: boolean
): UnifiedTabConfig[] {
  return tabs.filter(tab => {
    if (tab.isAdvanced && !advancedMode) return false;
    if (tab.isPowerUserOnly && !isPowerUser) return false;
    return true;
  });
}
