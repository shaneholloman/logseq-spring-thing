import React, { useEffect, useRef, useMemo, useState, useCallback } from 'react';
import * as THREE from 'three';
import { useFrame } from '@react-three/fiber';
import { Html } from '@react-three/drei';
import { useSettingsStore } from '@/store/settingsStore';
import { useWebSocketStore } from '@/store/websocketStore';
import type { EmbeddingCloudSettings } from '../../settings/config/settings';

interface EmbeddingMeta {
  key: string;
  namespace: string;
  sourceType: string;
}

interface EmbeddingCloudData {
  count: number;
  positions: number[];
  metadata: EmbeddingMeta[];
  namespaces: string[];
  sourceTypes: string[];
}

interface EmbeddingCloudProps {
  enabled: boolean;
}

interface MemoryFlashEvent {
  key: string;
  namespace: string;
  action: string;
  timestamp: number;
}

// ── Flash burst config ──
const BURST_DURATION = 1.8;       // seconds
const BURST_MAX_SCALE = 4.0;      // world-units radius at peak
const BURST_POOL_SIZE = 64;       // max concurrent bursts
const BURST_COLORS: number[] = [
  0x00ffff, 0x40e0d0, 0x7df9ff, 0x00fff7, // cyan family
  0xff6ec7, 0xff44cc, 0xda70d6,            // magenta/pink
  0x7fff00, 0x39ff14,                       // electric green
  0xffaa00, 0xff6600,                       // amber/orange
];

// Distinct palette for categorical coloring (up to 16 categories, then cycles)
const PALETTE = [
  0x4fc3f7, 0xaa96da, 0x81c784, 0xffb74d, 0xef5350,
  0x26c6da, 0xfff176, 0xce93d8, 0xa1887f, 0x90a4ae,
  0x4db6ac, 0xf06292, 0xaed581, 0x7986cb, 0xffcc80, 0xe0e0e0,
];

function buildColorAttribute(
  data: EmbeddingCloudData,
  colorBy: 'namespace' | 'sourceType',
): Float32Array {
  const categories = colorBy === 'namespace' ? data.namespaces : data.sourceTypes;
  const catMap = new Map<string, number>();
  categories.forEach((c, i) => catMap.set(c, i));

  const colors = new Float32Array(data.count * 3);
  const tmpColor = new THREE.Color();

  for (let i = 0; i < data.count; i++) {
    const meta = data.metadata[i];
    const cat = colorBy === 'namespace' ? meta.namespace : meta.sourceType;
    const idx = catMap.get(cat) ?? 0;
    tmpColor.set(PALETTE[idx % PALETTE.length]);
    colors[i * 3] = tmpColor.r;
    colors[i * 3 + 1] = tmpColor.g;
    colors[i * 3 + 2] = tmpColor.b;
  }
  return colors;
}

// ── Burst ring geometry (shared) ──
function createBurstRingGeometry(): THREE.RingGeometry {
  return new THREE.RingGeometry(0.8, 1.0, 32);
}

// ── Burst pool entry ──
interface BurstSlot {
  mesh: THREE.Mesh;
  startTime: number;
  active: boolean;
}

