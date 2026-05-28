import { Vector3, Color } from 'three';

export interface LayoutOptimization {
  algorithmUsed: 'force-directed' | 'hierarchical' | 'circular' | 'grid' | 'organic';
  improvements: {
    edgeCrossings: { before: number; after: number };
    nodeOverlaps: { before: number; after: number };
    readability: { before: number; after: number };
  };
  optimizedPositions: Map<string, Vector3>;
  confidence: number;
  reasoning: string[];
}

export interface ClusterDetection {
  clusters: GraphCluster[];
  algorithm: 'modularity' | 'density' | 'hierarchical' | 'spectral';
  quality: {
    modularity: number;
    silhouette: number;
    cohesion: number;
  };
  recommendations: string[];
}

export interface GraphCluster {
  id: string;
  nodes: string[];
  centerPosition: Vector3;
  radius: number;
  density: number;
  dominantTypes: string[];
  characteristics: {
    averageConnections: number;
    internalEdges: number;
    externalEdges: number;
    coherenceScore: number;
  };
  suggestedColor: Color;
  label: string;
}

export interface NodeRecommendation {
  nodeId: string;
  recommendationType: 'connect' | 'group' | 'highlight' | 'relocate' | 'merge' | 'split';
  confidence: number;
  reasoning: string;
  suggestedActions: RecommendedAction[];
  potentialImpact: {
    connectivityImprovement: number;
    readabilityImprovement: number;
    structuralImprovement: number;
  };
}

export interface RecommendedAction {
  type: 'create_edge' | 'move_node' | 'change_color' | 'add_label' | 'group_nodes';
  parameters: any;
  description: string;
  priority: 'low' | 'medium' | 'high';
}

export interface PatternRecognition {
  patterns: GraphPattern[];
  crossGraphPatterns: CrossGraphPattern[];
  anomalies: GraphAnomaly[];
  insights: string[];
}

export interface GraphPattern {
  id: string;
  type: 'hub' | 'chain' | 'star' | 'clique' | 'bridge' | 'community' | 'hierarchy';
  nodes: string[];
  strength: number;
  description: string;
  significance: number;
  visualizationHint: {
    highlight: boolean;
    color: Color;
    style: 'outline' | 'fill' | 'glow';
  };
}

export interface CrossGraphPattern {
  id: string;
  logseqPattern: GraphPattern;
  visionclawPattern: GraphPattern;
  similarity: number;
  relationship: 'identical' | 'similar' | 'complementary' | 'contradictory';
  insights: string[];
}

export interface GraphAnomaly {
  id: string;
  type: 'isolated_node' | 'unusual_hub' | 'broken_cluster' | 'duplicate_structure' | 'missing_connection';
  affectedNodes: string[];
  severity: 'low' | 'medium' | 'high';
  description: string;
  suggestedFix: string;
}

export interface GraphMetrics {
  density: number;
  averagePathLength: number;
  clusteringCoefficient: number;
  centralization: number;
  modularity: number;
  efficiency: number;
  smallWorldness: number;
}
