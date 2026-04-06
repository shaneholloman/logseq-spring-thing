import React, { useEffect, useRef, useMemo, useState, useCallback } from 'react';
import * as THREE from 'three';
import { useFrame } from '@react-three/fiber';
import { Html } from '@react-three/drei';
import { useSettingsStore } from '@/store/settingsStore';
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

const EmbeddingCloudLayer: React.FC<EmbeddingCloudProps> = ({ enabled }) => {
  const groupRef = useRef<THREE.Group>(null);
  const pointsRef = useRef<THREE.Points>(null);
  const [data, setData] = useState<EmbeddingCloudData | null>(null);
  const [hovered, setHovered] = useState<{ index: number; point: THREE.Vector3 } | null>(null);

  const settings = useSettingsStore(
    s => s.settings?.visualisation?.embeddingCloud,
  ) as EmbeddingCloudSettings | undefined;

  const pointSize = settings?.pointSize ?? 1.5;
  const opacity = settings?.opacity ?? 0.6;
  const colorBy = settings?.colorBy ?? 'namespace';
  const rotationSpeed = settings?.rotationSpeed ?? 0.0005;
  const maxPoints = settings?.maxPoints ?? 50000;

  // Fetch data once
  useEffect(() => {
    if (!enabled) return;
    fetch(import.meta.env.VITE_EMBEDDING_CLOUD_URL || '/embedding-cloud.json')
      .then(r => { if (!r.ok) throw new Error(r.statusText); return r.json(); })
      .then((d: EmbeddingCloudData) => {
        if (d.count > maxPoints) {
          d.positions = d.positions.slice(0, maxPoints * 3);
          d.metadata = d.metadata.slice(0, maxPoints);
          d.count = maxPoints;
        }
        setData(d);
      })
      .catch(() => { /* silently skip if no data file */ });
  }, [enabled, maxPoints]);

  // Build geometry imperatively to avoid R3F bufferAttribute args issues
  const geometry = useMemo(() => {
    if (!data) return null;
    const geo = new THREE.BufferGeometry();
    geo.setAttribute('position', new THREE.Float32BufferAttribute(new Float32Array(data.positions), 3));
    geo.setAttribute('color', new THREE.Float32BufferAttribute(buildColorAttribute(data, colorBy), 3));
    return geo;
  }, [data, colorBy]);

  // Slow rotation
  useFrame(() => {
    if (groupRef.current && rotationSpeed > 0) {
      groupRef.current.rotation.y += rotationSpeed;
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

  if (!enabled || !data || !geometry) return null;

  return (
    <group ref={groupRef} name="embedding-cloud-layer">
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
