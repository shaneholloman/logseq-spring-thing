import { ref, computed, inject, onMounted, watch } from 'vue';
import { Scene, Group, Vector3 } from 'three';
import { useVisualizationStore } from '../stores/visualization';
import { useBinaryUpdateStore } from '../stores/binaryUpdate';
import { useWebSocketStore } from '../stores/websocket';
import type { GraphNode, GraphEdge } from '../types/core';
import type { VisualizationConfig } from '../types/components';
import type { CoreState } from '../types/core';

export function useGraphSystem() {
  const visualizationStore = useVisualizationStore();
  const binaryStore = useBinaryUpdateStore();
  const webSocketStore = useWebSocketStore();
  
  // Get scene from visualization state
  const visualizationState = inject<{ value: CoreState }>('visualizationState');
  
  // Create Three.js groups
  const graphGroup = new Group();
  const nodesGroup = new Group();
  const edgesGroup = new Group();

  // Add groups to scene hierarchy
  graphGroup.add(nodesGroup);
  graphGroup.add(edgesGroup);

  // State
  const hoveredNode = ref<string | null>(null);
  const nodeCount = ref(0);
  const isProcessingUpdate = ref(false);

  // Computed states from WebSocket store
  const isGPUEnabled = computed(() => webSocketStore.isGPUEnabled);
  const isInitialLayout = computed(() => webSocketStore.isInitialLayoutPhase);

  // Watch for GPU state changes
  watch(isGPUEnabled, (enabled) => {
    console.debug(`GPU acceleration ${enabled ? 'enabled' : 'disabled'}`);
    if (visualizationState?.value.scene) {
      visualizationState.value.scene.userData.gpuEnabled = enabled;
      visualizationState.value.scene.userData.needsRender = true;
    }
  });

  // Watch for layout phase changes
  watch(isInitialLayout, (initial) => {
    console.debug(`Layout phase: ${initial ? 'initial' : 'dynamic'}`);
    if (visualizationState?.value.scene) {
      visualizationState.value.scene.userData.initialLayout = initial;
      visualizationState.value.scene.userData.needsRender = true;
    }
  });

  // Get node index from the visualization store's nodes array
  const getNodeIndex = (id: string): number => {
    return visualizationStore.nodes.findIndex(node => node.id === id);
  };

  // Direct access to binary data with GPU awareness
  const getNodePosition = (node: GraphNode | string): Vector3 => {
    const id = typeof node === 'object' ? node.id : node;
    const index = getNodeIndex(id);
    if (index === -1) return new Vector3();

    const position = binaryStore.getNodePosition(index);
    if (position) {
      return new Vector3(position[0], position[1], position[2]);
    }
    return new Vector3();
  };

  const getNodeVelocity = (node: GraphNode | string): Vector3 => {
    const id = typeof node === 'object' ? node.id : node;
    const index = getNodeIndex(id);
    if (index === -1) return new Vector3();

    const velocity = binaryStore.getNodeVelocity(index);
    if (velocity) {
      return new Vector3(velocity[0], velocity[1], velocity[2]);
    }
    return new Vector3();
  };

  const updateNodePosition = (
    id: string,
    position: Vector3,
    velocity: Vector3
  ) => {
    if (isProcessingUpdate.value) return; // Prevent concurrent updates
    
    const index = getNodeIndex(id);
    if (index === -1) return;

    isProcessingUpdate.value = true;
    try {
      binaryStore.updateNodePosition(
        index,
        position.x,
        position.y,
        position.z,
        velocity.x,
        velocity.y,
        velocity.z
      );

      if (visualizationState?.value.scene) {
        visualizationState.value.scene.userData.needsRender = true;
        visualizationState.value.scene.userData.lastUpdate = performance.now();
      }
    } finally {
      isProcessingUpdate.value = false;
    }
  };

  const getNodeScale = (node: GraphNode): number => {
    const baseSize = node.size || 1;
    const minSize = settings.value.min_node_size;
    const maxSize = settings.value.max_node_size;
    return minSize + (baseSize * (maxSize - minSize));
  };

  const getNodeColor = (node: GraphNode): string => {
    return node.id === hoveredNode.value
      ? settings.value.node_color_core
      : (node.color || settings.value.node_color);
  };

  // Edge helpers using direct access
  const getEdgePoints = (source: GraphNode, target: GraphNode): [Vector3, Vector3] => {
    return [
      getNodePosition(source),
      getNodePosition(target)
    ];
  };

  const getEdgeColor = (edge: GraphEdge): string => {
    return edge.color || settings.value.edge_color;
  };

  const getEdgeWidth = (edge: GraphEdge): number => {
    const baseWidth = edge.weight || 1;
    const minWidth = settings.value.edge_min_width;
    const maxWidth = settings.value.edge_max_width;
    return minWidth + (baseWidth * (maxWidth - minWidth));
  };

  // Event handlers with GPU awareness
  const handleNodeClick = (node: GraphNode) => {
    const position = getNodePosition(node);
    console.debug('Node clicked:', { 
      id: node.id, 
      position,
      gpuEnabled: isGPUEnabled.value,
      layoutPhase: isInitialLayout.value ? 'initial' : 'dynamic'
    });
  };

  const handleNodeHover = (node: GraphNode | null) => {
    hoveredNode.value = node?.id || null;
    if (visualizationState?.value.scene) {
      visualizationState.value.scene.userData.needsRender = true;
      visualizationState.value.scene.userData.lastUpdate = performance.now();
    }
  };

  // Graph data management with GPU and layout phase awareness
  const updateGraphData = (graphData: { nodes: GraphNode[]; edges: GraphEdge[] }) => {
    if (isProcessingUpdate.value) return; // Prevent concurrent updates
    
    isProcessingUpdate.value = true;
    try {
      nodeCount.value = graphData.nodes.length;
      
      // Create binary message with initial positions
      const dataSize = 1 + (nodeCount.value * 6); // isInitialLayout flag + (x,y,z,vx,vy,vz) per node
      const binaryData = new ArrayBuffer(dataSize * 4); // 4 bytes per float
      const dataView = new Float32Array(binaryData);
      
      // Set isInitialLayout flag based on current state
      dataView[0] = isInitialLayout.value ? 1 : 0;
      
      // Fill position and velocity data
      let offset = 1; // Start after flag
      graphData.nodes.forEach((node, index) => {
        // Set positions
        dataView[offset] = node.position?.[0] || 0;
        dataView[offset + 1] = node.position?.[1] || 0;
        dataView[offset + 2] = node.position?.[2] || 0;
        
        // Set velocities
        dataView[offset + 3] = node.velocity?.[0] || 0;
        dataView[offset + 4] = node.velocity?.[1] || 0;
        dataView[offset + 5] = node.velocity?.[2] || 0;
        
        offset += 6;
      });

      // Update binary store using the same format as websocket messages
      binaryStore.updateFromBinary({
        data: binaryData,
        isInitialLayout: isInitialLayout.value,
        nodeCount: nodeCount.value
      });

      // Mark scene for update
      if (visualizationState?.value.scene) {
        visualizationState.value.scene.userData.needsRender = true;
        visualizationState.value.scene.userData.lastUpdate = performance.now();
        visualizationState.value.scene.userData.gpuEnabled = isGPUEnabled.value;
        visualizationState.value.scene.userData.initialLayout = isInitialLayout.value;
      }
    } finally {
      isProcessingUpdate.value = false;
    }
  };

  // Get settings from store
  const settings = computed<VisualizationConfig>(() => {
    return visualizationStore.getVisualizationSettings;
  });

  // Initialize scene when available
  onMounted(() => {
    if (visualizationState?.value.scene) {
      visualizationState.value.scene.add(graphGroup);
      visualizationState.value.scene.userData.graphGroup = graphGroup;
      visualizationState.value.scene.userData.nodesGroup = nodesGroup;
      visualizationState.value.scene.userData.edgesGroup = edgesGroup;
      visualizationState.value.scene.userData.gpuEnabled = isGPUEnabled.value;
      visualizationState.value.scene.userData.initialLayout = isInitialLayout.value;
    }
  });

  return {
    // Groups
    graphGroup,
    nodesGroup,
    edgesGroup,
    
    // State
    hoveredNode,
    nodeCount,
    isGPUEnabled,
    isInitialLayout,
    
    // Node helpers
    getNodePosition,
    getNodeVelocity,
    updateNodePosition,
    getNodeScale,
    getNodeColor,
    
    // Edge helpers
    getEdgePoints,
    getEdgeColor,
    getEdgeWidth,
    
    // Event handlers
    handleNodeClick,
    handleNodeHover,
    
    // Data management
    updateGraphData
  };
}
