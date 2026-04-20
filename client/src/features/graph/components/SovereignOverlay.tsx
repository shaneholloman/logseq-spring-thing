/**
 * SovereignOverlay (ADR-048 / ADR-049, Sprint 3).
 *
 * A thin three.js layer rendered alongside the main GraphManager output that
 * draws three new surfaces without touching the hot-path rendering code:
 *
 *   1. Bold filaments for freshly-promoted BRIDGE_TO edges (amber -> cyan pulse).
 *   2. Dim overlay for private nodes (grey sphere, 60% opacity, no label).
 *   3. Red "X" tombstone markers for recently unpublished nodes, 5s fade.
 *
 * Positions come from the graph's node position ref; we look up via node id.
 * The overlay listens to the zustand slices + the `visionflow:migration-focus`
 * CustomEvent (dispatched from `MigrationEventToast`) to schedule camera pans.
 */

import React, { useEffect, useMemo, useRef } from 'react';
import { useFrame, useThree } from '@react-three/fiber';
import * as THREE from 'three';
import { useMigrationEventsStore } from '../store/migrationEventsSlice';
import {
  useVisibilityStore,
  TOMBSTONE_TTL_MS,
} from '../store/visibilitySlice';
import { useFeatureFlag } from '../../../services/featureFlags';
import type { BridgePromotionEvent, KGNode } from '../types/graphTypes';

const PULSE_DURATION_MS = 4_500;
const FILAMENT_WIDTH = 2.5;
const PRIVATE_SCALE = 1.15;
const TOMBSTONE_SIZE = 4;

interface FocusDetail {
  from_kg: string;
  to_owl: string;
  edge_id?: string;
  event_id: string;
}

function lerpColor(from: THREE.Color, to: THREE.Color, t: number): THREE.Color {
  const out = new THREE.Color();
  out.r = from.r + (to.r - from.r) * t;
  out.g = from.g + (to.g - from.g) * t;
  out.b = from.b + (to.b - from.b) * t;
  return out;
}

/**
 * Positions accepted by the overlay. GraphManager keeps positions in a flat
 * Float32Array (`[x,y,z,x,y,z,...]`) indexed via `nodeIdToIndexMap`, but the
 * public contract accepts the alternative Map/array shapes for future use.
 */
export type PositionStore =
  | Float32Array
  | null
  | Map<string, THREE.Vector3>
  | THREE.Vector3[];

export interface SovereignOverlayProps {
  nodes: KGNode[];
  nodePositionsRef: React.MutableRefObject<PositionStore>;
  nodeIdToIndexMap?: Map<string, number>;
}

export function SovereignOverlay({
  nodes,
  nodePositionsRef,
  nodeIdToIndexMap,
}: SovereignOverlayProps): React.ReactElement | null {
  const bridgeEnabled = useFeatureFlag('BRIDGE_EDGE_ENABLED');
  const visibilityEnabled = useFeatureFlag('VISIBILITY_TRANSITIONS');
  const migrationEvents = useMigrationEventsStore((s) => s.events);
  const overlay = useVisibilityStore((s) => s.overlay);
  const gcTombstones = useVisibilityStore((s) => s.gcTombstones);
  const { camera } = useThree();

  // Index node lookup for O(1) position resolution.
  const nodeById = useMemo(() => {
    const map = new Map<string, KGNode>();
    for (const n of nodes) map.set(n.id, n);
    return map;
  }, [nodes]);

  // Active filament pulses (keyed by event id).
  const pulseStartsRef = useRef<Map<string, number>>(new Map());

  useEffect(() => {
    if (!bridgeEnabled) return;
    const now = performance.now();
    for (const event of migrationEvents) {
      if (!pulseStartsRef.current.has(event.id)) {
        pulseStartsRef.current.set(event.id, now);
      }
    }
  }, [bridgeEnabled, migrationEvents]);

  // Camera pan on migration focus event (dispatched by MigrationEventToast).
  useEffect(() => {
    if (!bridgeEnabled) return;
    const handler = (e: Event): void => {
      const detail = (e as CustomEvent<FocusDetail>).detail;
      if (!detail) return;
      const pos = resolvePosition(detail.from_kg, nodePositionsRef, nodeIdToIndexMap);
      if (!pos) return;
      // Gentle pan - do not fight any live camera control.
      const target = pos.clone();
      const offset = new THREE.Vector3(0, 0, 40);
      camera.position.lerp(target.clone().add(offset), 0.35);
      camera.lookAt(target);
    };
    window.addEventListener('visionflow:migration-focus', handler);
    return () => window.removeEventListener('visionflow:migration-focus', handler);
  }, [bridgeEnabled, camera, nodePositionsRef, nodeIdToIndexMap]);

  // Tombstone GC loop.
  useEffect(() => {
    if (!visibilityEnabled) return;
    const handle = window.setInterval(gcTombstones, 1_000);
    return () => window.clearInterval(handle);
  }, [visibilityEnabled, gcTombstones]);

  // Refs to the instanced groups we update per frame.
  const filamentGroupRef = useRef<THREE.Group>(null);
  const privateGroupRef = useRef<THREE.Group>(null);
  const tombstoneGroupRef = useRef<THREE.Group>(null);

  // Static materials (recoloured per frame where needed).
  const amber = useMemo(() => new THREE.Color('#ffb347'), []);
  const cyan = useMemo(() => new THREE.Color('#35e0ff'), []);
  const privateColor = useMemo(() => new THREE.Color('#808a99'), []);
  const tombstoneColor = useMemo(() => new THREE.Color('#ff3b5c'), []);

  useFrame(() => {
    // --- Filaments ---
    if (bridgeEnabled && filamentGroupRef.current) {
      const now = performance.now();
      const group = filamentGroupRef.current;
      // Cheap "rebuild-per-frame" policy - amount of active filaments is tiny
      // (<= MIGRATION_EVENT_BUFFER_SIZE) so the alloc cost is negligible and
      // dropping children keeps this safe if IDs change mid-frame.
      group.clear();

      for (const event of migrationEvents) {
        const started = pulseStartsRef.current.get(event.id);
        if (!started) continue;
        const age = now - started;
        if (age > PULSE_DURATION_MS) {
          pulseStartsRef.current.delete(event.id);
          continue;
        }
        buildFilament(group, event, age, nodePositionsRef, nodeIdToIndexMap, amber, cyan);
      }
    }

    // --- Private opaque nodes ---
    if (visibilityEnabled && privateGroupRef.current) {
      const group = privateGroupRef.current;
      group.clear();
      for (const [nodeId, record] of Object.entries(overlay)) {
        if (record.visibility !== 'private') continue;
        const node = nodeById.get(nodeId);
        const pos = resolvePosition(nodeId, nodePositionsRef, nodeIdToIndexMap, node);
        if (!pos) continue;
        const mesh = new THREE.Mesh(
          new THREE.SphereGeometry(PRIVATE_SCALE, 12, 12),
          new THREE.MeshBasicMaterial({
            color: privateColor,
            transparent: true,
            opacity: 0.6,
            depthWrite: false,
          }),
        );
        mesh.position.copy(pos);
        group.add(mesh);
      }
    }

    // --- Tombstones ---
    if (visibilityEnabled && tombstoneGroupRef.current) {
      const now = Date.now();
      const group = tombstoneGroupRef.current;
      group.clear();
      for (const [nodeId, record] of Object.entries(overlay)) {
        if (record.visibility !== 'tombstone') continue;
        const node = nodeById.get(nodeId);
        const pos = resolvePosition(nodeId, nodePositionsRef, nodeIdToIndexMap, node);
        if (!pos) continue;
        const expires = record.expiresAt ?? now + TOMBSTONE_TTL_MS;
        const remaining = Math.max(0, expires - now);
        const t = remaining / TOMBSTONE_TTL_MS;
        group.add(buildTombstone(pos, tombstoneColor, t));
      }
    }
  });

  if (!bridgeEnabled && !visibilityEnabled) return null;

  return (
    <group name="sovereign-overlay">
      {bridgeEnabled && <group ref={filamentGroupRef} name="sovereign-filaments" />}
      {visibilityEnabled && (
        <group ref={privateGroupRef} name="sovereign-private-nodes" />
      )}
      {visibilityEnabled && (
        <group ref={tombstoneGroupRef} name="sovereign-tombstones" />
      )}
    </group>
  );
}

