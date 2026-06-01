import React, { useRef, useEffect, useState, useMemo, useCallback } from 'react'
import { useThree, useFrame, ThreeEvent } from '@react-three/fiber'
import * as THREE from 'three'
import { graphWorkerProxy } from '../managers/graphWorkerProxy'
import { usePlatformStore } from '../../../services/platformManager'
import { createLogger } from '../../../utils/loggerConfig'
import { useSettingsStore } from '../../../store/settingsStore'
import { BinaryNodeData, getActualNodeId } from '../../../types/binaryProtocol'
import { GemNodes, GemNodesHandle } from './GemNodes'
import { GlassEdges, GlassEdgesHandle } from './GlassEdges'
import { KnowledgeRings } from './KnowledgeRings'
import { ClusterHulls } from './ClusterHulls'
import { useGraphEventHandlers } from '../hooks/useGraphEventHandlers'
import { EdgeSettings } from '../../settings/config/settings'
import { useAnalyticsStore, useCurrentSSSPResult } from '../../analytics/store/analyticsStore'
import { AgentNodesLayer, useAgentNodes } from '../../visualisation/components/AgentNodesLayer'
import { TransientBeamsLayer, type BeamPositionResolver } from '../../visualisation/components/TransientBeamsLayer'
import { useGraphVisualState, type GraphVisualMode } from '../hooks/useGraphVisualState'
import { useGraphFiltering } from '../hooks/useGraphFiltering'
import { useFpsMonitor } from '../hooks/useFpsMonitor'
import { useCameraAutoFit } from '../hooks/useCameraAutoFit'
import { InstancedLabels } from './InstancedLabels'
import { layoutApi, type LayoutPosition } from '../../../api/layoutApi'
import { type GraphData } from '../managers/graphDataManager'
import { useGraphDataSubscription } from '../hooks/useGraphDataSubscription'
import { useGraphSelection } from '../hooks/useGraphSelection'
import { useEdgeBufferComputation } from '../hooks/useEdgeBufferComputation'

const logger = createLogger('GraphManager')

// Re-export GraphVisualMode from the hook for downstream consumers
export type { GraphVisualMode } from '../hooks/useGraphVisualState';

interface GraphManagerProps {
  onDragStateChange?: (isDragging: boolean) => void;
}

