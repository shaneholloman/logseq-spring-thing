import { Vector3 } from 'three';
import { createLogger } from '../../../../utils/loggerConfig';
import type { GraphData } from '../../managers/graphDataManager';

import type {
  LayoutOptimization,
  ClusterDetection,
  NodeRecommendation,
  PatternRecognition,
  GraphMetrics,
  GraphPattern,
} from './types';

import { generateCacheKey } from './utils';
import { computeGraphMetrics } from './graphMetrics';
import {
  selectOptimalAlgorithm,
  applyOptimizationAlgorithm,
  calculateLayoutMetrics,
  calculateOptimizationConfidence,
  generateOptimizationReasoning,
} from './layoutOptimizer';
import {
  selectOptimalClusteringAlgorithm,
  applyClustering,
  calculateClusterQuality,
  generateClusterRecommendations,
} from './clusterDetector';
import {
  analyzeNodeConnectivity,
  analyzeNodePositioning,
  analyzeNodeType,
  generateNodeSpecificRecommendations,
} from './nodeRecommender';
import {
  detectGraphPatterns,
  analyzeCrossGraphPatterns,
  detectGraphAnomalies,
  generatePatternInsights,
} from './patternRecognizer';

export * from './types';

const logger = createLogger('AIInsights');

export class AIInsights {
  private static instance: AIInsights;
  private optimizationCache: Map<string, LayoutOptimization> = new Map();
  private clusterCache: Map<string, ClusterDetection> = new Map();
  private patternCache: Map<string, PatternRecognition> = new Map();
  private metricsCache: Map<string, GraphMetrics> = new Map();

  private constructor() {}

  public static getInstance(): AIInsights {
    if (!AIInsights.instance) {
      AIInsights.instance = new AIInsights();
    }
    return AIInsights.instance;
  }

  public async optimizeLayout(
    graphData: GraphData,
    currentPositions: Map<string, Vector3>,
    constraints: {
      preserveRelativePositions?: boolean;
      minimizeEdgeCrossings?: boolean;
      maximizeReadability?: boolean;
      respectClusters?: boolean;
    } = {}
  ): Promise<LayoutOptimization> {
    logger.info('Starting AI-powered layout optimization');

    const cacheKey = generateCacheKey(graphData, constraints);
    if (this.optimizationCache.has(cacheKey)) {
      return this.optimizationCache.get(cacheKey)!;
    }

    const currentMetrics = calculateLayoutMetrics(graphData, currentPositions);
    const algorithm = selectOptimalAlgorithm(graphData, constraints);
    const optimizedPositions = await applyOptimizationAlgorithm(
      graphData, currentPositions, algorithm, constraints
    );
    const improvedMetrics = calculateLayoutMetrics(graphData, optimizedPositions);

    const improvements = {
      edgeCrossings: { before: currentMetrics.edgeCrossings, after: improvedMetrics.edgeCrossings },
      nodeOverlaps: { before: currentMetrics.nodeOverlaps, after: improvedMetrics.nodeOverlaps },
      readability: { before: currentMetrics.readability, after: improvedMetrics.readability },
    };

    const toGraphMetrics = (m: typeof currentMetrics): GraphMetrics => ({
      density: 0, averagePathLength: 0, clusteringCoefficient: 0,
      centralization: 0, modularity: 0, efficiency: m.readability, smallWorldness: 0,
    });

    const optimization: LayoutOptimization = {
      algorithmUsed: algorithm,
      improvements,
      optimizedPositions,
      confidence: calculateOptimizationConfidence(toGraphMetrics(currentMetrics), toGraphMetrics(improvedMetrics)),
      reasoning: generateOptimizationReasoning(algorithm, improvements),
    };

    this.optimizationCache.set(cacheKey, optimization);
    return optimization;
  }