// ---------------------------------------------------------------------------

function resolvePosition(
  nodeId: string,
  ref: SovereignOverlayProps['nodePositionsRef'],
  idMap: Map<string, number> | undefined,
  fallbackNode?: KGNode,
): THREE.Vector3 | null {
  const store = ref.current;
  if (store) {
    if (store instanceof Float32Array) {
      const idx = idMap?.get(nodeId);
      if (idx !== undefined && idx >= 0 && idx * 3 + 2 < store.length) {
        return new THREE.Vector3(store[idx * 3], store[idx * 3 + 1], store[idx * 3 + 2]);
      }
    } else if (store instanceof Map) {
      const v = store.get(nodeId);
      if (v) return v;
    } else if (Array.isArray(store)) {
      const idx = idMap?.get(nodeId);
      if (idx !== undefined && store[idx]) return store[idx];
    }
  }
  if (fallbackNode?.position) {
    return new THREE.Vector3(
      fallbackNode.position.x,
      fallbackNode.position.y,
      fallbackNode.position.z,
    );
  }
  return null;
}

function buildFilament(
  group: THREE.Group,
  event: BridgePromotionEvent,
  ageMs: number,
  ref: SovereignOverlayProps['nodePositionsRef'],
  idMap: Map<string, number> | undefined,
  amber: THREE.Color,
  cyan: THREE.Color,
): void {
  const from = resolvePosition(event.from_kg, ref, idMap);
  const to = resolvePosition(event.to_owl, ref, idMap);
  if (!from || !to) return;
  const t = Math.min(1, ageMs / PULSE_DURATION_MS);
  const color = lerpColor(amber, cyan, t);
  const opacity = 0.9 * (1 - t) + 0.3;
  const width = FILAMENT_WIDTH * (1 + 0.6 * Math.sin((ageMs / PULSE_DURATION_MS) * Math.PI));
  const geometry = new THREE.BufferGeometry().setFromPoints([from, to]);
  const material = new THREE.LineBasicMaterial({
    color,
    transparent: true,
    opacity,
    linewidth: width, // Honoured on WebGPU / some WebGL stacks.
    depthWrite: false,
  });
  group.add(new THREE.Line(geometry, material));
}

function buildTombstone(
  position: THREE.Vector3,
  color: THREE.Color,
  lifeFraction: number,
): THREE.Group {
  const g = new THREE.Group();
  g.position.copy(position);
  const opacity = Math.max(0.1, lifeFraction);
  const makeBar = (rotZ: number): THREE.Mesh => {
    const geom = new THREE.BoxGeometry(TOMBSTONE_SIZE, TOMBSTONE_SIZE * 0.18, 0.2);
    const mat = new THREE.MeshBasicMaterial({
      color,
      transparent: true,
      opacity,
      depthWrite: false,
    });
    const mesh = new THREE.Mesh(geom, mat);
    mesh.rotation.z = rotZ;
    return mesh;
  };
  g.add(makeBar(Math.PI / 4));
  g.add(makeBar(-Math.PI / 4));
  return g;
}

export default SovereignOverlay;
