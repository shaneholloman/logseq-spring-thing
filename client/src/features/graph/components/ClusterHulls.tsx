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

// Spatial-clustering target bucket count when no server cluster_id exists.
// Over-provisioned vs maxHulls so the N largest occupied cells are dense and
// well-separated; the long tail of sparse cells is dropped by MIN_CLUSTER_SIZE.
const SPATIAL_TARGET_CELLS = 96;

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

interface ClusterPoint {
  id: string;
  x: number;
  y: number;
  z: number;
}

/**
 * Spatial grid clustering over the live node positions.
 *
 * When no server-side cluster_id is available, semantic-domain grouping is
 * useless for hulls: a domain (e.g. "AI") is a global tag whose members are
 * scattered across the entire disc, so its convex hull blankets the whole
 * population. The force-directed layout, however, already places densely
 * connected nodes near each other — spatial proximity IS the community
 * structure the user sees. Bucketing positions into a grid yields disjoint,
 * tight, non-overlapping clusters by construction.
 *
 * Grid resolution per axis is proportional to that axis's extent so an
 * anisotropic population (the disc spans z far wider than x/y) is subdivided
 * evenly in world space rather than by index, targeting ~targetCells buckets.
 */
function buildSpatialClusters(
  pts: ClusterPoint[],
  targetCells: number,
): Map<string, string[]> {
  const map = new Map<string, string[]>();
  if (pts.length === 0) return map;

  let minX = Infinity, minY = Infinity, minZ = Infinity;
  let maxX = -Infinity, maxY = -Infinity, maxZ = -Infinity;
  for (const p of pts) {
    if (p.x < minX) minX = p.x; if (p.x > maxX) maxX = p.x;
    if (p.y < minY) minY = p.y; if (p.y > maxY) maxY = p.y;
    if (p.z < minZ) minZ = p.z; if (p.z > maxZ) maxZ = p.z;
  }
  const ex = Math.max(maxX - minX, 1e-3);
  const ey = Math.max(maxY - minY, 1e-3);
  const ez = Math.max(maxZ - minZ, 1e-3);

  // Choose k so nx*ny*nz ≈ targetCells, with each nAxis ∝ axis extent.
  const k = Math.cbrt(targetCells / (ex * ey * ez));
  const nx = Math.max(1, Math.round(k * ex));
  const ny = Math.max(1, Math.round(k * ey));
  const nz = Math.max(1, Math.round(k * ez));
  const cellX = ex / nx;
  const cellY = ey / ny;
  const cellZ = ez / nz;

  for (const p of pts) {
    const ix = Math.min(nx - 1, Math.floor((p.x - minX) / cellX));
    const iy = Math.min(ny - 1, Math.floor((p.y - minY) / cellY));
    const iz = Math.min(nz - 1, Math.floor((p.z - minZ) / cellZ));
    const key = `sp-${ix}-${iy}-${iz}`;
    let arr = map.get(key);
    if (!arr) { arr = []; map.set(key, arr); }
    arr.push(p.id);
  }
  return map;
}

/**
 * Given a set of 3D points, compute padded points offset from centroid,
 * then return a ConvexGeometry. Returns null if fewer than MIN_CLUSTER_SIZE
 * valid points or the set stays degenerate.
 */
