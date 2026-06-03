/**
 * useEdgeBufferComputation — useFrame hot loop for edge buffer computation.
 *
 * Extracted from GraphManager.tsx (Phase B1 modularisation).
 * Reads SAB positions each frame, computes surface-to-surface edge endpoints,
 * fills pre-allocated buffers, and pushes to GlassEdgesHandle imperatively.
 * Zero allocations on the hot path (buffers only grow, never shrink).
 */
import { useRef } from 'react'
import { useFrame } from '@react-three/fiber'
import * as THREE from 'three'
import type { GraphData } from '../managers/graphDataManager'
import type { GlassEdgesHandle } from '../components/GlassEdges'
import type { GraphVisualMode } from './useGraphVisualState'
import { getEdgeTypeColor } from './useGraphNodeColors'
import { computeNodeScale } from '../utils/nodeScaling'
import { createLogger } from '../../../utils/loggerConfig'

const logger = createLogger('useEdgeBufferComputation')

export interface EdgeBufferComputationOptions {
  graphData: GraphData
  nodePositionsRef: React.MutableRefObject<Float32Array | null>
  nodeIdToIndexMap: Map<string, number>
  connectionCountMap: Map<string, number>
  perNodeVisualModeMap: Map<string, GraphVisualMode>
  hierarchyMap: Map<string, any>
  graphMode: GraphVisualMode
  /** IDs of nodes actually rendered (GraphManager's typeFilteredNodes). An edge
   *  is drawn only when BOTH endpoints are in this set, so pruned nodes
   *  (linked_page stubs, low-degree, quality-filtered, or type-toggled-off)
   *  never leave dangling edges. Single source of truth shared with the meshes. */
  visibleNodeIds: Set<string>
  graphTypeVisuals: any
  nodeSize: number
  selectedNodeId: string | null
  dragDataRef: React.MutableRefObject<{
    isDragging: boolean
    nodeId: string | null
    currentNodePos3D: THREE.Vector3
  }>
  edgeFlowRef: React.RefObject<GlassEdgesHandle | null>
  highlightEdgeFlowRef: React.RefObject<GlassEdgesHandle | null>
  highlightEdgePoints: number[]
  setEdgePoints: (pts: number[]) => void
  setHighlightEdgePoints: (pts: number[]) => void
}

