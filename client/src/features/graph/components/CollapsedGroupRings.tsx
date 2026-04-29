/**
 * CollapsedGroupRings
 *
 * Renders a pulsing double-ring halo around ontology parent nodes that are
 * currently collapsed (i.e. their children are hidden). Gives the user a
 * clear visual signal that double-clicking will expand the group.
 *
 * Uses R3F instanced mesh — one draw call for all collapsed parents.
 */

import React, { useRef, useMemo } from 'react';
import { useFrame } from '@react-three/fiber';
import * as THREE from 'three';
import type { Node as KGNode } from '../managers/graphDataManager';
import { getDescendants, type HierarchyNode } from '../utils/hierarchyDetector';
import type { ExpansionState } from '../hooks/useExpansionState';

interface CollapsedGroupRingsProps {
  nodes: KGNode[];
  hierarchyMap: Map<string, HierarchyNode>;
  expansionState: ExpansionState;
  nodePositionsRef: React.MutableRefObject<Float32Array | null>;
  nodeIdToIndexMap: Map<string, number>;
}

const RING_COLOR = new THREE.Color('#5DADE2');
const RING_INNER = 0.08;
const RING_OUTER = 0.12;
const RING_SEGMENTS = 48;
const PULSE_SPEED = 1.2;
const PULSE_AMPLITUDE = 0.18;
const BASE_SCALE = 2.2;

const _mat = new THREE.Matrix4();
const _pos = new THREE.Vector3();
const _scale = new THREE.Vector3();
const _quat = new THREE.Quaternion();

export const CollapsedGroupRings: React.FC<CollapsedGroupRingsProps> = ({
  nodes,
  hierarchyMap,
  expansionState,
  nodePositionsRef,
  nodeIdToIndexMap,
}) => {
  const outerRef = useRef<THREE.InstancedMesh>(null);
  const innerRef = useRef<THREE.InstancedMesh>(null);

  // Only parent nodes that are currently collapsed.
  //
  // We also precompute total *transitive* descendant count per collapsed parent
  // so the ring scale reflects how much subtree is hidden, not just the direct
  // child fan-out. This is what gives a single big ring around a parent that
  // hides 200 nodes vs. a small ring around one that hides 3 — the visual
  // signal users actually want from a tier-collapse view.
  //
  // BFS via getDescendants is O(subtree) per collapsed parent. With hundreds of
  // collapsed parents and thousands of descendants this is still cheap — runs
  // only when the collapsed set changes (slider move / click toggle), not per
  // frame.
  const collapsedParents = useMemo(() => {
    const out: Array<{ node: KGNode; descendantCount: number }> = [];
    for (const n of nodes) {
      const id = String(n.id);
      const h = hierarchyMap.get(id);
      if (!h || h.childIds.length === 0) continue;
      if (expansionState.isExpanded(id)) continue;
      out.push({
        node: n,
        descendantCount: getDescendants(id, hierarchyMap).length,
      });
    }
    return out;
  }, [nodes, hierarchyMap, expansionState]);

  const count = collapsedParents.length;

  const outerGeo = useMemo(() => new THREE.TorusGeometry(1.0, RING_OUTER, 6, RING_SEGMENTS), []);
  const innerGeo = useMemo(() => new THREE.TorusGeometry(0.7, RING_INNER, 6, RING_SEGMENTS), []);
  const mat = useMemo(() => new THREE.MeshBasicMaterial({
    color: RING_COLOR,
    transparent: true,
    opacity: 0.65,
    depthWrite: false,
    side: THREE.DoubleSide,
  }), []);

  useFrame(({ clock }) => {
    const outer = outerRef.current;
    const inner = innerRef.current;
    if (!outer || !inner || count === 0) return;

    const positions = nodePositionsRef.current;
    const t = clock.elapsedTime;

    for (let i = 0; i < count; i++) {
      const { node, descendantCount } = collapsedParents[i];
      const nodeId = String(node.id);
      const srcIdx = nodeIdToIndexMap.get(nodeId);
      const posIdx = srcIdx !== undefined ? srcIdx : i;
      const i3 = posIdx * 3;

      let x = 0, y = 0, z = 0;
      if (positions && i3 + 2 < positions.length) {
        x = positions[i3]; y = positions[i3 + 1]; z = positions[i3 + 2];
      } else if (node.position) {
        x = node.position.x; y = node.position.y; z = node.position.z;
      }

      // Scale grows with log of total descendants (not just direct children),
      // so a parent hiding 200 nodes is visibly bigger than one hiding 3.
      // Pulse amplitude also grows with the hidden subtree to make large
      // collapses pulse harder.
      const dc = Math.max(1, descendantCount);
      const sizeBoost = 1 + Math.log(dc) * 0.18;
      const ampBoost = 1 + Math.min(1.0, Math.log(dc) * 0.12);
      const localPulse = 1.0 + Math.sin(t * PULSE_SPEED) * PULSE_AMPLITUDE * ampBoost;
      const s = BASE_SCALE * localPulse * sizeBoost;

      _pos.set(x, y, z);
      _scale.set(s, s, s);
      _mat.compose(_pos, _quat, _scale);
      outer.setMatrixAt(i, _mat);

      const si = s * 0.85;
      _scale.set(si, si, si);
      _mat.compose(_pos, _quat, _scale);
      inner.setMatrixAt(i, _mat);
    }

    outer.count = count;
    inner.count = count;
    outer.instanceMatrix.needsUpdate = true;
    inner.instanceMatrix.needsUpdate = true;
  });

  if (count === 0) return null;

  return (
    <>
      <instancedMesh ref={outerRef} args={[outerGeo, mat, Math.max(count, 1)]} />
      <instancedMesh ref={innerRef} args={[innerGeo, mat, Math.max(count, 1)]} />
    </>
  );
};

export default CollapsedGroupRings;