const GraphManager: React.FC<GraphManagerProps> = ({ onDragStateChange }) => {

  // Narrow selectors — subscribe only to sub-trees GraphManager actually reads.
  const logseqSettings  = useSettingsStore(s => s.settings?.visualisation?.graphs?.logseq)
  const graphTypeVisuals = useSettingsStore(s => s.settings?.visualisation?.graphTypeVisuals)
  const debugSettings   = useSettingsStore(s => s.settings?.system?.debug)
  const nodeFilterSettings = useSettingsStore(s => s.settings?.nodeFilter)
  const nodeTypeVisibility = useSettingsStore(
    s => s.settings?.visualisation?.graphs?.logseq?.nodes?.nodeTypeVisibility
  )
  // Stable ref for the full settings object — updated every render but doesn't trigger re-renders.
  const settingsRef = useRef(useSettingsStore.getState().settings)
  useEffect(() => {
    const unsub = useSettingsStore.subscribe(state => { settingsRef.current = state.settings })
    return unsub
  }, [])
  const settings = settingsRef.current

  const ssspResult = useCurrentSSSPResult()
  const normalizeDistances = useAnalyticsStore(state => state.normalizeDistances)
  const [normalizedSSSPResult, setNormalizedSSSPResult] = useState<any>(null)
  const isXRMode = usePlatformStore(state => state.isXRMode)
  const gemNodesRef = useRef<GemNodesHandle>(null)

  const [graphData, setGraphData] = useState<GraphData>({ nodes: [], edges: [] })

  // === Decomposed hooks: visual state + filtering ===
  const { perNodeVisualModeMap, hierarchyMap, connectionCountMap, dominantMode: graphMode } = useGraphVisualState(graphData)
  const { visibleNodes, nodeIdToIndexMap, expansionState } = useGraphFiltering(graphData, hierarchyMap, connectionCountMap)

  // Node-type visibility filtering
  const typeFilteredNodes = useMemo(() => {
    const vis = nodeTypeVisibility
    if (!vis || (vis.knowledge !== false && vis.ontology !== false && vis.agent !== false)) {
      return visibleNodes
    }
    const filtered = visibleNodes.filter(node => {
      const mode = perNodeVisualModeMap.get(String(node.id)) || graphMode
      if (mode === 'knowledge_graph') return vis.knowledge !== false
      if (mode === 'ontology')        return vis.ontology  !== false
      if (mode === 'agent')           return vis.agent     !== false
      return true
    })
    console.debug(`[TypeFilter] vis=${JSON.stringify(vis)} mapSize=${perNodeVisualModeMap.size} graphMode=${graphMode} ${visibleNodes.length}→${filtered.length}`)
    return filtered
  }, [visibleNodes, perNodeVisualModeMap, graphMode, nodeTypeVisibility])

  // Cluster hulls are scoped to the ONTOLOGY population. Louvain clusters mix
  // populations through the dominant KG<->ontology cross-links, so unscoped
  // hulls would span the separation gap and join the two discs visually.
  // Restricting to ontology nodes gives one cleanly delineated hull per
  // ontology cluster on its own disc.
  const ontologyHullNodes = useMemo(
    () => graphData.nodes.filter(
      node => (perNodeVisualModeMap.get(String(node.id)) || graphMode) === 'ontology'
    ),
    [graphData.nodes, perNodeVisualModeMap, graphMode]
  )

  const { agents: agentLayerNodes, connections: agentLayerConnections } = useAgentNodes()
  useFpsMonitor()

  const nodePositionsRef = useRef<Float32Array | null>(null)

  // === Transient agent-action beams (0x23) — id-space resolution ===
  //
  // Agent registry (/api/bots/agents) keys agents by the MASKED numeric node
  // id as a string (`String(getActualNodeId(nodeId))`), matching the
  // reconciliation in BotsDataContext. We build a lookup from that key to the
  // agent's world position so source_agent_id (which may arrive with the high
  // AGENT_NODE_FLAG bit set) resolves consistently.
  const agentPositionByMaskedId = useMemo(() => {
    const map = new Map<string, { x: number; y: number; z: number }>()
    for (const agent of agentLayerNodes) {
      if (agent.position) map.set(String(agent.id), agent.position)
    }
    return map
  }, [agentLayerNodes])
  const agentPositionMapRef = useRef(agentPositionByMaskedId)
  agentPositionMapRef.current = agentPositionByMaskedId

  // Resolve source_agent_id → agent world position. Mask the AGENT_NODE_FLAG
  // (and any other high flag bits) via getActualNodeId, then look up by the
  // masked string key; fall back to the raw id for safety. Returns false when
  // unresolvable so the beam is skipped silently.
  const resolveAgentPosition = useCallback<BeamPositionResolver>((id, out) => {
    const map = agentPositionMapRef.current
    const masked = getActualNodeId(id)
    const pos = map.get(String(masked)) ?? map.get(String(id))
    if (!pos) return false
    out.set(pos.x, pos.y, pos.z)
    return true
  }, [])

  // Resolve target_node_id → KG node world position from the LIVE position
  // buffer (SAB), via the same nodeIdToIndexMap the edge renderer uses. Try the
  // raw id first, then the masked id (KG nodes may carry KNOWLEDGE/ontology
  // flag bits). Returns false when unresolvable so the beam is skipped.
  const resolveNodePosition = useCallback<BeamPositionResolver>((id, out) => {
    const positions = nodePositionsRef.current
    if (!positions) return false
    let index = nodeIdToIndexMap.get(String(id))
    if (index === undefined) index = nodeIdToIndexMap.get(String(getActualNodeId(id)))
    if (index === undefined) return false
    const i3 = index * 3
    if (i3 + 2 >= positions.length) return false
    out.set(positions[i3], positions[i3 + 1], positions[i3 + 2])
    return true
  }, [nodeIdToIndexMap])

  // Layout mode transition state
  const transitionRef = useRef<{
    active: boolean
    startPositions: Float32Array
    targetPositions: Float32Array
    progress: number
    duration: number
    startTime: number
  } | null>(null)
  const [activeLayoutMode, setActiveLayoutMode] = useState<string>('')
  const [layoutTransitioning, setLayoutTransitioning] = useState(false)

  const { requestFit: requestCameraFit } = useCameraAutoFit(nodePositionsRef, graphData.nodes.length)

  const [edgePoints, setEdgePoints]               = useState<number[]>([])
  const [highlightEdgePoints, setHighlightEdgePoints] = useState<number[]>([])
  const edgeFlowRef          = useRef<GlassEdgesHandle>(null)
  const highlightEdgeFlowRef = useRef<GlassEdgesHandle>(null)
  const prevLabelPositionsLengthRef = useRef<number>(0)
  const labelPositionsRef = useRef<Array<{x: number, y: number, z: number}>>([])
  const labelTickRef  = useRef(0)
  const [labelUpdateTick, setLabelUpdateTick] = useState(0)

  const [nodesAreAtOrigin, setNodesAreAtOrigin] = useState(false)
  const [forceUpdate, setForceUpdate] = useState(0)

  const frustum = useMemo(() => new THREE.Frustum(), [])
  const cameraViewProjectionMatrix = useMemo(() => new THREE.Matrix4(), [])

  const animationStateRef = useRef({
    time: 0,
    selectedNode: null as string | null,
    hoveredNode: null as string | null,
    pulsePhase: 0,
  })

  const [dragState, setDragState] = useState<{ nodeId: string | null; instanceId: number | null }>({
    nodeId: null,
    instanceId: null,
  })
  const dragDataRef = useRef({
    isDragging: false,
    pointerDown: false,
    nodeId: null as string | null,
    instanceId: null as number | null,
    startPointerPos: new THREE.Vector2(),
    startTime: 0,
    startNodePos3D: new THREE.Vector3(),
    currentNodePos3D: new THREE.Vector3(),
    lastUpdateTime: 0,
    pendingUpdate: null as BinaryNodeData | null,
  })

  const { camera, size } = useThree()
  const nodeSettings = logseqSettings?.nodes || settings?.visualisation?.nodes

  useEffect(() => {
    if (ssspResult) {
      const normalized = normalizeDistances(ssspResult)
      setNormalizedSSSPResult({ ...ssspResult, normalizedDistances: normalized })
    } else {
      setNormalizedSSSPResult(null)
    }
  }, [ssspResult, normalizeDistances])

  // ===  Layout mode transition helpers ===
  const startLayoutTransition = useCallback((targetPositions: LayoutPosition[], durationMs: number) => {
    const positions = nodePositionsRef.current
    const nodeCount = graphData.nodes.length
    if (!positions || nodeCount === 0) return

    const needed = nodeCount * 3
    const startSnap = new Float32Array(needed)
    startSnap.set(positions.subarray(0, Math.min(needed, positions.length)))

    const targetSnap = new Float32Array(needed)
    const idxById = new Map<number, number>()
    for (let i = 0; i < targetPositions.length; i++) idxById.set(targetPositions[i].id, i)
    for (let ni = 0; ni < nodeCount; ni++) {
      const node = graphData.nodes[ni]
      const numericId = parseInt(String(node.id), 10)
      const tp = idxById.get(numericId)
      if (tp !== undefined) {
        targetSnap[ni * 3]     = targetPositions[tp].x
        targetSnap[ni * 3 + 1] = targetPositions[tp].y
        targetSnap[ni * 3 + 2] = targetPositions[tp].z
      } else {
        targetSnap[ni * 3]     = startSnap[ni * 3]
        targetSnap[ni * 3 + 1] = startSnap[ni * 3 + 1]
        targetSnap[ni * 3 + 2] = startSnap[ni * 3 + 2]
      }
    }
    transitionRef.current = { active: true, startPositions: startSnap, targetPositions: targetSnap, progress: 0, duration: durationMs, startTime: Date.now() }
    setLayoutTransitioning(true)
  }, [graphData.nodes])

  // Physics fingerprint (ADR-03 D7: no-op worker forward; kept as dependency marker)
  const logseqPhysics   = logseqSettings?.physics
  const visionclawPhysics = useSettingsStore(s => s.settings?.visualisation?.graphs?.visionclaw?.physics)
  const physicsFingerprint = useMemo(() => JSON.stringify({ vf: visionclawPhysics, lq: logseqPhysics }), [visionclawPhysics, logseqPhysics])
  useEffect(() => { void physicsFingerprint }, [physicsFingerprint])

  // layoutMode → layoutApi.setMode
  const layoutMode = useSettingsStore(s =>
    (s.settings as unknown as Record<string, Record<string, unknown>>)?.qualityGates?.layoutMode as string | undefined
  )
  const prevLayoutModeRef = useRef<string | undefined>(undefined)
  useEffect(() => {
    if (!layoutMode || layoutMode === prevLayoutModeRef.current) return
    prevLayoutModeRef.current = layoutMode
    const TRANSITION_MS = 800
    setActiveLayoutMode(layoutMode)
    setLayoutTransitioning(true)
    layoutApi.setMode(layoutMode, TRANSITION_MS).then(response => {
      const { data } = response
      if (data.success && data.positions && data.positions.length > 0) {
        startLayoutTransition(data.positions, data.transitionMs ?? TRANSITION_MS)
      } else {
        setLayoutTransitioning(false)
      }
    }).catch(err => {
      logger.warn('[GraphManager] layoutApi.setMode failed:', err)
      setLayoutTransitioning(false)
    })
  }, [layoutMode, startLayoutTransition])

  // === Graph data subscription ===
  useGraphDataSubscription({
    onGraphData: setGraphData,
    onEdgePoints: setEdgePoints,
    onNodesAtOrigin: setNodesAreAtOrigin,
    settings: settingsRef,
  })

  // === Selection state + camera fly-to + search events ===
  const { selectedNodeId, setSelectedNodeId, flyToTargetRef, flyToProgressRef } = useGraphSelection({
    graphData,
    nodeIdToIndexMap,
    nodePositionsRef,
    connectionCountMap,
    camera,
  })

  // === Priority -2 useFrame: SAB reads, layout transition LERP, label pos, camera fly-to ===
  useFrame((state, delta) => {
    animationStateRef.current.time = state.clock.elapsedTime

    // Camera fly-to animation
    if (flyToTargetRef.current) {
      flyToProgressRef.current = Math.min(1, flyToProgressRef.current + delta * 2.0)
      const eased = 1 - Math.pow(1 - flyToProgressRef.current, 3)
      camera.position.lerp(flyToTargetRef.current, eased * 0.08)
      if (flyToProgressRef.current >= 1) flyToTargetRef.current = null
    }

    // Periodic label frustum refresh (~4 updates/sec at 60fps)
    labelTickRef.current++
    if (labelTickRef.current >= 15) {
      labelTickRef.current = 0
      cameraViewProjectionMatrix.multiplyMatrices(camera.projectionMatrix, camera.matrixWorldInverse)
      frustum.setFromProjectionMatrix(cameraViewProjectionMatrix)
      setLabelUpdateTick(prev => prev + 1)
    }

    if (graphData.nodes.length > 0) {
      const positions = graphWorkerProxy.getPositionsSync()
      if (!positions) return

      if (!nodePositionsRef.current) {
        let hasNonZero = false
        const checkLen = Math.min(graphData.nodes.length * 3, positions.length)
        for (let ci = 0; ci < checkLen; ci++) {
          if (positions[ci] !== 0) { hasNonZero = true; break }
        }
        if (!hasNonZero && checkLen > 0) return
      }
      nodePositionsRef.current = positions

      // Layout mode transition: mass-aware LERP
      if (transitionRef.current?.active) {
        const t = transitionRef.current
        const elapsed = Date.now() - t.startTime
        const rawProgress = Math.min(elapsed / t.duration, 1.0)
        const progress = rawProgress < 0.5
          ? 2 * rawProgress * rawProgress
          : 1 - Math.pow(-2 * rawProgress + 2, 2) / 2
        const nodeCount = graphData.nodes.length
        for (let i = 0; i < nodeCount; i++) {
          const idx = i * 3
          if (idx + 2 >= positions.length) break
          const cc = connectionCountMap.get(String(i)) || 0
          const massFactor = 1.0 / (1.0 + Math.sqrt(cc) * 0.3)
          const np = Math.min(progress / massFactor, 1.0)
          positions[idx]     = t.startPositions[idx]     + (t.targetPositions[idx]     - t.startPositions[idx])     * np
          positions[idx + 1] = t.startPositions[idx + 1] + (t.targetPositions[idx + 1] - t.startPositions[idx + 1]) * np
          positions[idx + 2] = t.startPositions[idx + 2] + (t.targetPositions[idx + 2] - t.startPositions[idx + 2]) * np
        }
        if (rawProgress >= 1.0) { transitionRef.current.active = false; setLayoutTransitioning(false) }
      }

      requestCameraFit()

      const positionsValid = positions.length >= graphData.nodes.length * 3

      if (positionsValid) {
        // Update label positions ref every frame (fast, no re-render)
        const labelCount = graphData.nodes.length
        let labelArr = labelPositionsRef.current
        if (labelArr.length !== labelCount) {
          labelArr = new Array(labelCount)
          for (let i = 0; i < labelCount; i++) labelArr[i] = { x: 0, y: 0, z: 0 }
        }
        for (let i = 0; i < labelCount; i++) {
          const i3 = i * 3
          labelArr[i].x = positions[i3]
          labelArr[i].y = positions[i3 + 1]
          labelArr[i].z = positions[i3 + 2]
        }
        labelPositionsRef.current = labelArr
        prevLabelPositionsLengthRef.current = labelCount
      }
    }
  }, -2)

  // === Edge buffer computation (extracted hot loop) — runs in its own useFrame(-2) ===
  useEdgeBufferComputation({
    graphData,
    nodePositionsRef,
    nodeIdToIndexMap,
    connectionCountMap,
    perNodeVisualModeMap,
    hierarchyMap,
    graphMode,
    nodeTypeVisibility,
    graphTypeVisuals,
    nodeSize: nodeSettings?.nodeSize ?? 0.5,
    selectedNodeId,
    dragDataRef,
    edgeFlowRef,
    highlightEdgeFlowRef,
    highlightEdgePoints,
    setEdgePoints,
    setHighlightEdgePoints,
  })

  // Proxy ref: useGraphEventHandlers expects RefObject<InstancedMesh>.
  const meshProxyRef = useMemo(() => ({
    get current() { return gemNodesRef.current?.getMesh() ?? null },
    set current(_v: any) { /* GemNodes owns the mesh */ },
  }), []) as React.RefObject<THREE.InstancedMesh>

  const { handlePointerDown, handlePointerMove, handlePointerUp } = useGraphEventHandlers(
    meshProxyRef,
    dragDataRef,
    setDragState,
    graphData,
    typeFilteredNodes,
    camera,
    size,
    settings,
    setGraphData,
    onDragStateChange,
    setSelectedNodeId,
  )

  const defaultEdgeSettings: EdgeSettings = {
    arrowSize: 0.5,
    baseWidth: 0.1,
    color: '#FF5722',
    enableArrows: true,
    opacity: 0.15,
    widthRange: [0.1, 0.3],
    quality: 'medium',
    enableFlowEffect: false,
    flowSpeed: 1,
    flowIntensity: 1,
    glowStrength: 1,
    distanceIntensity: 0.5,
    useGradient: false,
    gradientColors: ['#ff0000', '#0000ff'],
  }

  useEffect(() => {
    if (debugSettings?.enableNodeDebug) {
      logger.debug('Component mounted', {
        nodeCount: graphData.nodes.length,
        edgeCount: graphData.edges.length,
        edgePointsLength: edgePoints.length,
        gemNodesRef: !!gemNodesRef.current,
      })
    }
    return () => {
      if (debugSettings?.enableNodeDebug) logger.debug('Component unmounting')
    }
  }, [])

  return (
    <>
      <GemNodes
        ref={gemNodesRef}
        nodes={typeFilteredNodes}
        edges={graphData.edges}
        graphMode={graphMode}
        perNodeVisualModeMap={perNodeVisualModeMap}
        nodePositionsRef={nodePositionsRef}
        connectionCountMap={connectionCountMap}
        hierarchyMap={hierarchyMap}
        nodeIdToIndexMap={nodeIdToIndexMap}
        settings={settings}
        ssspResult={normalizedSSSPResult}
        dragDataRef={dragDataRef}
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={(event: any) => handlePointerUp(event)}
        onPointerMissed={() => {
          if (dragDataRef.current.pointerDown) handlePointerUp()
          setSelectedNodeId(null)
        }}
        onDoubleClick={(event: ThreeEvent<MouseEvent>) => {
          if (event.instanceId !== undefined && event.instanceId < typeFilteredNodes.length) {
            const node = typeFilteredNodes[event.instanceId]
            if (node) {
              const pageUrl  = node.metadata?.page_url || node.metadata?.pageUrl || node.metadata?.url
              if (pageUrl) { window.open(pageUrl, '_blank', 'noopener,noreferrer'); return }
              const filePath = node.metadata?.file_path || node.metadata?.filePath || node.metadata?.path
              if (filePath) { window.open(`https://narrativegoldmine.com/#/page/${encodeURIComponent(filePath)}`, '_blank', 'noopener,noreferrer'); return }
              if (node.label) { window.open(`https://narrativegoldmine.com/#/page/${encodeURIComponent(node.label)}`, '_blank', 'noopener,noreferrer'); return }
              const hierarchyNode = hierarchyMap.get(node.id)
              if (hierarchyNode && hierarchyNode.childIds.length > 0) expansionState.toggleExpansion(node.id)
            }
          }
        }}
        selectedNodeId={selectedNodeId}
      />

      <GlassEdges
        ref={edgeFlowRef}
        points={edgePoints}
        settings={settings?.visualisation?.graphs?.logseq?.edges || settings?.visualisation?.edges || defaultEdgeSettings}
        colorOverride={
          graphMode === 'knowledge_graph'
            ? settings?.visualisation?.graphTypeVisuals?.knowledgeGraph?.edgeColor
            : graphMode === 'ontology'
            ? settings?.visualisation?.graphTypeVisuals?.ontology?.edgeColor
            : undefined
        }
      />

      <GlassEdges
        ref={highlightEdgeFlowRef}
        points={highlightEdgePoints}
        settings={settings?.visualisation?.graphs?.logseq?.edges || settings?.visualisation?.edges || defaultEdgeSettings}
        colorOverride={settings?.visualisation?.interaction?.selectionHighlightColor || '#00FFFF'}
      />

      <KnowledgeRings
        nodes={typeFilteredNodes}
        perNodeVisualModeMap={perNodeVisualModeMap}
        nodePositionsRef={nodePositionsRef}
        nodeIdToIndexMap={nodeIdToIndexMap}
        connectionCountMap={connectionCountMap}
        edges={graphData.edges}
        hierarchyMap={hierarchyMap}
        settings={settings}
        nodeColorSourceRef={gemNodesRef}
      />

      <ClusterHulls
        nodes={ontologyHullNodes}
        nodePositionsRef={nodePositionsRef}
        nodeIdToIndexMap={nodeIdToIndexMap}
        settings={settings}
      />

      {agentLayerNodes.length > 0 && nodeTypeVisibility?.agent !== false && (
        <AgentNodesLayer agents={agentLayerNodes} connections={agentLayerConnections} />
      )}

      {/* Embodied agent-action beams (0x23): agent node → KG node, coloured
          by action type, opacity fades in → holds → out over duration_ms. */}
      <TransientBeamsLayer
        resolveAgentPosition={resolveAgentPosition}
        resolveNodePosition={resolveNodePosition}
      />

      <InstancedLabels
        nodes={typeFilteredNodes}
        nodeIdToIndexMap={nodeIdToIndexMap}
        nodePositionsRef={nodePositionsRef}
        labelPositionsRef={labelPositionsRef}
        settings={settings}
        graphMode={graphMode}
        perNodeVisualModeMap={perNodeVisualModeMap}
        connectionCountMap={connectionCountMap}
        hierarchyMap={hierarchyMap}
        graphTypeVisuals={graphTypeVisuals}
        ssspResult={normalizedSSSPResult}
        isXRMode={isXRMode}
      />
    </>
  )
}

export default GraphManager
