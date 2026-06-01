import React, { useRef, useMemo, useEffect, useState } from 'react';
import { useFrame } from '@react-three/fiber';
import * as THREE from 'three';
import { ConvexGeometry } from 'three/examples/jsm/geometries/ConvexGeometry.js';
import { nodeAnalyticsStore } from '../../analytics/store/nodeAnalyticsStore';

// ============================================================================
// Types
// ============================================================================

interface ClusterHullsProps {
  nodes: Array<{
    id: string;
    label?: string;
    metadata?: any;
    position?: { x: number; y: number; z: number };
  }>;
  nodePositionsRef: React.RefObject<Float32Array | null>;
  nodeIdToIndexMap: Map<string, number>;
  settings: any;
}

// ============================================================================
// Constants
// ============================================================================

const DOMAIN_COLORS: Record<string, string> = {
  'AI': '#4FC3F7',
  'BC': '#81C784',
  'RB': '#FFB74D',
  'MV': '#CE93D8',
  'TC': '#FFD54F',
  'DT': '#EF5350',
  'NGM': '#4DB6AC',
  'SEC': '#FF7043',
  'INFRA': '#78909C',
};

const DEFAULT_COLOR = '#90A4AE';

const MIN_CLUSTER_SIZE = 4;
const TICK_INTERVAL = 30;
// GPU Louvain on this graph yields ~2000 communities (mostly singletons/tiny);
// drawing every cluster >= MIN_CLUSTER_SIZE renders 800+ overlapping translucent
// hulls — opaque mush, not inspectable. Cap to the N largest communities so
// distinct regions stay legible in 3D. Overridable via clusterHulls.maxHulls.
const DEFAULT_MAX_HULLS = 32;

// ============================================================================
// Helpers
// ============================================================================

/**
 * Get cluster key from analytics buffer (cluster_id) when available,
 * falling back to domain-based grouping from node label/metadata.
 */
function getClusterKey(
  node: { id: string; metadata?: any; label?: string },
  nodeIndex?: number,
  analyticsBuffer?: Float32Array | null,
): string {
  // 1. Prefer server-provided cluster_id from binary protocol V3 analytics
  if (analyticsBuffer && nodeIndex !== undefined) {
    const clusterId = analyticsBuffer[nodeIndex * 3]; // index 0 = cluster_id
    if (clusterId > 0) {
      return `cluster-${clusterId}`;
    }
  }

  // 2. Check metadata for explicit domain
  const domain =
    node.metadata?.source_domain ??
    node.metadata?.domain ??
    node.metadata?.cluster;
  if (domain && domain !== 'unknown') return domain;

  // 3. Extract domain from source_file prefix (most reliable — ontology IRI encoding)
  const sourceFile = node.metadata?.source_file ?? '';
  const sfMatch = sourceFile.match(/^(BC|AI|MV|RB|TC|DT|NGM)[-_]/i);
  if (sfMatch) return sfMatch[1].toUpperCase();

  // 4. Extract domain from label prefix (e.g., "BC-0034-nonce" → "BC")
  const label = node.label ?? node.metadata?.label ?? '';
  const prefixMatch = label.match(/^(BC|AI|MV|RB|TC|DT|NGM|bc|ai|mv|rb|tc|dt|ngm)[-:_]/i);
  if (prefixMatch) return prefixMatch[1].toUpperCase();

  // 5. Detect domain from keywords in label (expanded for better coverage)
  if (label.match(/Blockchain|Crypto|Token|DeFi|Smart Contract|Bitcoin|Ethereum|Consensus|ERC\d|Staking|Wallet|Ledger|Mining|DAO|DApp|Solidity|Proof.of/i)) return 'BC';
  if (label.match(/Artificial Intelligence|Machine Learning|Neural|NLP|Deep Learning|Chatbot|GPT|LLM|Transformer|Embedding|Inference|Generative|Anthropic|Claude|Reinforcement|Classification|Prompt/i)) return 'AI';
  if (label.match(/Metaverse|VR|AR|XR|Avatar|Render|Digital Twin|Hologram|Augmented|Virtual Reality|Mixed Reality|Spatial|Haptic|Immersive|Gaming|3D|Scene/i)) return 'MV';
  if (label.match(/Robot|Drone|Sensor|IoT|Actuator|Autonomous/i)) return 'RB';
  if (label.match(/Security|Privacy|Auth|Encrypt|Access Control|Firewall|Vulnerability|Threat|Compliance|GDPR/i)) return 'SEC';
  if (label.match(/Network|Protocol|API|Infrastructure|Server|Cloud|Database|Storage|Computing|Compute|Docker|Kubernetes/i)) return 'INFRA';

  // Skip 'unknown' group entirely — don't create a hull for unclassified nodes
  return '';
}

