// api/settings/types.ts
// Type definitions matching Rust backend exactly

export interface PhysicsSettings {
  autoBalance: boolean;
  autoBalanceIntervalMs: number;
  autoBalanceConfig: {
    maxIterations: number;
    threshold: number;
  };
  autoPause: {
    enabled: boolean;
    inactivityThresholdMs: number;
  };
  springK: number;
  repelK: number;
  damping: number;
  dt: number;
  gravity: number;
  centerGravityK: number;
  temperature: number;
  restLength: number;
  maxVelocity: number;
  maxForce: number;
  boundsSize: number;
  boundaryDamping: number;
  separationRadius: number;
  gridCellSize: number;
  maxRepulsionDist: number;
  warmupIterations: number;
  iterations: number;
  coolingRate: number;
  linLogMode: boolean;
  scalingRatio: number;
  adaptiveSpeed: boolean;
  graphSeparationX: number;
  enableBounds: boolean;
  enabled: boolean;
}

export type PriorityWeighting = 'linear' | 'exponential' | 'quadratic';

export interface ConstraintSettings {
  lodEnabled: boolean;
  farThreshold: number;
  mediumThreshold: number;
  nearThreshold: number;
  priorityWeighting: PriorityWeighting;
  progressiveActivation: boolean;
  activationFrames: number;
}

export interface RenderingSettings {
  ambientLightIntensity: number;
  backgroundColor: string;
  directionalLightIntensity: number;
  enableAmbientOcclusion: boolean;
  enableAntialiasing: boolean;
  enableShadows: boolean;
  environmentIntensity: number;
  shadowMapSize?: string;
  shadowBias?: number;
  context?: string;
  /** Phase 6 (ADR-04 D1): hard ceiling on dynamic edge capacity. Default 32_768. */
  maxEdgesCeiling?: number;
  /** Phase 6 (ADR-04 D5): software-WebGL fallback policy. Default 'auto'. */
  softwareFallback?: 'auto' | 'force-on' | 'force-off';
  /** Phase 6 (ADR-04 D6): frames between full label layout rebuilds. Default 3. */
  labelLayoutEvery?: number;
}

export interface NodeFilterSettings {
  enabled: boolean;
  qualityThreshold: number;
  authorityThreshold: number;
  filterByQuality: boolean;
  filterByAuthority: boolean;
  filterMode: 'or' | 'and';
  includeLinkedPages: boolean;
}

export interface QualityGateSettings {
  gpuAcceleration: boolean;
  ontologyPhysics: boolean;
  semanticForces: boolean;
  layoutMode: 'force-directed' | 'dag-topdown' | 'dag-radial' | 'dag-leftright' | 'type-clustering';
  showClusters: boolean;
  showAnomalies: boolean;
  showCommunities: boolean;
  ruvectorEnabled: boolean;
  gnnPhysics: boolean;
  minFpsThreshold: number;
  maxNodeCount: number;
  autoAdjust: boolean;
  ontologyStrength: number;
  dagLevelAttraction: number;
  dagSiblingRepulsion: number;
  typeClusterAttraction: number;
  typeClusterRadius: number;
}

export interface AllSettings {
  physics: PhysicsSettings;
  constraints: ConstraintSettings;
  rendering: RenderingSettings;
  nodeFilter: NodeFilterSettings;
  qualityGates: QualityGateSettings;
  visual?: Record<string, unknown>;
}

export interface SettingsProfile {
  id: number;
  name: string;
  createdAt: string;
  updatedAt: string;
}

export interface SaveProfileRequest {
  name: string;
}

export interface ProfileIdResponse {
  id: number;
}

export interface ErrorResponse {
  error: string;
}