export function useEdgeBufferComputation(opts: EdgeBufferComputationOptions) {
  const {
    graphData, nodePositionsRef, nodeIdToIndexMap, connectionCountMap,
    perNodeVisualModeMap, hierarchyMap, graphMode, visibleNodeIds,
    graphTypeVisuals, nodeSize, selectedNodeId, dragDataRef,
    edgeFlowRef, highlightEdgeFlowRef, highlightEdgePoints,
    setEdgePoints, setHighlightEdgePoints,
  } = opts

  // Pre-allocated reusable vectors (module-level would conflict across hook instances)
  const tempVec3      = useRef(new THREE.Vector3()).current
  const tempPosition  = useRef(new THREE.Vector3()).current
  const tempDirection = useRef(new THREE.Vector3()).current
  const tempSrcOff    = useRef(new THREE.Vector3()).current
  const tempTgtOff    = useRef(new THREE.Vector3()).current

  // Pre-allocated buffers — grow only, never shrink
  const edgeBufferRef         = useRef<number[]>([])
  const highlightBufferRef    = useRef<number[]>([])
  const edgeColorBufferRef    = useRef<Float32Array>(new Float32Array(0))
  const edgeWeightBufferRef   = useRef<Float32Array>(new Float32Array(0))
  const edgeUpdatePendingRef  = useRef<number[] | null>(null)
  const hlUpdatePendingRef    = useRef<number[] | null>(null)

  useFrame(() => {
    const positions = nodePositionsRef.current
    if (!positions) return

    // Position-buffer sufficiency is a function of NODE count (3 floats/node),
    // never edge count. The prior `positions.length >= graphData.edges.length`
    // guard froze the whole edge pipeline whenever a graph had more edges than
    // position-buffer floats (e.g. 94k edges vs 84k floats) — edges then never
    // refiltered/cleared, so node-type visibility toggles had no effect on them.
    if (graphData.nodes.length === 0 || positions.length < graphData.nodes.length * 3) return

    const edgeCount       = graphData.edges.length
    const edgeBufferNeeded = edgeCount * 6
    if (edgeBufferRef.current.length < edgeBufferNeeded) {
      edgeBufferRef.current = new Array<number>(edgeBufferNeeded)
    }
    const newEdgePoints = edgeBufferRef.current
    let edgePointIdx = 0

    const edgeColorNeeded = edgeCount * 3
    if (edgeColorBufferRef.current.length < edgeColorNeeded) {
      edgeColorBufferRef.current = new Float32Array(edgeColorNeeded)
    }
    const edgeColors = edgeColorBufferRef.current
    let edgeColorIdx = 0

    // Per-edge weights, written in lockstep with colours so index i in the
    // weight buffer matches instance i in GlassEdges (same emit/filter order).
    if (edgeWeightBufferRef.current.length < edgeCount) {
      edgeWeightBufferRef.current = new Float32Array(edgeCount)
    }
    const edgeWeights = edgeWeightBufferRef.current
    let edgeWeightIdx = 0

    const isDragging  = dragDataRef.current.isDragging
    const dragNodeId  = isDragging ? dragDataRef.current.nodeId : null
    const dragPos     = dragDataRef.current.currentNodePos3D

    graphData.edges.forEach(edge => {
      const sourceStr = String(edge.source)
      const targetStr = String(edge.target)
      // Skip edges touching a non-rendered node. Subsumes the node-type toggle
      // AND the linked_page / min-degree / quality prunes, keeping edges in
      // lockstep with the population meshes (no edges to invisible endpoints).
      if (!visibleNodeIds.has(sourceStr) || !visibleNodeIds.has(targetStr)) return
      const sourceNodeIndex = nodeIdToIndexMap.get(sourceStr)
      const targetNodeIndex = nodeIdToIndexMap.get(targetStr)
      if (sourceNodeIndex === undefined || targetNodeIndex === undefined) return

      const i3s = sourceNodeIndex * 3
      const i3t = targetNodeIndex * 3
      if (i3s + 2 >= positions.length || i3t + 2 >= positions.length) return

      if (dragNodeId === sourceStr) tempVec3.set(dragPos.x, dragPos.y, dragPos.z)
      else                          tempVec3.set(positions[i3s], positions[i3s + 1], positions[i3s + 2])
      if (dragNodeId === targetStr) tempPosition.set(dragPos.x, dragPos.y, dragPos.z)
      else                          tempPosition.set(positions[i3t], positions[i3t + 1], positions[i3t + 2])

      tempDirection.subVectors(tempPosition, tempVec3)
      const edgeLength = tempDirection.length()
      if (edgeLength <= 0.001) return

      tempDirection.normalize()
      const sourceNode = graphData.nodes[sourceNodeIndex]
      const targetNode = graphData.nodes[targetNodeIndex]
      const srcMode = perNodeVisualModeMap.get(sourceStr) || graphMode
      const tgtMode = perNodeVisualModeMap.get(targetStr) || graphMode

      const srcR = computeNodeScale(sourceNode, connectionCountMap, srcMode, hierarchyMap, graphTypeVisuals) * nodeSize
      const tgtR = computeNodeScale(targetNode, connectionCountMap, tgtMode, hierarchyMap, graphTypeVisuals) * nodeSize

      tempSrcOff.copy(tempVec3).addScaledVector(tempDirection, srcR)
      tempTgtOff.copy(tempPosition).addScaledVector(tempDirection, -tgtR)

      if (tempSrcOff.distanceTo(tempTgtOff) > 0.1) {
        newEdgePoints[edgePointIdx++] = tempSrcOff.x
        newEdgePoints[edgePointIdx++] = tempSrcOff.y
        newEdgePoints[edgePointIdx++] = tempSrcOff.z
        newEdgePoints[edgePointIdx++] = tempTgtOff.x
        newEdgePoints[edgePointIdx++] = tempTgtOff.y
        newEdgePoints[edgePointIdx++] = tempTgtOff.z
        const eColor = getEdgeTypeColor(edge.edgeType)
        edgeColors[edgeColorIdx++] = eColor.r
        edgeColors[edgeColorIdx++] = eColor.g
        edgeColors[edgeColorIdx++] = eColor.b
        // Weight in lockstep with colour/points so the index aligns with the
        // GlassEdges instance index (drives per-edge tube radius). Default 1.0
        // (the geometry's design radius) when an edge has no weight.
        edgeWeights[edgeWeightIdx++] = edge.weight ?? 1.0
      }
    })

    // Highlight edges for selected node
    if (selectedNodeId) {
      const hlBufferNeeded = edgeCount * 6
      if (highlightBufferRef.current.length < hlBufferNeeded) {
        highlightBufferRef.current = new Array<number>(hlBufferNeeded)
      }
      const hlBuf = highlightBufferRef.current
      let hlIdx = 0

      graphData.edges.forEach((edge: any) => {
        const sourceStr = String(edge.source)
        const targetStr = String(edge.target)
        if (sourceStr !== selectedNodeId && targetStr !== selectedNodeId) return
        if (!visibleNodeIds.has(sourceStr) || !visibleNodeIds.has(targetStr)) return

        const sourceIdx = nodeIdToIndexMap.get(sourceStr)
        const targetIdx = nodeIdToIndexMap.get(targetStr)
        if (sourceIdx === undefined || targetIdx === undefined) return

        const si3 = sourceIdx * 3
        const ti3 = targetIdx * 3
        if (si3 + 2 >= positions.length || ti3 + 2 >= positions.length) return

        if (dragNodeId === sourceStr) tempVec3.set(dragPos.x, dragPos.y, dragPos.z)
        else                          tempVec3.set(positions[si3], positions[si3 + 1], positions[si3 + 2])
        if (dragNodeId === targetStr) tempPosition.set(dragPos.x, dragPos.y, dragPos.z)
        else                          tempPosition.set(positions[ti3], positions[ti3 + 1], positions[ti3 + 2])

        tempDirection.subVectors(tempPosition, tempVec3)
        const len = tempDirection.length()
        if (len <= 0.001) return
        tempDirection.normalize()

        const srcNode = graphData.nodes[sourceIdx]
        const tgtNode = graphData.nodes[targetIdx]
        const srcMode = perNodeVisualModeMap.get(sourceStr) || graphMode
        const tgtMode = perNodeVisualModeMap.get(targetStr) || graphMode
        const srcR = computeNodeScale(srcNode, connectionCountMap, srcMode, hierarchyMap, graphTypeVisuals) * nodeSize
        const tgtR = computeNodeScale(tgtNode, connectionCountMap, tgtMode, hierarchyMap, graphTypeVisuals) * nodeSize

        tempSrcOff.copy(tempVec3).addScaledVector(tempDirection, srcR)
        tempTgtOff.copy(tempPosition).addScaledVector(tempDirection, -tgtR)

        if (tempSrcOff.distanceTo(tempTgtOff) > 0.2) {
          hlBuf[hlIdx++] = tempSrcOff.x; hlBuf[hlIdx++] = tempSrcOff.y; hlBuf[hlIdx++] = tempSrcOff.z
          hlBuf[hlIdx++] = tempTgtOff.x; hlBuf[hlIdx++] = tempTgtOff.y; hlBuf[hlIdx++] = tempTgtOff.z
        }
      })

      if (highlightEdgeFlowRef.current) {
        highlightEdgeFlowRef.current.updatePoints(hlBuf, hlIdx)
      } else {
        hlUpdatePendingRef.current = hlBuf.slice(0, hlIdx)
      }
    } else if (highlightEdgePoints.length > 0) {
      if (highlightEdgeFlowRef.current) highlightEdgeFlowRef.current.updatePoints([])
      else hlUpdatePendingRef.current = []
    }

    // Push edge buffers
    if (edgeFlowRef.current) {
      // Widths BEFORE points: updatePoints composes matrices reading the stored
      // weights, so the radius factor must be current when matrices rebuild.
      edgeFlowRef.current.updateWidths(edgeWeights, edgeWeightIdx)
      edgeFlowRef.current.updatePoints(newEdgePoints, edgePointIdx)
      const edgeCountWithColor = edgeColorIdx / 3
      if (edgeCountWithColor > 0) {
        edgeFlowRef.current.updateColors(edgeColors, edgeCountWithColor)
      }
    } else {
      edgeUpdatePendingRef.current = newEdgePoints.slice(0, edgePointIdx)
    }

    // Flush pending state updates for initial mount before imperative handles are available
    if (edgeUpdatePendingRef.current && !edgeFlowRef.current) {
      const pending = edgeUpdatePendingRef.current
      edgeUpdatePendingRef.current = null
      setEdgePoints(pending)
    }
    if (hlUpdatePendingRef.current !== null && !highlightEdgeFlowRef.current) {
      const pending = hlUpdatePendingRef.current
      hlUpdatePendingRef.current = null
      setHighlightEdgePoints(pending)
    }

    // One-time diagnostic
    if (!(window as unknown as Record<string, boolean>).__gmDiagV2) {
      ;(window as unknown as Record<string, boolean>).__gmDiagV2 = true
      let nonZeroCount = 0
      for (let si = 0; si < graphData.nodes.length; si++) {
        const si3 = si * 3
        if (Math.abs(positions[si3]) > 0.01 || Math.abs(positions[si3 + 1]) > 0.01 || Math.abs(positions[si3 + 2]) > 0.01) nonZeroCount++
      }
      logger.debug('[useEdgeBufferComputation] DIAG first frame:', {
        nodeCount: graphData.nodes.length,
        edgeCount: graphData.edges.length,
        positionsLength: positions.length,
        edgePointsComputed: edgePointIdx / 6,
        nonZeroPositions: nonZeroCount,
        hasEdgeFlowRef: !!edgeFlowRef.current,
      })
    }
  }, -2)
}
