// api/settings/defaults.ts
// Default settings objects — must mirror Rust backend defaults exactly

import type { PhysicsSettings } from './types';

// ============================================================================
// Physics defaults — must mirror Rust SimParams defaults exactly
// ============================================================================

export const DEFAULT_PHYSICS_SETTINGS: Partial<PhysicsSettings> = {
  springK: 12.0,
  repelK: 800.0,
  damping: 0.85,
  dt: 0.016,
  gravity: 0.0001,
  centerGravityK: 0.05,
  temperature: 0.01,
  restLength: 80.0,
  maxVelocity: 200.0,
  maxForce: 50.0,
  boundsSize: 1200.0,
  boundaryDamping: 0.8,
  separationRadius: 3.0,
  gridCellSize: 40.0,
  maxRepulsionDist: 2000.0,
  warmupIterations: 200,
  iterations: 200,
  coolingRate: 0.002,
  linLogMode: true,
  scalingRatio: 10.0,
  adaptiveSpeed: true,
  graphSeparationX: 1000.0,
  axisCompressionZ: 0.9,
  enabled: true,
  enableBounds: true,
};

// ============================================================================
// Visual effect defaults (used when API doesn't provide them)
// ============================================================================

export const DEFAULT_GLOW_SETTINGS = {
  enabled: true,
  intensity: 0.5,
  radius: 0.3,
  threshold: 0.3,
  diffuseStrength: 0.3,
  atmosphericDensity: 0.2,
  volumetricIntensity: 0.3,
  baseColor: '#ffffff',
  emissionColor: '#00ffff',
  opacity: 1.0,
  pulseSpeed: 1.0,
  flowSpeed: 0.5,
  nodeGlowStrength: 0.6,
  edgeGlowStrength: 0.3,
  environmentGlowStrength: 0.2
};

export const DEFAULT_BLOOM_SETTINGS = {
  enabled: true,
  intensity: 0.4,
  threshold: 0.3,
  radius: 0.3,
  strength: 0.4
};

export const DEFAULT_HOLOGRAM_SETTINGS = {
  ringCount: 3,
  ringColor: '#00ffff',
  ringOpacity: 0.5,
  sphereSizes: [100, 150] as [number, number],
  globalRotationSpeed: 0.5,
  ringRotationSpeed: 0.5,
};

export const DEFAULT_GEM_MATERIAL = {
  ior: 2.42,
  transmission: 0.6,
  clearcoat: 1.0,
  clearcoatRoughness: 0.02,
  emissiveIntensity: 0.6,
  iridescence: 0.3,
};

export const DEFAULT_SCENE_EFFECTS = {
  enabled: true,
  particleCount: 128,
  particleOpacity: 0.3,
  particleDrift: 0.5,
  particleColor: '#6680E6',
  wispsEnabled: true,
  wispCount: 32,
  wispOpacity: 0.4,
  wispDriftSpeed: 1.0,
  wispColor: '#668FCC',
  fogEnabled: false,
  fogOpacity: 0.05,
  atmosphereResolution: 128,
};

export const DEFAULT_CLUSTER_HULLS = {
  enabled: false,
  opacity: 0.08,
  padding: 0.15,
};

export const DEFAULT_EMBEDDING_CLOUD = {
  enabled: true,
  pointSize: 7.5,
  opacity: 0.6,
  colorBy: 'namespace' as const,
  rotationSpeed: 0.0005,
  maxPoints: 50000,
  cloudScale: 5.0,
};

export const DEFAULT_ANIMATION_SETTINGS = {
  enableMotionBlur: false,
  enableNodeAnimations: true,
  motionBlurStrength: 0.5,
  selectionWaveEnabled: true,
  pulseEnabled: true,
  pulseSpeed: 1.0,
  pulseStrength: 0.5,
  waveSpeed: 1.0,
};

export const DEFAULT_INTERACTION_SETTINGS = {
  selectionHighlightColor: '#ffff00',
  selectionEdgeFlow: false,
  selectionEdgeFlowSpeed: 1.0,
  selectionEdgeWidth: 0.5,
  selectionEdgeOpacity: 0.8,
};

export const DEFAULT_NODES_SETTINGS = {
  baseColor: '#4a6fa5',
  metalness: 0.1,
  opacity: 1.0,
  roughness: 0.6,
  nodeSize: 1.7,
  quality: 'high' as const,
  enableInstancing: true,
  enableMetadataShape: false,
  enableMetadataVisualisation: true,
  nodeTypeVisibility: {
    knowledge: true,
    ontology: true,
    agent: true,
  }
};

export const DEFAULT_EDGES_SETTINGS = {
  arrowSize: 0.02,
  baseWidth: 0.61,
  color: '#ff0000',
  enableArrows: false,
  opacity: 0.5,
  widthRange: [0.3, 1.5] as [number, number],
  quality: 'high' as const,
  enableFlowEffect: false,
  flowSpeed: 1.0,
  flowIntensity: 0.5,
  glowStrength: 0.3,
  distanceIntensity: 0.5,
  useGradient: false,
  gradientColors: ['#4a9eff', '#ff4a9e'] as [string, string]
};

export const DEFAULT_LABELS_SETTINGS = {
  desktopFontSize: 0.4,
  enableLabels: true,
  labelDistanceThreshold: 1200,
  textColor: '#676565',
  textOutlineColor: '#00ff40',
  textOutlineWidth: 0.0074725277,
  textResolution: 32,
  textPadding: 0.3,
  billboardMode: 'camera' as const,
  showMetadata: true,
  maxLabelWidth: 5.0
};

export const DEFAULT_GRAPH_TYPE_VISUALS = {
  knowledgeGraph: {
    metalness: 0.6,
    roughness: 0.15,
    glowStrength: 2.5,
    innerGlowIntensity: 0.3,
    facetDetail: 2,
    authorityScaleFactor: 0.5,
    connectionInfluence: 0.4,
    globalScaleMultiplier: 2.5,
    showDomainBadge: true,
    showQualityStars: true,
    showRecencyIndicator: true,
    showConnectionDensity: false,
  },
  ontology: {
    glowStrength: 1.8,
    orbitalRingCount: 8,
    orbitalRingSpeed: 0.5,
    hierarchyScaleFactor: 0.15,
    minScale: 0.4,
    instanceCountInfluence: 0.1,
    depthColorGradient: true,
    showHierarchyBreadcrumb: true,
    showInstanceCount: true,
    showConstraintStatus: false,
    nebulaGlowIntensity: 0.7,
  },
  agent: {
    membraneOpacity: 0.7,
    nucleusGlowIntensity: 0.6,
    breathingSpeed: 1.5,
    breathingAmplitude: 0.4,
    workloadInfluence: 0.3,
    tokenRateInfluence: 100,
    tokenRateCap: 0.5,
    showHealthBar: true,
    showTokenRate: true,
    showTaskCount: false,
    bioluminescentIntensity: 0.6,
  },
};