// GPU cluster palette for numeric cluster IDs
const GPU_CLUSTER_COLORS = [
  '#4FC3F7', '#81C784', '#FFB74D', '#CE93D8', '#FFD54F',
  '#EF5350', '#4DB6AC', '#FF7043', '#78909C', '#AED581',
  '#F48FB1', '#80DEEA', '#FFCC80', '#B39DDB', '#A5D6A7',
  '#90CAF9', '#FFAB91', '#80CBC4', '#FFF176', '#E6EE9C',
];

function getDomainHullColor(domain: string): string {
  // Handle GPU cluster IDs (cluster-1, cluster-2, etc.)
  const clusterMatch = domain.match(/^cluster-(\d+)$/);
  if (clusterMatch) {
    const idx = parseInt(clusterMatch[1], 10) % GPU_CLUSTER_COLORS.length;
    return GPU_CLUSTER_COLORS[idx];
  }
  return DOMAIN_COLORS[domain] ?? DEFAULT_COLOR;
}

/**
 * Given a set of 3D points, compute padded points offset from centroid,
 * then return a ConvexGeometry. Returns null if fewer than 4 valid points.
 */
function buildHullGeometry(
  points: THREE.Vector3[],
  padding: number,
  slabThickness: number,
): ConvexGeometry | null {
  if (points.length < MIN_CLUSTER_SIZE) return null;

  // Compute centroid
  const centroid = new THREE.Vector3();
  for (const p of points) {
    centroid.add(p);
  }
  centroid.divideScalar(points.length);

  // Offset each point away from centroid by (1 + padding), then split into a
  // thin Z slab. The co-planar disc projection flattens Z to ~3%, leaving
  // each cluster's points near-coplanar — a 3D ConvexGeometry of a coplanar
  // set is degenerate (throws, or yields a zero-volume sliver invisible from
  // top-down). Duplicating each point at z ± slabThickness gives the hull
  // real volume so it renders as a filled region when viewed onto the disc.
  const slab: THREE.Vector3[] = [];
  for (const p of points) {
    const dir = new THREE.Vector3().subVectors(p, centroid);
    const padded = centroid.clone().add(dir.multiplyScalar(1 + padding));
    slab.push(new THREE.Vector3(padded.x, padded.y, padded.z + slabThickness));
    slab.push(new THREE.Vector3(padded.x, padded.y, padded.z - slabThickness));
  }

  try {
    return new ConvexGeometry(slab);
  } catch {
    // ConvexGeometry can throw on degenerate point sets
    return null;
  }
}

// ============================================================================
// Component
// ============================================================================

