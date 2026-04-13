/**
 * VRAgentActionScene
 *
 * Complete VR scene for agent action visualization.
 * Integrates VRActionConnectionsLayer with hand tracking and XR controls.
 *
 * Usage:
 * ```tsx
 * <Canvas>
 *   <XR store={xrStore}>
 *     <VRAgentActionScene agents={agents} />
 *   </XR>
 * </Canvas>
 * ```
 *
 * Performance targets:
 * - Quest 3: 72fps stable
 * - Max 20 active connections
 * - LOD-based geometry reduction
 */

import React, { useMemo, useEffect, useCallback } from 'react';
import { useXREvent } from '@react-three/xr';
import { useThree, useFrame } from '@react-three/fiber';
import * as THREE from 'three';
import { VRActionConnectionsLayer } from './VRActionConnectionsLayer';
import { VRTargetHighlight } from './VRTargetHighlight';
import { VRPerformanceStats } from './VRPerformanceStats';
import { useActionConnections, ActionConnection } from '../../features/visualisation/hooks/useActionConnections';
import { useAgentActionVisualization } from '../../features/visualisation/hooks/useAgentActionVisualization';
import { useVRHandTracking, agentsToTargetNodes, TargetNode } from '../hooks/useVRHandTracking';
import { useVRConnectionsLOD, calculateOptimalThresholds } from '../hooks/useVRConnectionsLOD';
import { updateHandTrackingFromSession } from '../hooks/updateHandTrackingFromSession';
import { createLogger } from '../../utils/loggerConfig';
import { AgentData } from '../types';

const logger = createLogger('VRAgentActionScene');

interface VRAgentActionSceneProps {
  /** Agent data for targeting */
  agents?: AgentData[];
  /** Maximum connections (auto-scales for performance) */
  maxConnections?: number;
  /** Base animation duration (ms) */
  baseDuration?: number;
  /** Enable hand tracking interaction */
  enableHandTracking?: boolean;
  /** Show performance overlay */
  showStats?: boolean;
  /** Callback when agent is targeted */
  onAgentTargeted?: (agentId: string | null) => void;
  /** Callback when agent is selected (pinch/trigger) */
  onAgentSelected?: (agentId: string) => void;
  /** Enable debug visualization */
  debug?: boolean;
}

export const VRAgentActionScene: React.FC<VRAgentActionSceneProps> = ({
  agents = [],
  maxConnections = 20,
  baseDuration = 500,
  enableHandTracking = true,
  showStats = false,
  onAgentTargeted,
  onAgentSelected,
  debug = false,
}) => {
  const { gl, camera } = useThree();

  // Calculate optimal LOD thresholds
  const lodConfig = useMemo(
    () => calculateOptimalThresholds(72, maxConnections),
    [maxConnections]
  );

  // LOD management
  const { updateCameraPosition, getCacheStats } = useVRConnectionsLOD(lodConfig);

  // Action visualization hook (VR mode)
  const { connections, activeCount } = useAgentActionVisualization({
    enabled: true,
    maxConnections,
    baseDuration,
    vrMode: true,
    debug,
  });

  // Convert agents to target nodes
  const targetNodes = useMemo(() => agentsToTargetNodes(agents), [agents]);

  // Hand tracking
  const {
    previewStart,
    previewEnd,
    showPreview,
    previewColor,
    targetedNode,
    setTargetNodes,
    updateHandState,
    triggerHaptic,
  } = useVRHandTracking({
    maxRayDistance: 30,
    targetRadius: 1.5,
    enableHaptics: true,
  });

  // Update target nodes when agents change
  useEffect(() => {
    setTargetNodes(targetNodes);
  }, [targetNodes, setTargetNodes]);

  // Notify parent of targeted agent
  useEffect(() => {
    onAgentTargeted?.(targetedNode?.id || null);
  }, [targetedNode, onAgentTargeted]);

  // Handle XR controller input
  useXREvent('selectstart', (event) => {
    // Access handedness from the inputSource data
    const inputSource = event.data as XRInputSource;
    if (targetedNode && inputSource?.handedness === 'right') {
      logger.info('Agent selected via VR controller:', targetedNode.id);
      triggerHaptic('primary', 0.8, 100);
      onAgentSelected?.(targetedNode.id);
    }
  });

  // Update camera position for LOD each frame
  useFrame(() => {
    updateCameraPosition(camera.position);

    // Update hand tracking from XR session
    const session = (gl.xr as unknown as { getSession?: () => XRSession | null })?.getSession?.();
    if (session && enableHandTracking) {
      updateHandTrackingFromSession(session, updateHandState);
    }
  });

  // Calculate opacity based on active count
  const opacity = useMemo(() => {
    if (activeCount > 18) return 0.6;
    if (activeCount > 12) return 0.8;
    return 1.0;
  }, [activeCount]);

  // Convert Vector3 to the expected format
  const handPos = useMemo(() => {
    if (!previewStart) return null;
    return previewStart.clone();
  }, [previewStart]);

  const targetPos = useMemo(() => {
    if (!previewEnd) return null;
    return previewEnd.clone();
  }, [previewEnd]);

  return (
    <group name="vr-agent-action-scene">
      {/* Main action connections layer */}
      <VRActionConnectionsLayer
        connections={connections}
        opacity={opacity}
        showHandPreview={showPreview && enableHandTracking}
        handPosition={handPos}
        previewTarget={targetPos}
        previewColor={previewColor}
      />

      {/* Target highlight when agent is being targeted */}
      {targetedNode && (
        <VRTargetHighlight
          position={targetedNode.position}
          color={previewColor}
        />
      )}

      {/* Performance stats (debug) */}
      {showStats && (
        <VRPerformanceStats
          activeConnections={activeCount}
          lodCacheSize={getCacheStats().size}
        />
      )}
    </group>
  );
};

export default VRAgentActionScene;
