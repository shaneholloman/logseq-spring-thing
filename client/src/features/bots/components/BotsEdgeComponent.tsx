/**
 * BotsEdgeComponent.tsx
 * Edge between two agent nodes with:
 *   - Organic curved tendril (perpendicular sway via SimpleLine)
 *   - Activity-driven opacity / colour
 *   - Secondary energy channels at high token rates
 *   - 4-particle data-flow animation (zero-alloc: preallocated Vector3 refs +
 *     PARTICLE_BASE_T module constant)
 *
 * Zero-alloc contract: particleVecs preallocated in useRef; no Vector3 created
 * inside useFrame.  organicCurvePoints re-memos only when src/tgt/distance change
 * (floor(now*2) quantisation keeps sway updates sparse).
 */
import React, { useRef, useEffect, useState, useMemo } from 'react';
import { useFrame } from '@react-three/fiber';
import * as THREE from 'three';
import { BotsEdge, BotsAgent } from '../types/BotsTypes';
import { SimpleLine, PARTICLE_BASE_T } from './BotsShared';

export interface BotsEdgeComponentProps {
  edge: BotsEdge;
  sourcePos: THREE.Vector3;
  targetPos: THREE.Vector3;
  color: string;
  sourceAgent?: BotsAgent;
  targetAgent?: BotsAgent;
}

export const BotsEdgeComponent: React.FC<BotsEdgeComponentProps> = ({
  edge,
  sourcePos,
  targetPos,
  color,
  sourceAgent,
  targetAgent,
}) => {
  const [isActive, setIsActive] = useState(false);
  const particleVecs = useRef([
    new THREE.Vector3(), new THREE.Vector3(),
    new THREE.Vector3(), new THREE.Vector3(),
  ]);
  const elapsedRef = useRef(0);

  useFrame((state) => {
    elapsedRef.current = state.clock.elapsedTime;
  });

  useEffect(() => {
    const checkActivity = () => {
      setIsActive(Date.now() - edge.lastMessageTime < 5000);
    };
    checkActivity();
    const interval = setInterval(checkActivity, 1000);
    return () => clearInterval(interval);
  }, [edge.lastMessageTime]);

  const sourceTokenRate = sourceAgent?.tokenRate || 0;
  const targetTokenRate = targetAgent?.tokenRate || 0;
  const avgTokenRate    = (sourceTokenRate + targetTokenRate) / 2;

  const baseWidth    = Math.max(0.5, edge.dataVolume / 1000);
  const tokenWidth   = avgTokenRate > 0 ? Math.min(avgTokenRate / 10, 2) : 0;
  const messageWidth = edge.messageCount > 0 ? Math.min(edge.messageCount / 100, 1.5) : 0;
  const _lineWidth   = isActive
    ? Math.max(1.5, baseWidth + tokenWidth + messageWidth)
    : Math.max(0.5, baseWidth * 0.5);

  const baseOpacity  = isActive ? 0.8 : 0.3;
  const tokenOpacity = avgTokenRate > 10 ? Math.min(avgTokenRate / 50, 0.4) : 0;
  const opacity      = Math.min(baseOpacity + tokenOpacity, 1);

  const edgeColor = isActive
    ? (avgTokenRate > 20 ? '#E67E22' : avgTokenRate > 10 ? '#3498DB' : '#2980B9')
    : color;

  const shouldAnimate  = isActive && (avgTokenRate > 15 || edge.messageCount > 50);
  const animationSpeed = Math.min(avgTokenRate / 10, 3) + Math.min(edge.messageCount / 100, 2);
  const now            = elapsedRef.current;

  const shouldPulse    = avgTokenRate > 40 || edge.messageCount > 200;
  const pulseIntensity = shouldPulse ? Math.sin(now * 5) * 0.3 + 1 : 1;

  const distance = sourcePos.distanceTo(targetPos);

  const organicCurvePoints = useMemo(() => {
    const mid = new THREE.Vector3().copy(sourcePos).add(targetPos).multiplyScalar(0.5);
    const dir = new THREE.Vector3().subVectors(targetPos, sourcePos).normalize();
    const perp = new THREE.Vector3(-dir.y, dir.x, dir.z * 0.5).normalize();
    const sway = Math.sin(now * 0.5) * distance * 0.15;
    mid.add(perp.multiplyScalar(sway));
    return [sourcePos, mid, targetPos];
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sourcePos, targetPos, distance, Math.floor(now * 2)]);

  const sourceGlowColor = useMemo(() => {
    if (!sourceAgent) return '#3498DB';
    const h = sourceAgent.health || 50;
    if (h >= 80) return '#2ECC71';
    if (h >= 50) return '#F1C40F';
    return '#E74C3C';
  }, [sourceAgent]);

  return (
    <>
      {/* Main organic tendril */}
      <SimpleLine points={organicCurvePoints} color={edgeColor}
        opacity={opacity * pulseIntensity} transparent />

      {/* Secondary energy channel */}
      {avgTokenRate > 25 && isActive && (
        <SimpleLine points={organicCurvePoints} color="#F39C12"
          opacity={0.4 * pulseIntensity} transparent />
      )}

      {/* Overload channel */}
      {avgTokenRate > 50 && edge.messageCount > 300 && isActive && (
        <SimpleLine points={organicCurvePoints} color="#E74C3C"
          opacity={0.6 * pulseIntensity} transparent />
      )}

      {/* Data particles */}
      {isActive && shouldAnimate && (
        <group>
          {PARTICLE_BASE_T.map((baseT, i) => {
            const speedVar   = 1 + (i * 0.13);
            const t          = (baseT + (now * animationSpeed * speedVar * 0.15)) % 1;
            const particlePos = particleVecs.current[i];
            if (t < 0.5) {
              particlePos.lerpVectors(sourcePos, organicCurvePoints[1], t * 2);
            } else {
              particlePos.lerpVectors(organicCurvePoints[1], targetPos, (t - 0.5) * 2);
            }
            const sizeVar = 0.04 + Math.sin(now * 2 + i * 1.5) * 0.015;
            return (
              <mesh key={i} position={particlePos}>
                <sphereGeometry args={[sizeVar, 6, 6]} />
                <meshBasicMaterial
                  color={sourceGlowColor}
                  transparent
                  opacity={(0.6 + Math.sin(now * 3 + i) * 0.2) * pulseIntensity}
                />
              </mesh>
            );
          })}
        </group>
      )}
    </>
  );
};