function buildHullGeometry(
  points: THREE.Vector3[],
  padding: number,
  slabThickness: number,
): ConvexGeometry | null {
  if (points.length < MIN_CLUSTER_SIZE) return null;

  // Centroid + radial padding (push points out so the hull breathes past nodes).
  const centroid = new THREE.Vector3();
  for (const p of points) centroid.add(p);
  centroid.divideScalar(points.length);

  const padded: THREE.Vector3[] = [];
  let minX = Infinity, minY = Infinity, minZ = Infinity;
  let maxX = -Infinity, maxY = -Infinity, maxZ = -Infinity;
  for (const p of points) {
    const q = new THREE.Vector3()
      .subVectors(p, centroid)
      .multiplyScalar(1 + padding)
      .add(centroid);
    padded.push(q);
    if (q.x < minX) minX = q.x; if (q.x > maxX) maxX = q.x;
    if (q.y < minY) minY = q.y; if (q.y > maxY) maxY = q.y;
    if (q.z < minZ) minZ = q.z; if (q.z > maxZ) maxZ = q.z;
  }

  // ConvexHull of a near-coplanar / near-collinear set is degenerate (throws or
  // yields a zero-volume sliver). Detect the THINNEST axis and, only if it's
  // flat relative to the other two, extrude along it by slabThickness. This
  // generalises the old z-only slab — the co-planar disc projection can leave
  // any axis as the degenerate one depending on view, not just z.
  const exts: Array<['x' | 'y' | 'z', number]> = [
    ['x', maxX - minX],
    ['y', maxY - minY],
    ['z', maxZ - minZ],
  ];
  exts.sort((a, b) => a[1] - b[1]);
  const [thinAxis, thinExt] = exts[0];
  const fatExt = exts[2][1];

  let hullPts = padded;
  if (thinExt < 0.05 * fatExt) {
    hullPts = [];
    for (const q of padded) {
      const a = q.clone();
      const b = q.clone();
      a[thinAxis] += slabThickness;
      b[thinAxis] -= slabThickness;
      hullPts.push(a, b);
    }
  }

  try {
    return new ConvexGeometry(hullPts);
  } catch {
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
  // Two grouping sources, in priority order:
  //   1. Server cluster_id (Louvain) — communities that are spatially coherent
  //      under the force layout, so their hulls are tight. Used when present.
  //   2. Spatial grid over live positions — the fallback when no clustering run
  //      exists. Semantic-domain tags are spatially scattered (their hulls
  //      blanket the whole disc = mush), so we cluster by the proximity the
  //      layout already produced. These carry no semantic key, so hullEntries
  //      colours them by rank for visual separation.
  const clusterMap = useMemo(() => {
    const analytics = analyticsRef.current;
    const positions = nodePositionsRef.current;

    // Detect whether any real cluster_id is present (stride 3, index 0 > 0).
    let hasClusterId = false;
    if (analytics) {
      for (const idx of nodeIdToIndexMap.values()) {
        if (analytics[idx * 3] > 0) { hasClusterId = true; break; }
      }
    }

    const map = new Map<string, string[]>();
    const colorByKey = new Map<string, string>();

    if (hasClusterId) {
      for (let ni = 0; ni < nodes.length; ni++) {
        const node = nodes[ni];
        const nodeIndex = nodeIdToIndexMap.get(node.id);
        const key = getClusterKey(node, nodeIndex, analytics);
        if (!key) continue;
        let arr = map.get(key);
        if (!arr) { arr = []; map.set(key, arr); colorByKey.set(key, getDomainHullColor(key)); }
        arr.push(node.id);
      }
      return { map, colorByKey };
    }

    // Spatial fallback: bucket live positions into a proximity grid.
    if (!positions) return { map, colorByKey };
    const pts: ClusterPoint[] = [];
    for (let ni = 0; ni < nodes.length; ni++) {
      const node = nodes[ni];
      const idx = nodeIdToIndexMap.get(node.id);
      if (idx === undefined) continue;
      const base = idx * 3;
      if (base + 2 >= positions.length) continue;
      const x = positions[base], y = positions[base + 1], z = positions[base + 2];
      if (Number.isNaN(x) || Number.isNaN(y) || Number.isNaN(z)) continue;
      pts.push({ id: node.id, x, y, z });
    }
    // Spatial clusters carry no semantic key; hullEntries colours them by rank.
    return { map: buildSpatialClusters(pts, SPATIAL_TARGET_CELLS), colorByKey };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [nodes, nodeIdToIndexMap, nodePositionsRef, tick]);

  // ---- Build hull geometries from current positions ----
  const hullEntries = useMemo(() => {
    if (!enabled) return [];

    const positions = nodePositionsRef.current;
    if (!positions) return [];

    const { map, colorByKey } = clusterMap;
    const entries: Array<{
      key: string;
      color: string;
      geometry: ConvexGeometry;
    }> = [];

    // Rank clusters by node count and keep only the N largest, so the dense
    // tail of tiny communities doesn't bury the graph in overlapping hulls.
    const ranked = Array.from(map.entries())
      .filter(([, nodeIds]) => nodeIds.length >= MIN_CLUSTER_SIZE)
      .sort((a, b) => b[1].length - a[1].length)
      .slice(0, maxHulls);

    let rank = 0;
    for (const [key, nodeIds] of ranked) {
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
        // Spatial clusters carry no semantic identity, so the dominant-domain
        // colour collapses to one hue when the population is single-domain —
        // 32 distinct regions then read as one mass. Colour spatial hulls by
        // rank from the palette so neighbouring regions stay visually separable.
        // Real cluster_id / domain hulls keep their semantic colour.
        const color = key.startsWith('sp-')
          ? GPU_CLUSTER_COLORS[rank % GPU_CLUSTER_COLORS.length]
          : (colorByKey.get(key) ?? DEFAULT_COLOR);
        entries.push({ key, color, geometry });
      }
      rank++;
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
      {hullEntries.map(({ key, color, geometry }) => (
        <mesh
          key={`hull-${key}`}
          geometry={geometry}
          renderOrder={1}
        >
          <meshBasicMaterial
            color={color}
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