export const ClusterHulls: React.FC<ClusterHullsProps> = ({
  nodes,
  nodePositionsRef,
  nodeIdToIndexMap,
  settings,
}) => {
  const groupRef = useRef<THREE.Group>(null);
  const frameCounter = useRef(0);
  const [tick, setTick] = useState(0);
  const analyticsRef = useRef<Float32Array | null>(null);

  // ---- Early exit if feature is disabled ----
  // Respect both the visual clusterHulls.enabled toggle AND qualityGates.showClusters
  const clusterHullsEnabled = settings?.visualisation?.clusterHulls?.enabled;
  const showClusters = settings?.qualityGates?.showClusters;
  // If qualityGates.showClusters is explicitly false, disable hulls even if clusterHulls.enabled is true
  const enabled = showClusters === false ? false : clusterHullsEnabled;
  const opacity = settings?.visualisation?.clusterHulls?.opacity ?? 0.08;
  const padding = settings?.visualisation?.clusterHulls?.padding ?? 0.15;
  const maxHulls = settings?.visualisation?.clusterHulls?.maxHulls ?? DEFAULT_MAX_HULLS;
  // Z half-thickness of each hull slab. After the disc projection flattens z to
  // ~3%, a cluster's points are near-coplanar; a 3D ConvexGeometry of coplanar
  // points is degenerate. Extruding ±slabThickness gives the hull real volume.
  const slabThickness = settings?.visualisation?.clusterHulls?.slabThickness ?? 35;

  // ---- Increment tick every TICK_INTERVAL frames and refresh analytics ----
  // Pull the per-node analytics buffer (cluster_id at stride 0) from the
  // main-thread analytics store (ADR-03 D7 "Phase 5"). The store is fed from
  // the V3 binary protocol: POST /api/analytics/clustering/run populates
  // server-side node_analytics, whose cluster_id rides every position frame at
  // wire offset 36. With no clustering run the store returns null and
  // getClusterKey falls back to domain heuristics.
  useFrame(() => {
    frameCounter.current += 1;
    if (frameCounter.current >= TICK_INTERVAL) {
      frameCounter.current = 0;
      const buf = nodeAnalyticsStore.getIndexedBuffer(nodeIdToIndexMap);
      if (buf) analyticsRef.current = buf;
      setTick((t) => t + 1);
    }
  });

  // ---- Group nodes into clusters ----
  const clusterMap = useMemo(() => {
    const analytics = analyticsRef.current;
    const map = new Map<string, string[]>();
    for (let ni = 0; ni < nodes.length; ni++) {
      const node = nodes[ni];
      const nodeIndex = nodeIdToIndexMap.get(node.id);
      const key = getClusterKey(node, nodeIndex, analytics);
      if (!key) continue; // Skip unclassified nodes — no hull for them
      let arr = map.get(key);
      if (!arr) {
        arr = [];
        map.set(key, arr);
      }
      arr.push(node.id);
    }
    return map;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [nodes, nodeIdToIndexMap, tick]);

  // ---- Build hull geometries from current positions ----
  const hullEntries = useMemo(() => {
    if (!enabled) return [];

    const positions = nodePositionsRef.current;
    if (!positions) return [];

    const entries: Array<{
      domain: string;
      geometry: ConvexGeometry;
    }> = [];

    // Rank clusters by node count and keep only the N largest, so the dense
    // tail of tiny communities doesn't bury the graph in overlapping hulls.
    const ranked = Array.from(clusterMap.entries())
      .filter(([, nodeIds]) => nodeIds.length >= MIN_CLUSTER_SIZE)
      .sort((a, b) => b[1].length - a[1].length)
      .slice(0, maxHulls);

    for (const [domain, nodeIds] of ranked) {
      const points: THREE.Vector3[] = [];

      for (const id of nodeIds) {
        const idx = nodeIdToIndexMap.get(id);
        if (idx === undefined) continue;

        const base = idx * 3;
        if (base + 2 >= positions.length) continue;

        const x = positions[base];
        const y = positions[base + 1];
        const z = positions[base + 2];

        // Skip zero/NaN positions
        if (Number.isNaN(x) || Number.isNaN(y) || Number.isNaN(z)) continue;

        points.push(new THREE.Vector3(x, y, z));
      }

      const geometry = buildHullGeometry(points, padding, slabThickness);
      if (geometry) {
        entries.push({ domain, geometry });
      }
    }

    return entries;
    // tick drives periodic recomputation
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [enabled, clusterMap, nodeIdToIndexMap, nodePositionsRef, padding, maxHulls, slabThickness, tick]);

  // ---- Cleanup previous geometries when entries change ----
  const prevGeometries = useRef<THREE.BufferGeometry[]>([]);
  useEffect(() => {
    prevGeometries.current = hullEntries.map((e) => e.geometry);
    return () => {
      for (const geo of prevGeometries.current) {
        geo.dispose();
      }
    };
  }, [hullEntries]);

  if (!enabled) return null;

  return (
    <group ref={groupRef} renderOrder={1}>
      {hullEntries.map(({ domain, geometry }) => (
        <mesh
          key={`hull-${domain}`}
          geometry={geometry}
          renderOrder={1}
        >
          <meshBasicMaterial
            color={getDomainHullColor(domain)}
            transparent={true}
            opacity={opacity}
            side={THREE.DoubleSide}
            depthWrite={false}
          />
        </mesh>
      ))}
    </group>
  );
};