  public async detectClusters(
    graphData: GraphData,
    options: {
      algorithm?: ClusterDetection['algorithm'];
      minClusterSize?: number;
      maxClusters?: number;
    } = {}
  ): Promise<ClusterDetection> {
    logger.info('Detecting clusters using AI algorithms');

    const cacheKey = generateCacheKey(graphData, options);
    if (this.clusterCache.has(cacheKey)) {
      return this.clusterCache.get(cacheKey)!;
    }

    const algorithm = options.algorithm || selectOptimalClusteringAlgorithm(graphData);
    const clusters = await applyClustering(graphData, algorithm, options);
    const quality = calculateClusterQuality(graphData, clusters);
    const recommendations = generateClusterRecommendations(clusters, quality);

    const detection: ClusterDetection = { clusters, algorithm, quality, recommendations };
    this.clusterCache.set(cacheKey, detection);
    return detection;
  }

  public async generateNodeRecommendations(
    graphData: GraphData,
    targetNodeId?: string
  ): Promise<NodeRecommendation[]> {
    logger.info('Generating AI-powered node recommendations');

    const recommendations: NodeRecommendation[] = [];
    const nodes = targetNodeId
      ? [graphData.nodes.find(n => n.id === targetNodeId)!]
      : graphData.nodes;

    for (const node of nodes) {
      if (!node) continue;
      const connectivity = analyzeNodeConnectivity(node, graphData);
      const positioning = analyzeNodePositioning(node, graphData);
      const typeAnalysis = analyzeNodeType(node, graphData);
      recommendations.push(...generateNodeSpecificRecommendations(node, connectivity, positioning, typeAnalysis));
    }

    recommendations.sort((a, b) => {
      const score = (r: NodeRecommendation) =>
        r.confidence * (
          r.potentialImpact.connectivityImprovement +
          r.potentialImpact.readabilityImprovement +
          r.potentialImpact.structuralImprovement
        );
      return score(b) - score(a);
    });

    return recommendations.slice(0, 10);
  }

  public async recognizePatterns(
    logseqGraph: GraphData,
    visionclawGraph: GraphData,
    options: {
      detectAnomalies?: boolean;
      crossGraphAnalysis?: boolean;
      patternTypes?: GraphPattern['type'][];
    } = {}
  ): Promise<PatternRecognition> {
    logger.info('Recognizing patterns using AI algorithms');

    const cacheKey = generateCacheKey({ logseqGraph, visionclawGraph }, options);
    if (this.patternCache.has(cacheKey)) {
      return this.patternCache.get(cacheKey)!;
    }

    const logseqPatterns = await detectGraphPatterns(logseqGraph, options.patternTypes);
    const visionclawPatterns = await detectGraphPatterns(visionclawGraph, options.patternTypes);
    const crossGraphPatterns = options.crossGraphAnalysis
      ? await analyzeCrossGraphPatterns(logseqPatterns, visionclawPatterns)
      : [];
    const anomalies = options.detectAnomalies
      ? await detectGraphAnomalies(logseqGraph, visionclawGraph)
      : [];
    const insights = generatePatternInsights(
      [...logseqPatterns, ...visionclawPatterns], crossGraphPatterns, anomalies
    );

    const recognition: PatternRecognition = {
      patterns: [...logseqPatterns, ...visionclawPatterns],
      crossGraphPatterns,
      anomalies,
      insights,
    };

    this.patternCache.set(cacheKey, recognition);
    return recognition;
  }

  public calculateGraphMetrics(graphData: GraphData): GraphMetrics {
    const cacheKey = generateCacheKey(graphData);
    if (this.metricsCache.has(cacheKey)) {
      return this.metricsCache.get(cacheKey)!;
    }
    const metrics = computeGraphMetrics(graphData);
    this.metricsCache.set(cacheKey, metrics);
    return metrics;
  }

  public dispose(): void {
    this.optimizationCache.clear();
    this.clusterCache.clear();
    this.patternCache.clear();
    this.metricsCache.clear();
    logger.info('AI insights disposed');
  }
}

export const aiInsights = AIInsights.getInstance();
