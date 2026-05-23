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
      { key: 'nodeColor', label: 'Node Color', type: 'color', path: 'visualisation.graphs.logseq.nodes.baseColor', description: 'Base color for nodes' },
      { key: 'nodeSize', label: 'Node Size', type: 'slider', min: 0.2, max: 2, step: 0.1, path: 'visualisation.graphs.logseq.nodes.nodeSize', description: 'Size multiplier for nodes' },
      // Nodes - Advanced
      { key: 'enableMetadataShape', label: 'Metadata Shape', type: 'toggle', path: 'visualisation.graphs.logseq.nodes.enableMetadataShape', description: 'Shape based on metadata', isAdvanced: true },

      // Node Type Visibility
      { key: 'showKnowledge', label: 'Knowledge Nodes', type: 'toggle', path: 'visualisation.graphs.logseq.nodes.nodeTypeVisibility.knowledge', description: 'Show knowledge graph nodes' },
      { key: 'showOntology', label: 'Ontology Nodes', type: 'toggle', path: 'visualisation.graphs.logseq.nodes.nodeTypeVisibility.ontology', description: 'Show ontology nodes' },
      { key: 'showAgents', label: 'Agent Nodes', type: 'toggle', path: 'visualisation.graphs.logseq.nodes.nodeTypeVisibility.agent', description: 'Show agent nodes' },

      // Edges - Basic
      { key: 'edgeColor', label: 'Edge Color', type: 'color', path: 'visualisation.graphs.logseq.edges.color', description: 'Base color for edges' },
      { key: 'edgeWidth', label: 'Edge Thickness', type: 'slider', min: 0.005, max: 0.1, step: 0.005, path: 'visualisation.graphs.logseq.edges.edgeRadius', description: 'Thickness of edges' },
      { key: 'edgeOpacity', label: 'Edge Opacity', type: 'slider', min: 0, max: 1, step: 0.01, path: 'visualisation.graphs.logseq.edges.opacity', description: 'Transparency of edges' },

      // Knowledge Graph Mode - Basic
      { key: 'kgEdgeColor', label: 'KG Edge Color', type: 'color', path: 'visualisation.graphTypeVisuals.knowledgeGraph.edgeColor', description: 'Edge color for knowledge graph mode' },
      { key: 'ontologyEdgeColor', label: 'Ontology Edge Color', type: 'color', path: 'visualisation.graphTypeVisuals.ontology.edgeColor', description: 'Edge color for ontology mode', isAdvanced: true },

      // Labels - Basic
      { key: 'enableLabels', label: 'Show Labels', type: 'toggle', path: 'visualisation.graphs.logseq.labels.enableLabels', description: 'Display node labels' },
      { key: 'labelSize', label: 'Label Size', type: 'slider', min: 0.05, max: 3.0, step: 0.05, path: 'visualisation.graphs.logseq.labels.desktopFontSize', description: 'Font size for labels' },
      { key: 'labelColor', label: 'Label Color', type: 'color', path: 'visualisation.graphs.logseq.labels.textColor', description: 'Color of label text' },
      { key: 'showMetadata', label: 'Show Metadata', type: 'toggle', path: 'visualisation.graphs.logseq.labels.showMetadata', description: 'Show domain, links, and quality info under labels' },
      { key: 'labelStandoff', label: 'Label Standoff', type: 'slider', min: -1.0, max: 3.0, step: 0.05, path: 'visualisation.graphs.logseq.labels.textPadding', description: 'Gap between node surface and label' },
      // Labels - Advanced
      { key: 'labelOutlineColor', label: 'Outline Color', type: 'color', path: 'visualisation.graphs.logseq.labels.textOutlineColor', description: 'Label outline color', isAdvanced: true },
      { key: 'labelOutlineWidth', label: 'Outline Width', type: 'slider', min: 0, max: 0.01, step: 0.001, path: 'visualisation.graphs.logseq.labels.textOutlineWidth', description: 'Label outline width', isAdvanced: true },
      { key: 'labelDistanceThreshold', label: 'Label Draw Distance', type: 'slider', min: 50, max: 2000, step: 50, path: 'visualisation.graphs.logseq.labels.labelDistanceThreshold', description: 'Max camera distance for label visibility' },
      { key: 'maxLabelWidth', label: 'Max Label Width', type: 'slider', min: 2, max: 20, step: 0.5, path: 'visualisation.graphs.logseq.labels.maxLabelWidth', description: 'Maximum text wrapping width', isAdvanced: true },

      // Rendering - Basic
      { key: 'ambientLight', label: 'Ambient Light', type: 'slider', min: 0, max: 2, step: 0.1, path: 'visualisation.rendering.ambientLightIntensity', description: 'Overall scene brightness' },
      { key: 'directionalLight', label: 'Direct Light', type: 'slider', min: 0, max: 2, step: 0.1, path: 'visualisation.rendering.directionalLightIntensity', description: 'Directional light intensity' },
      // Rendering - Advanced (Phase 6 ADR-04 renderer-architectural controls)
      { key: 'maxEdgesCeiling', label: 'Max Edges Ceiling', type: 'slider', min: 1024, max: 262144, step: 1024, path: 'visualisation.rendering.maxEdgesCeiling', description: 'Hard cap on dynamically-grown edge instance capacity (Phase 6)', isAdvanced: true },
      { key: 'softwareFallback', label: 'Software WebGL Fallback', type: 'select', options: ['auto', 'force-on', 'force-off'], path: 'visualisation.rendering.softwareFallback', description: 'Behaviour on software-rendered WebGL contexts (SwiftShader/llvmpipe)', isAdvanced: true },
      { key: 'labelLayoutEvery', label: 'Label Layout Cadence (frames)', type: 'slider', min: 1, max: 10, step: 1, path: 'visualisation.rendering.labelLayoutEvery', description: 'Frames between full label re-layout passes', isAdvanced: true },

      // Selection Highlighting - Basic
      { key: 'selectionHighlightColor', label: 'Selection Color', type: 'color', path: 'visualisation.interaction.selectionHighlightColor', description: 'Edge color when node is selected' }
    ]
  },

  // -------------------------------------------------------------------------
  // PHYSICS TAB - Simulation controls
  // -------------------------------------------------------------------------
  physics: {
    title: 'Physics Simulation',
    fields: [
      // Core - Basic
      { key: 'enabled', label: 'Physics Enabled', type: 'toggle', path: 'visualisation.graphs.logseq.physics.enabled', description: 'Enable physics simulation' },
      { key: 'autoBalance', label: 'Auto Balance', type: 'toggle', path: 'visualisation.graphs.logseq.physics.autoBalance', description: 'Adaptive force balancing' },
      { key: 'damping', label: 'Damping', type: 'slider', min: 0, max: 1, step: 0.01, path: 'visualisation.graphs.logseq.physics.damping', description: 'Velocity damping — lower = more energy, higher = faster settle' },
      { key: 'springK', label: 'Spring Strength', type: 'slider', min: 0.01, max: 100, step: 0.5, path: 'visualisation.graphs.logseq.physics.springK', description: 'Edge spring constant (recommended: 8-20 for 2K+ node graphs)' },
      { key: 'repelK', label: 'Repulsion', type: 'slider', min: 0, max: 3000, step: 10, path: 'visualisation.graphs.logseq.physics.repelK', description: 'Node repulsion — balance with gravity (recommended: 800-1500)' },
      { key: 'gravity', label: 'Gravity', type: 'slider', min: 0, max: 0.01, step: 0.0001, path: 'visualisation.graphs.logseq.physics.gravity', description: 'Center-pull force — affects how loosely-connected nodes drift' },

      // --- Layout Mode (moved from Quality) ---
      { key: 'layoutMode', label: 'Layout Mode', type: 'select', options: ['force-directed', 'dag-topdown', 'dag-radial', 'dag-leftright', 'type-clustering'], path: 'qualityGates.layoutMode', description: 'Graph layout algorithm — force-directed uses spring/repulsion, DAG modes add hierarchical layout, type-clustering groups by node type' },

      // --- Ontology Forces (moved from Quality) ---
      { key: 'ontologyPhysics', label: 'Ontology Forces', type: 'toggle', path: 'qualityGates.ontologyPhysics', description: 'Enable OWL ontology-derived constraint forces in the physics simulation' },
      { key: 'ontologyStrength', label: 'Ontology Strength', type: 'slider', min: 0, max: 1, step: 0.05, path: 'qualityGates.ontologyStrength', description: 'Global strength of ontology constraint forces (lower = gentler, higher = stricter)', isAdvanced: true },

      // --- Semantic Layout Forces (moved from Quality) ---
      { key: 'semanticForces', label: 'Semantic Layout Forces', type: 'toggle', path: 'qualityGates.semanticForces', description: 'Enable DAG hierarchy layout and type-based clustering forces' },
      { key: 'dagLevelAttraction', label: 'DAG Level Attraction', type: 'slider', min: 0, max: 2.0, step: 0.05, path: 'qualityGates.dagLevelAttraction', description: 'How strongly nodes pull toward their hierarchy level', isAdvanced: true },
      { key: 'dagSiblingRepulsion', label: 'DAG Sibling Repulsion', type: 'slider', min: 0, max: 2.0, step: 0.05, path: 'qualityGates.dagSiblingRepulsion', description: 'How strongly same-level nodes spread apart', isAdvanced: true },
      { key: 'typeClusterAttraction', label: 'Type Cluster Attraction', type: 'slider', min: 0, max: 2.0, step: 0.05, path: 'qualityGates.typeClusterAttraction', description: 'How strongly same-type nodes group together', isAdvanced: true },
      { key: 'typeClusterRadius', label: 'Type Cluster Radius', type: 'slider', min: 10, max: 500, step: 10, path: 'qualityGates.typeClusterRadius', description: 'Target radius for type-based cluster zones', isAdvanced: true },

      // Dynamics - Basic
      { key: 'globalSpeed', label: 'Global Speed', type: 'slider', min: 0.01, max: 5.0, step: 0.01, path: 'visualisation.graphs.logseq.physics.globalSpeed', description: 'FA2 base integration speed (kernel param, was implicitly dt × 10)' },
      { key: 'maxVelocity', label: 'Max Velocity', type: 'slider', min: 0.1, max: 200, step: 1, path: 'visualisation.graphs.logseq.physics.maxVelocity', description: 'Maximum node speed — higher allows faster layout changes' },
      { key: 'enableBounds', label: 'Enable Bounds', type: 'toggle', path: 'visualisation.graphs.logseq.physics.enableBounds', description: 'Constrain to bounds' },
      { key: 'boundsSize', label: 'Bounds Size', type: 'slider', min: 100, max: 20000, step: 500, path: 'visualisation.graphs.logseq.physics.boundsSize', description: 'Size of bounding box — larger allows more spread' },

      // Advanced dynamics
      { key: 'dt', label: 'Time Step', type: 'slider', min: 0.001, max: 0.1, step: 0.001, path: 'visualisation.graphs.logseq.physics.dt', description: 'Simulation time step', isAdvanced: true },
      { key: 'separationRadius', label: 'Separation Radius', type: 'slider', min: 0.1, max: 50, step: 0.5, path: 'visualisation.graphs.logseq.physics.separationRadius', description: 'Minimum node separation — tiny for dense, large for spacing', isAdvanced: true },
      { key: 'iterations', label: 'Iterations', type: 'slider', min: 1, max: 5000, step: 50, path: 'visualisation.graphs.logseq.physics.iterations', description: 'Solver iterations per frame — more = finer resolution', isAdvanced: true },
      { key: 'warmupIterations', label: 'Warmup Iterations', type: 'slider', min: 0, max: 500, step: 10, path: 'visualisation.graphs.logseq.physics.warmupIterations', description: 'Initial stabilization iterations', isAdvanced: true },
      { key: 'coolingRate', label: 'Cooling Rate', type: 'slider', min: 0.00001, max: 0.01, step: 0.0001, path: 'visualisation.graphs.logseq.physics.coolingRate', description: 'Simulated annealing rate', isAdvanced: true },

      // Fine-tuning - Advanced
      { key: 'minDistance', label: 'Min Distance', type: 'slider', min: 0.05, max: 5, step: 0.1, path: 'visualisation.graphs.logseq.physics.minDistance', description: 'Minimum repulsion distance', isAdvanced: true },
      { key: 'maxRepulsionDist', label: 'Max Repulsion Dist', type: 'slider', min: 10, max: 2000, step: 10, path: 'visualisation.graphs.logseq.physics.maxRepulsionDist', description: 'Maximum repulsion range — larger affects more distant nodes', isAdvanced: true },
      { key: 'restLength', label: 'Node Spacing', type: 'slider', min: 1, max: 200, step: 1, path: 'visualisation.graphs.logseq.physics.restLength', description: 'Spring rest length — small = dense, large = spread' },
      { key: 'centerGravityK', label: 'Cluster Tightness', type: 'slider', min: 0, max: 1.0, step: 0.01, path: 'visualisation.graphs.logseq.physics.centerGravityK', description: 'Pull towards center — higher values tightly cluster the graph' },
      { key: 'clusterStrength', label: 'Cluster Strength', type: 'slider', min: 0, max: 2, step: 0.05, path: 'visualisation.graphs.logseq.physics.clusterStrength', description: 'How strongly nodes cluster together', isAdvanced: true },
      { key: 'alignmentStrength', label: 'Alignment Strength', type: 'slider', min: 0, max: 2, step: 0.05, path: 'visualisation.graphs.logseq.physics.alignmentStrength', description: 'Flocking cohesion force between nearby nodes', isAdvanced: true },
      { key: 'constraintRampFrames', label: 'Constraint Ramp', type: 'slider', min: 1, max: 300, step: 10, path: 'visualisation.graphs.logseq.physics.constraintRampFrames', description: 'Frames over which ontology constraints ramp up after a change', isAdvanced: true },
      { key: 'constraintMaxForce', label: 'Constraint Max Force', type: 'slider', min: 1, max: 200, step: 5, path: 'visualisation.graphs.logseq.physics.constraintMaxForcePerNode', description: 'Per-node cap on ontology constraint forces', isAdvanced: true },

      // FA2 advanced controls
      { key: 'linLogMode', label: 'LinLog Mode', type: 'toggle', path: 'visualisation.graphs.logseq.physics.linLogMode', description: 'Logarithmic attraction (modularity-preserving) vs linear Hooke springs', isAdvanced: true },
      { key: 'scalingRatio', label: 'FA2 Scaling Ratio', type: 'slider', min: 0.1, max: 100, step: 0.5, path: 'visualisation.graphs.logseq.physics.scalingRatio', description: 'ForceAtlas2 repulsion scaling — higher spreads degree-heavy nodes further', isAdvanced: true },
      { key: 'adaptiveSpeed', label: 'Adaptive Speed', type: 'toggle', path: 'visualisation.graphs.logseq.physics.adaptiveSpeed', description: 'Per-node adaptive convergence speed (reduces oscillation)', isAdvanced: true },
      { key: 'graphSeparationX', label: 'Graph Separation', type: 'slider', min: 0, max: 2000, step: 50, path: 'visualisation.graphs.logseq.physics.graphSeparationX', description: 'X-axis separation between knowledge and ontology populations (0 = merged)', isAdvanced: true },
      { key: 'axisCompressionZ', label: 'Z-Axis Compression', type: 'slider', min: 0, max: 1, step: 0.05, path: 'visualisation.graphs.logseq.physics.axisCompressionZ', description: 'Flatten KG + ontology toward XY plane (0 = full 3D, 1 = flat disks). Agents stay 3D as bridges.', isAdvanced: true },
      { key: 'gridCellSize', label: 'Grid Cell Size', type: 'slider', min: 10, max: 500, step: 10, path: 'visualisation.graphs.logseq.physics.gridCellSize', description: 'Spatial grid cell size — larger for spread-out graphs', isAdvanced: true },
      { key: 'repulsionSofteningEpsilon', label: 'Repulsion Epsilon', type: 'slider', min: 0.00001, max: 0.01, step: 0.0001, path: 'visualisation.graphs.logseq.physics.repulsionSofteningEpsilon', description: 'Softening for close nodes', isAdvanced: true },
      { key: 'boundaryDamping', label: 'Boundary Damping', type: 'slider', min: 0, max: 1, step: 0.01, path: 'visualisation.graphs.logseq.physics.boundaryDamping', description: 'Velocity damping when nodes approach boundary', isAdvanced: true },
      { key: 'maxForce', label: 'Max Force', type: 'slider', min: 1, max: 200, step: 1, path: 'visualisation.graphs.logseq.physics.maxForce', description: 'Maximum force per node — higher allows stronger layout forces', isAdvanced: true },
      { key: 'temperature', label: 'Temperature', type: 'slider', min: 0.001, max: 10, step: 0.01, path: 'visualisation.graphs.logseq.physics.temperature', description: 'Simulation temperature (energy) — higher = more movement', isAdvanced: true },

      // Client-side tweening / interpolation - Basic
      { key: 'tweeningEnabled', label: 'Smooth Node Movement', type: 'toggle', path: 'visualisation.graphs.logseq.tweening.enabled', description: 'Smoothly animate nodes toward server positions instead of snapping instantly' },
      { key: 'tweeningLerpBase', label: 'Node Animation Speed', type: 'slider', min: 0.0001, max: 0.15, step: 0.001, path: 'visualisation.graphs.logseq.tweening.lerpBase', description: 'How quickly nodes reach their target positions (lower = faster, higher = smoother)' },
      { key: 'tweeningMaxDivergence', label: 'Maximum Node Jump', type: 'slider', min: 1, max: 100, step: 1, path: 'visualisation.graphs.logseq.tweening.maxDivergence', description: 'Distance threshold above which nodes snap instantly instead of animating' },
      // Client-side tweening - Advanced
      { key: 'tweeningSnapThreshold', label: 'Snap Distance', type: 'slider', min: 0.01, max: 1.0, step: 0.01, path: 'visualisation.graphs.logseq.tweening.snapThreshold', description: 'Distance below which nodes snap to their target (sub-pixel precision)', isAdvanced: true }
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
      { key: 'clusterHullsOpacity', label: 'Hull Opacity', type: 'slider', min: 0.01, max: 0.3, step: 0.01, path: 'visualisation.clusterHulls.opacity', description: 'Transparency of cluster hulls' }
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
