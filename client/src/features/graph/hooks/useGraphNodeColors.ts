/**
 * useGraphNodeColors — colour tables and node/edge colour resolution helpers.
 *
 * Extracted from GraphManager.tsx (Phase B1 modularisation).
 * All objects are module-level singletons to guarantee zero per-frame allocation.
 */
import * as THREE from 'three'
import type { Node as GraphNode } from '../managers/graphDataManager'
import type { GraphVisualMode } from './useGraphVisualState'

// === Domain colours ===
export const DOMAIN_COLORS: Record<string, string> = {
  'AI':  '#4FC3F7',
  'BC':  '#81C784',
  'RB':  '#FFB74D',
  'MV':  '#CE93D8',
  'TC':  '#FFD54F',
  'DT':  '#EF5350',
  'NGM': '#4DB6AC',
}
export const DEFAULT_DOMAIN_COLOR = '#90A4AE'

export const getDomainColor = (domain?: string): string =>
  domain && DOMAIN_COLORS[domain] ? DOMAIN_COLORS[domain] : DEFAULT_DOMAIN_COLOR

// === Edge-type colours ===
export const EDGE_TYPE_COLORS: Record<string, THREE.Color> = {
  'hierarchical':  new THREE.Color('#FFD700'),
  'subclass':      new THREE.Color('#FFD700'),
  'structural':    new THREE.Color('#4FC3F7'),
  'has_part':      new THREE.Color('#4FC3F7'),
  'is_part_of':    new THREE.Color('#4FC3F7'),
  'dependency':    new THREE.Color('#81C784'),
  'requires':      new THREE.Color('#81C784'),
  'depends_on':    new THREE.Color('#81C784'),
  'enables':       new THREE.Color('#81C784'),
  'associative':   new THREE.Color('#CE93D8'),
  'relates_to':    new THREE.Color('#CE93D8'),
  'bridge':        new THREE.Color('#FF7043'),
  'bridges_to':    new THREE.Color('#FF7043'),
  'bridges_from':  new THREE.Color('#FF7043'),
  'explicit_link': new THREE.Color('#FFFFFF'),
  'namespace':     new THREE.Color('#78909C'),
  'inferred':      new THREE.Color('#B0BEC5'),
}
export const DEFAULT_EDGE_COLOR = new THREE.Color('#AAAAAA')

/** O(1) edge-type → pre-allocated THREE.Color. */
export function getEdgeTypeColor(edgeType?: string): THREE.Color {
  if (!edgeType) return DEFAULT_EDGE_COLOR
  return EDGE_TYPE_COLORS[edgeType] ?? EDGE_TYPE_COLORS[edgeType.toLowerCase()] ?? DEFAULT_EDGE_COLOR
}

// === Ontology depth spectrum ===
export const ONTOLOGY_DEPTH_COLORS: THREE.Color[] = [
  new THREE.Color('#FF6B6B'),
  new THREE.Color('#FFD93D'),
  new THREE.Color('#4ECDC4'),
  new THREE.Color('#AA96DA'),
  new THREE.Color('#95E1D3'),
]
export const ONTOLOGY_PROPERTY_COLOR = new THREE.Color('#F38181')
export const ONTOLOGY_INSTANCE_COLOR  = new THREE.Color('#B8D4E3')

// === Agent status/type colours ===
export const AGENT_STATUS_COLORS: Record<string, THREE.Color> = {
  'active':  new THREE.Color('#2ECC71'),
  'busy':    new THREE.Color('#F39C12'),
  'idle':    new THREE.Color('#95A5A6'),
  'error':   new THREE.Color('#E74C3C'),
  'default': new THREE.Color('#2ECC71'),
}
export const AGENT_TYPE_COLORS: Record<string, THREE.Color> = {
  'queen':       new THREE.Color('#FFD700'),
  'coordinator': new THREE.Color('#E67E22'),
}

// === Knowledge-graph node type colours ===
const TYPE_THREE_COLORS: Record<string, THREE.Color> = {
  'folder':   new THREE.Color('#FFD700'),
  'file':     new THREE.Color('#00CED1'),
  'function': new THREE.Color('#FF6B6B'),
  'class':    new THREE.Color('#4ECDC4'),
  'variable': new THREE.Color('#95E1D3'),
  'import':   new THREE.Color('#F38181'),
  'export':   new THREE.Color('#AA96DA'),
  'default':  new THREE.Color('#00ffff'),
}

