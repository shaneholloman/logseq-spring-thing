import React, { useRef, useMemo, useEffect, useState } from 'react';
import { useFrame } from '@react-three/fiber';
import * as THREE from 'three';
import { ConvexGeometry } from 'three/examples/jsm/geometries/ConvexGeometry.js';
import { graphWorkerProxy } from '../managers/graphWorkerProxy';

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

  // 3. Extract domain from label prefix (e.g., "BC-0034-nonce" → "BC",
  //    "AI-0411-Privacy..." → "AI", "mv:Avatar" → "MV")
  const label = node.label ?? node.metadata?.label ?? '';
  const prefixMatch = label.match(/^(BC|AI|MV|RB|TC|DT|NGM|bc|ai|mv|rb|tc|dt|ngm)[-:]/i);
  if (prefixMatch) return prefixMatch[1].toUpperCase();

  // 4. Detect domain from common keywords in label
  if (label.match(/Blockchain|Crypto|Token|DeFi|Smart Contract|Bitcoin|Ethereum|Consensus/i)) return 'BC';
  if (label.match(/Artificial Intelligence|Machine Learning|Neural|NLP|Deep Learning|Agent(?!$)/i)) return 'AI';
  if (label.match(/Metaverse|VR|AR|XR|Avatar|Render|Digital Twin|Hologram/i)) return 'MV';
  if (label.match(/Robot|Drone|Sensor|IoT|Actuator/i)) return 'RB';
  if (label.match(/Security|Privacy|Auth|Encrypt|Access Control|Firewall/i)) return 'SEC';
  if (label.match(/Network|Protocol|API|Infrastructure|Server|Cloud/i)) return 'INFRA';

  return 'unknown';
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
): ConvexGeometry | null {
  if (points.length < MIN_CLUSTER_SIZE) return null;

  // Compute centroid
  const centroid = new THREE.Vector3();
  for (const p of points) {
    centroid.add(p);
  }
  centroid.divideScalar(points.length);

  // Offset each point away from centroid by (1 + padding)
  const paddedPoints = points.map((p) => {
    const dir = new THREE.Vector3().subVectors(p, centroid);
    return centroid.clone().add(dir.multiplyScalar(1 + padding));
  });

  try {
    return new ConvexGeometry(paddedPoints);
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

  // ---- Increment tick every TICK_INTERVAL frames and refresh analytics ----
  useFrame(() => {
    frameCounter.current += 1;
    if (frameCounter.current >= TICK_INTERVAL) {
      frameCounter.current = 0;
      // Refresh analytics buffer from worker
      graphWorkerProxy.getAnalyticsBuffer().then(buf => {
        analyticsRef.current = buf.length > 0 ? buf : null;
      }).catch(() => { /* ignore worker errors */ });
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

    clusterMap.forEach((nodeIds, domain) => {
      if (nodeIds.length < MIN_CLUSTER_SIZE) return;

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

      const geometry = buildHullGeometry(points, padding);
      if (geometry) {
        entries.push({ domain, geometry });
      }
    });

    return entries;
    // tick drives periodic recomputation
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [enabled, clusterMap, nodeIdToIndexMap, nodePositionsRef, padding, tick]);

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