const EmbeddingCloudLayer: React.FC<EmbeddingCloudProps> = ({ enabled }) => {
  const groupRef = useRef<THREE.Group>(null);
  const pointsRef = useRef<THREE.Points>(null);
  const [data, setData] = useState<EmbeddingCloudData | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [hovered, setHovered] = useState<{ index: number; point: THREE.Vector3 } | null>(null);

  // Lookup maps for flash targeting
  const keyIndexMap = useRef<Map<string, number[]>>(new Map());
  const nsIndexMap = useRef<Map<string, number[]>>(new Map());

  // Burst effect pool
  const burstPool = useRef<BurstSlot[]>([]);
  const burstGroupRef = useRef<THREE.Group>(null);
  const burstNextSlot = useRef(0);
  const burstRingGeo = useRef<THREE.RingGeometry | null>(null);

  const settings = useSettingsStore(
    s => s.settings?.visualisation?.embeddingCloud,
  ) as EmbeddingCloudSettings | undefined;

  const pointSize = settings?.pointSize ?? 7.5;
  const opacity = settings?.opacity ?? 0.6;
  const colorBy = settings?.colorBy ?? 'namespace';
  const rotationSpeed = settings?.rotationSpeed ?? 0.0005;
  const maxPoints = settings?.maxPoints ?? 50000;
  const cloudScale = settings?.cloudScale ?? 5.0;

  // Fetch data lazily -- only when the embedding cloud feature is enabled.
  // The JSON file (~8MB) is never bundled; it lives in public/ and is fetched on demand.
  useEffect(() => {
    if (!enabled) return;
    // Avoid re-fetching if we already have data
    if (data) return;

    const controller = new AbortController();
    setLoading(true);
    setError(null);

    fetch(import.meta.env.VITE_EMBEDDING_CLOUD_URL || '/embedding-cloud.json', {
      signal: controller.signal,
    })
      .then(r => { if (!r.ok) throw new Error(r.statusText); return r.json(); })
      .then((d: EmbeddingCloudData) => {
        if (d.count > maxPoints) {
          d.positions = d.positions.slice(0, maxPoints * 3);
          d.metadata = d.metadata.slice(0, maxPoints);
          d.count = maxPoints;
        }
        const kMap = new Map<string, number[]>();
        const nMap = new Map<string, number[]>();
        for (let i = 0; i < d.count; i++) {
          const m = d.metadata[i];
          if (m.key) {
            const arr = kMap.get(m.key);
            if (arr) arr.push(i); else kMap.set(m.key, [i]);
          }
          if (m.namespace) {
            const arr = nMap.get(m.namespace);
            if (arr) arr.push(i); else nMap.set(m.namespace, [i]);
          }
        }
        keyIndexMap.current = kMap;
        nsIndexMap.current = nMap;
        setData(d);
        setLoading(false);
      })
      .catch((err) => {
        if (err.name !== 'AbortError') {
          setError('Failed to load embedding cloud data');
          setLoading(false);
        }
      });

    return () => { controller.abort(); };
  }, [enabled, maxPoints, data]);

  // Build point cloud geometry
  const geometry = useMemo(() => {
    if (!data) return null;
    const geo = new THREE.BufferGeometry();
    geo.setAttribute('position', new THREE.Float32BufferAttribute(new Float32Array(data.positions), 3));
    geo.setAttribute('color', new THREE.Float32BufferAttribute(buildColorAttribute(data, colorBy), 3));
    return geo;
  }, [data, colorBy]);

  // Initialize burst pool (ring meshes that expand + fade)
  useEffect(() => {
    if (!enabled || !data) return;
    if (!burstRingGeo.current) {
      burstRingGeo.current = createBurstRingGeometry();
    }
    const geo = burstRingGeo.current;
    const pool: BurstSlot[] = [];
    for (let i = 0; i < BURST_POOL_SIZE; i++) {
      const mat = new THREE.MeshBasicMaterial({
        color: BURST_COLORS[i % BURST_COLORS.length],
        transparent: true,
        opacity: 0,
        side: THREE.DoubleSide,
        depthWrite: false,
        blending: THREE.AdditiveBlending,
      });
      const mesh = new THREE.Mesh(geo, mat);
      mesh.visible = false;
      mesh.renderOrder = 999;
      // Face camera by default (billboard in useFrame)
      pool.push({ mesh, startTime: 0, active: false });
    }
    burstPool.current = pool;

    // Add meshes to burst group
    if (burstGroupRef.current) {
      // Clear previous
      while (burstGroupRef.current.children.length > 0) {
        burstGroupRef.current.remove(burstGroupRef.current.children[0]);
      }
      pool.forEach(s => burstGroupRef.current!.add(s.mesh));
    }

    return () => {
      pool.forEach(s => {
        (s.mesh.material as THREE.MeshBasicMaterial).dispose();
      });
      geo.dispose();
      burstRingGeo.current = null;
    };
  }, [enabled, data]);

  // Spawn a burst at a world position
  const spawnBurst = useCallback((worldPos: THREE.Vector3) => {
    const pool = burstPool.current;
    if (pool.length === 0) return;
    const slot = pool[burstNextSlot.current % pool.length];
    burstNextSlot.current++;

    slot.mesh.position.copy(worldPos);
    slot.mesh.scale.setScalar(0.01);
    slot.mesh.visible = true;
    const mat = slot.mesh.material as THREE.MeshBasicMaterial;
    mat.opacity = 1.0;
    mat.color.set(BURST_COLORS[Math.floor(Math.random() * BURST_COLORS.length)]);
    slot.startTime = performance.now();
    slot.active = true;
  }, []);

  // Subscribe to memory_flash WebSocket events
  useEffect(() => {
    if (!enabled || !data) return;

    const positions = data.positions;

    const unsubscribe = useWebSocketStore.getState().on('memoryFlash', (raw: unknown) => {
      const event = raw as MemoryFlashEvent;
      if (!event?.key) return;

      // Find point indices for this key
      let indices = keyIndexMap.current.get(event.key);

      // Fallback: pick random points from namespace
      if (!indices || indices.length === 0) {
        if (event.namespace) {
          const nsIndices = nsIndexMap.current.get(event.namespace);
          if (nsIndices && nsIndices.length > 0) {
            // Pick 1-3 random from namespace
            const picks = Math.min(3, nsIndices.length);
            indices = [];
            for (let i = 0; i < picks; i++) {
              indices.push(nsIndices[Math.floor(Math.random() * nsIndices.length)]);
            }
          }
        }
      }

      // Final fallback: random position in the cloud
      if (!indices || indices.length === 0) {
        const count = data.count;
        if (count > 0) {
          indices = [Math.floor(Math.random() * count)];
        } else {
          return;
        }
      }

      // Spawn burst rings at each matched position
      for (const idx of indices) {
        const px = positions[idx * 3];
        const py = positions[idx * 3 + 1];
        const pz = positions[idx * 3 + 2];
        spawnBurst(new THREE.Vector3(px, py, pz));
      }
    });

    return unsubscribe;
  }, [enabled, data, spawnBurst]);

  // Per-frame: rotation + burst animation
  useFrame(({ camera }) => {
    if (groupRef.current && rotationSpeed > 0) {
      groupRef.current.rotation.y += rotationSpeed;
    }

    // Animate burst rings: expand + fade + billboard
    const now = performance.now();
    for (const slot of burstPool.current) {
      if (!slot.active) continue;
      const elapsed = (now - slot.startTime) / 1000;
      if (elapsed >= BURST_DURATION) {
        slot.active = false;
        slot.mesh.visible = false;
        continue;
      }
      const t = elapsed / BURST_DURATION;
      // Fast expand, slow fade
      const scale = BURST_MAX_SCALE * (1 - Math.pow(1 - t, 3)); // ease-out cubic
      const alpha = 1.0 - t * t; // quadratic fade
      slot.mesh.scale.setScalar(scale);
      (slot.mesh.material as THREE.MeshBasicMaterial).opacity = alpha * 0.85;
      // Billboard: face camera
      slot.mesh.quaternion.copy(camera.quaternion);
    }
  });

  // Raycaster hover handler
  const onPointerMove = useCallback(
    (e: THREE.Event & { index?: number; point?: THREE.Vector3 }) => {
      if (e.index != null && e.point && data) {
        setHovered({ index: e.index, point: e.point.clone() });
      }
    },
    [data],
  );

  const onPointerOut = useCallback(() => setHovered(null), []);

  if (!enabled) return null;

  // Show loading indicator while fetching the ~8MB embedding data
  if (loading) {
    return (
      <Html center>
        <div style={{
          background: 'rgba(0,0,0,0.7)',
          color: '#7df9ff',
          padding: '8px 16px',
          borderRadius: 6,
          fontSize: 12,
          fontFamily: 'monospace',
        }}>
          Loading embedding cloud...
        </div>
      </Html>
    );
  }

  if (error) {
    return (
      <Html center>
        <div style={{
          background: 'rgba(80,0,0,0.7)',
          color: '#ff6666',
          padding: '8px 16px',
          borderRadius: 6,
          fontSize: 12,
          fontFamily: 'monospace',
        }}>
          {error}
        </div>
      </Html>
    );
  }

  if (!data || !geometry) return null;

  return (
    <group ref={groupRef} name="embedding-cloud-layer" scale={cloudScale}>
      <points
        ref={pointsRef}
        geometry={geometry}
        onPointerMove={onPointerMove}
        onPointerOut={onPointerOut}
      >
        <pointsMaterial
          size={pointSize}
          opacity={opacity}
          transparent
          vertexColors
          sizeAttenuation
          depthWrite={false}
          blending={THREE.AdditiveBlending}
        />
      </points>
      {/* Burst ring pool — lives inside the cloud group so it scales/rotates with it */}
      <group ref={burstGroupRef} />
      {hovered && data.metadata[hovered.index] && (
        <Html position={hovered.point} center style={{ pointerEvents: 'none' }}>
          <div
            style={{
              background: 'rgba(0,0,0,0.85)',
              color: '#fff',
              padding: '4px 8px',
              borderRadius: 4,
              fontSize: 11,
              whiteSpace: 'nowrap',
              maxWidth: 300,
              overflow: 'hidden',
              textOverflow: 'ellipsis',
            }}
          >
            <div><b>{data.metadata[hovered.index].key}</b></div>
            <div style={{ opacity: 0.7 }}>
              {data.metadata[hovered.index].namespace} / {data.metadata[hovered.index].sourceType}
            </div>
          </div>
        </Html>
      )}
    </group>
  );
};

export default EmbeddingCloudLayer;