// Reusable singleton — callers must not hold a reference across calls.
const _nodeColor = new THREE.Color()

/**
 * Mode-aware node colour resolver.
 * Returns the shared `_nodeColor` instance — consume values before the next call.
 */
export function getNodeColor(
  node: GraphNode,
  ssspResult?: any,
  graphMode: GraphVisualMode = 'knowledge_graph',
  hierarchyMap?: Map<string, any>,
  connectionCountMap?: Map<string, number>,
): THREE.Color {

  // SSSP overrides all modes
  if (ssspResult) {
    const distance = ssspResult.distances[node.id]
    if (node.id === ssspResult.sourceNodeId) return _nodeColor.set('#00FFFF')
    if (!isFinite(distance)) return _nodeColor.set('#666666')
    const normalizedDistances = ssspResult.normalizedDistances || {}
    const nd = normalizedDistances[node.id] || 0
    return _nodeColor.setRGB(Math.min(1, nd * 1.2), Math.min(1, (1 - nd) * 1.2), 0.1)
  }

  // Ontology mode: cosmic hierarchy spectrum
  if (graphMode === 'ontology') {
    const nodeType = node.metadata?.type?.toLowerCase() || ''
    if (nodeType === 'property' || nodeType === 'datatype_property' || nodeType === 'object_property')
      return _nodeColor.copy(ONTOLOGY_PROPERTY_COLOR)
    if (nodeType === 'instance' || nodeType === 'individual')
      return _nodeColor.copy(ONTOLOGY_INSTANCE_COLOR)
    const hierarchyNode = hierarchyMap?.get(node.id)
    const depth = hierarchyNode?.depth ?? (node.metadata?.depth ?? 0)
    _nodeColor.copy(ONTOLOGY_DEPTH_COLORS[Math.min(depth, ONTOLOGY_DEPTH_COLORS.length - 1)])
    const instanceCount = parseInt(node.metadata?.instanceCount || '0', 10)
    if (instanceCount > 0) {
      const g = Math.min(instanceCount / 50, 0.4)
      _nodeColor.offsetHSL(0, g * 0.2, g * 0.15)
    }
    return _nodeColor
  }

  // Agent mode: status-based bioluminescence
  if (graphMode === 'agent') {
    const agentType = node.metadata?.agentType?.toLowerCase() || ''
    if (AGENT_TYPE_COLORS[agentType]) return _nodeColor.copy(AGENT_TYPE_COLORS[agentType])
    const statusColor = AGENT_STATUS_COLORS[node.metadata?.status?.toLowerCase() || 'active'] || AGENT_STATUS_COLORS['default']
    return _nodeColor.copy(statusColor)
  }

  // Knowledge graph mode (default): authority brightness + metallic tint
  const nodeType = node.metadata?.type || 'default'
  _nodeColor.copy(TYPE_THREE_COLORS[nodeType] ?? TYPE_THREE_COLORS['default'])
  const authority = node.metadata?.authority ?? node.metadata?.authorityScore ?? 0
  if (authority > 0) _nodeColor.offsetHSL(0, authority * 0.06, authority * 0.3)
  const connections = connectionCountMap?.get(node.id) || 0
  if (connections > 5) {
    const ms = Math.min(connections / 30, 0.15)
    _nodeColor.offsetHSL(-0.02 * ms, 0.1 * ms, 0.05 * ms)
  }
  return _nodeColor
}

// === Initial position generation (Fibonacci sphere) ===
export function getPositionForNode(
  node: GraphNode,
  index: number,
  totalNodes: number,
): [number, number, number] {
  if (!node.position || (node.position.x === 0 && node.position.y === 0 && node.position.z === 0)) {
    const goldenAngle = Math.PI * (3 - Math.sqrt(5))
    const theta = index * goldenAngle
    const y = 1 - (index / totalNodes) * 2
    const radius = Math.sqrt(1 - y * y)
    const sf = 15
    const x = Math.cos(theta) * radius * sf
    const z = Math.sin(theta) * radius * sf
    const yScaled = y * sf
    if (node.position) { node.position.x = x; node.position.y = yScaled; node.position.z = z }
    else                { node.position = { x, y: yScaled, z } }
    return [x, yScaled, z]
  }
  return [node.position.x, node.position.y, node.position.z]
}
