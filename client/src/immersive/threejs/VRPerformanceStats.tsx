/**
 * VRPerformanceStats
 *
 * VR-visible performance stats panel positioned in 3D space.
 * Follows the camera and displays connection count and LOD cache bars.
 */

import React, { useRef } from 'react';
import { useThree, useFrame } from '@react-three/fiber';
import * as THREE from 'three';

interface VRPerformanceStatsProps {
  activeConnections: number;
  lodCacheSize: number;
}

export const VRPerformanceStats: React.FC<VRPerformanceStatsProps> = ({
  activeConnections,
  lodCacheSize,
}) => {
  const { camera } = useThree();
  const groupRef = useRef<THREE.Group>(null);

  // Position stats panel in front of camera
  useFrame(() => {
    if (groupRef.current) {
      const offset = new THREE.Vector3(0, -0.3, -1);
      offset.applyQuaternion(camera.quaternion);
      groupRef.current.position.copy(camera.position).add(offset);
      groupRef.current.quaternion.copy(camera.quaternion);
    }
  });

  return (
    <group ref={groupRef}>
      {/* Background panel */}
      <mesh position={[0, 0, 0.01]}>
        <planeGeometry args={[0.4, 0.15]} />
        <meshBasicMaterial color="#000000" transparent opacity={0.7} />
      </mesh>

      {/* Connection bar */}
      <mesh position={[-0.15, 0.03, 0]}>
        <planeGeometry args={[Math.min(0.02 * activeConnections, 0.3), 0.03]} />
        <meshBasicMaterial color="#00ff88" />
      </mesh>

      {/* LOD cache bar */}
      <mesh position={[-0.15, -0.03, 0]}>
        <planeGeometry args={[Math.min(0.001 * lodCacheSize, 0.3), 0.03]} />
        <meshBasicMaterial color="#ffaa00" />
      </mesh>
    </group>
  );
};

export default VRPerformanceStats;
