import { ref, computed, onBeforeUnmount, provide, markRaw, shallowRef, watch } from 'vue';
import * as THREE from 'three';
import { OrbitControls as ThreeOrbitControls } from 'three/examples/jsm/controls/OrbitControls.js';
import { 
  forceSimulation, 
  forceLink, 
  forceManyBody, 
  forceCenter,
  Simulation,
  SimulationNodeDatum,
  SimulationLinkDatum,
  ForceLink
} from 'd3-force-3d';
import { useSettingsStore } from '../stores/settings';
import { useVisualizationStore } from '../stores/visualization';
import { useBinaryUpdateStore } from '../stores/binaryUpdate';
import { useWebSocketStore } from '../stores/websocket';
import type { Node, Edge, CoreState, InitializationOptions, GraphNode, GraphEdge, GraphData } from '../types/core';
import { POSITION_SCALE } from '../constants/websocket';
import { VISUALIZATION_CONSTANTS as CONSTANTS, LIGHT_SETTINGS, SCENE_SETTINGS, FORCE_SETTINGS, CAMERA_SETTINGS } from '../constants/visualization';

// Symbol for providing scene to components
export const SCENE_KEY = Symbol('three-scene');

// Extend SimulationNodeDatum to include our node properties
interface ForceNode extends SimulationNodeDatum {
  id: string;
  x?: number;
  y?: number;
  z?: number;
  position?: [number, number, number];
  velocity?: [number, number, number];
  fx?: number | null;
  fy?: number | null;
  fz?: number | null;
}

// Add binary data helper
function createBinaryPositionData(nodes: ForceNode[]): ArrayBuffer {
  // 4 bytes for header + 24 bytes per node (x,y,z,vx,vy,vz as f32)
  const buffer = new ArrayBuffer(4 + nodes.length * 24);
  const view = new DataView(buffer);
  
  // Write header (1.0 to indicate client-side force update)
  view.setFloat32(0, 1.0, true);
  
  // Write node positions and velocities
  nodes.forEach((node, i) => {
    const offset = 4 + i * 24;
    view.setFloat32(offset, node.x || 0, true);
    view.setFloat32(offset + 4, node.y || 0, true);
    view.setFloat32(offset + 8, node.z || 0, true);
    view.setFloat32(offset + 12, node.vx || 0, true);
    view.setFloat32(offset + 16, node.vy || 0, true);
    view.setFloat32(offset + 20, node.vz || 0, true);
  });
  
  return buffer;
}

export function useVisualization() {
  const settingsStore = useSettingsStore();
  const visualizationStore = useVisualizationStore();
  const binaryStore = useBinaryUpdateStore();
  const webSocketStore = useWebSocketStore();
  
  // Core visualization state
  const state = shallowRef<CoreState>({
    renderer: null,
    camera: null,
    scene: null,
    canvas: null,
    isInitialized: false,
    isXRSupported: false,
    isWebGL2: false,
    isGPUMode: false,
    fps: 0,
    lastFrameTime: 0
  });

  // Mesh cache using Maps for O(1) lookup
  const meshCache = {
    nodes: new Map<string, THREE.Mesh>()
  };

  // Interaction state
  const hoveredNode = ref<string | null>(null);
  const selectedNode = ref<string | null>(null);
  const isProcessingUpdate = ref(false);
  const isInteracting = ref(false);

  // GPU acceleration state
  const isGPUEnabled = computed(() => webSocketStore.isGPUEnabled);

  // Track animation frame for cleanup
  let animationFrameId: number | null = null;
  let controls: ThreeOrbitControls | null = null;

  // Initialize force simulation (only used during interactions)
  const simulation = forceSimulation<ForceNode>()
    .stop() // Initially stopped since we use server updates by default
    .alpha(FORCE_SETTINGS.alpha)
    .alphaDecay(FORCE_SETTINGS.alphaDecay)
    .velocityDecay(FORCE_SETTINGS.velocityDecay)
    .force('link', forceLink<ForceNode>()
      .id((d: ForceNode) => d.id)
      .distance(FORCE_SETTINGS.linkDistance)
      .strength(FORCE_SETTINGS.linkStrength))
    .force('charge', forceManyBody<ForceNode>()
      .strength(FORCE_SETTINGS.charge))
    .force('center', forceCenter());

  // Public method to trigger immediate layout updates
  const updateLayoutPositions = () => {
    if (!isInteracting.value) {
      startInteraction();
    }
    
    // Reheat the simulation for immediate updates
    simulation
      .alpha(FORCE_SETTINGS.alpha)
      .restart();
  };

  // Create or update node mesh with efficient caching
  const createNodeMesh = (node: Node): THREE.Mesh => {
    const geometry = new THREE.SphereGeometry(0.02, 32, 32);
    const material = new THREE.MeshStandardMaterial({
      color: node.color || 0xffffff,
      metalness: 0.3,
      roughness: 0.7,
      emissive: node.color || 0xffffff,
      emissiveIntensity: 0.2
    });

    const mesh = new THREE.Mesh(geometry, material);
    mesh.castShadow = true;
    mesh.receiveShadow = true;
    
    if (node.position) {
      mesh.position.set(
        node.position[0] / POSITION_SCALE,
        node.position[1] / POSITION_SCALE,
        node.position[2] / POSITION_SCALE
      );
    }
    
    const size = (node.size || 1) * 0.02;
    mesh.scale.setScalar(size);

    mesh.userData = {
      id: node.id,
      type: 'node',
      originalData: { ...node }
    };

    return markRaw(mesh);
  };

  // Initialize Three.js scene
  const initScene = (canvas: HTMLCanvasElement) => {
    const scene = new THREE.Scene();
    scene.background = new THREE.Color(0x000000);
    scene.fog = new THREE.Fog(0x000000, SCENE_SETTINGS.fogNear, SCENE_SETTINGS.fogFar);

    const camera = new THREE.PerspectiveCamera(
      CAMERA_SETTINGS.fov,
      window.innerWidth / window.innerHeight,
      CAMERA_SETTINGS.near,
      CAMERA_SETTINGS.far
    );
    camera.position.copy(CAMERA_SETTINGS.position);
    camera.lookAt(CAMERA_SETTINGS.target);

    const renderer = new THREE.WebGLRenderer({
      canvas,
      antialias: true,
      alpha: true,
      powerPreference: 'high-performance',
      logarithmicDepthBuffer: true
    });
    renderer.setSize(window.innerWidth, window.innerHeight);
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    renderer.shadowMap.enabled = true;
    renderer.shadowMap.type = THREE.PCFSoftShadowMap;

    // Add lights
    const ambientLight = new THREE.AmbientLight(
      LIGHT_SETTINGS.ambient.color,
      LIGHT_SETTINGS.ambient.intensity
    );
    scene.add(ambientLight);

    const directionalLight = new THREE.DirectionalLight(
      LIGHT_SETTINGS.directional.color,
      LIGHT_SETTINGS.directional.intensity
    );
    directionalLight.position.set(...LIGHT_SETTINGS.directional.position);
    directionalLight.castShadow = true;
    scene.add(directionalLight);
    
    const hemiLight = new THREE.HemisphereLight(
      LIGHT_SETTINGS.hemisphere.skyColor,
      LIGHT_SETTINGS.hemisphere.groundColor,
      LIGHT_SETTINGS.hemisphere.intensity
    );
    hemiLight.position.set(0, 20, 0);
    scene.add(hemiLight);

    // Add controls
    controls = new ThreeOrbitControls(camera, renderer.domElement);
    if (controls) {
      controls.enableDamping = true;
      controls.dampingFactor = 0.05;
      controls.maxDistance = 5;
      controls.minDistance = 0.1;
      controls.maxPolarAngle = Math.PI * 0.8;
      controls.target.copy(CAMERA_SETTINGS.target);
    }

    // Add grid helper
    const gridHelper = new THREE.GridHelper(
      SCENE_SETTINGS.gridSize,
      SCENE_SETTINGS.gridDivisions,
      0x444444,
      0x222222
    );
    scene.add(gridHelper);

    // Add axes helper
    const axesHelper = new THREE.AxesHelper(1);
    scene.add(axesHelper);

    // Store GPU state in scene
    scene.userData.gpuEnabled = isGPUEnabled.value;

    provide(SCENE_KEY, scene);

    return {
      scene: markRaw(scene),
      camera: markRaw(camera),
      renderer: markRaw(renderer)
    };
  };

  // Animation loop with optimized updates and continuous server sync
  const animate = () => {
    if (!state.value.isInitialized) return;

    const { renderer, scene, camera } = state.value;
    if (renderer && scene && camera) {
      controls?.update();

      // Update positions based on interaction state
      if (isInteracting.value) {
        // Use local force simulation during interaction
        simulation.tick();
        
        // Update mesh positions
        simulation.nodes().forEach((node: ForceNode) => {
          const mesh = meshCache.nodes.get(node.id);
          if (mesh && typeof node.x === 'number' && typeof node.y === 'number' && typeof node.z === 'number') {
            mesh.position.set(
              node.x / POSITION_SCALE,
              node.y / POSITION_SCALE,
              node.z / POSITION_SCALE
            );
          }
        });

        // Send binary update to server
        const binaryData = createBinaryPositionData(simulation.nodes());
        webSocketStore.sendBinary(binaryData);

        scene.userData.needsRender = true;
      } else {
        // Use server-provided positions when not interacting
        const positions = binaryStore.getAllPositions;
        const nodeCount = binaryStore.nodeCount;
        
        for (let i = 0; i < nodeCount; i++) {
          const node = simulation.nodes()[i] as ForceNode;
          if (!node) continue;

          const mesh = meshCache.nodes.get(node.id);
          if (!mesh) continue;

          const posOffset = i * 3;
          mesh.position.set(
            positions[posOffset] / POSITION_SCALE,
            positions[posOffset + 1] / POSITION_SCALE,
            positions[posOffset + 2] / POSITION_SCALE
          );
        }
      }

      const currentTime = performance.now();
      const delta = currentTime - state.value.lastFrameTime;
      state.value.fps = 1000 / delta;
      state.value.lastFrameTime = currentTime;

      const needsRender = scene.userData?.needsRender !== false || 
                         controls?.enabled || 
                         currentTime - (scene.userData?.lastUpdate || 0) > 1000;

      if (needsRender) {
        renderer.render(scene, camera);
        scene.userData.needsRender = false;
        scene.userData.lastUpdate = currentTime;
      }
    }

    animationFrameId = requestAnimationFrame(animate);
  };

  // Initialize visualization system
  const initialize = async (options: InitializationOptions) => {
    if (state.value.isInitialized) return;

    try {
      const { scene, camera, renderer } = initScene(options.canvas);

      state.value = markRaw({
        renderer,
        camera,
        scene,
        canvas: options.canvas,
        isInitialized: true,
        isXRSupported: false,
        isWebGL2: renderer.capabilities.isWebGL2,
        isGPUMode: isGPUEnabled.value,
        fps: 0,
        lastFrameTime: performance.now()
      });

      // Watch for graph data changes
      watch(() => visualizationStore.graphData, (graphData: GraphData | null) => {
        if (!graphData) return;

        // Update simulation data (but don't start it unless interacting)
        const forceNodes = graphData.nodes.map((node: GraphNode): ForceNode => ({
          ...node,
          x: node.position?.[0] || 0,
          y: node.position?.[1] || 0,
          z: node.position?.[2] || 0
        }));

        const forceLinks = graphData.edges.map((edge: GraphEdge) => ({
          ...edge,
          source: edge.source,
          target: edge.target
        }));

        simulation.nodes(forceNodes);
        simulation.force<ForceLink<ForceNode>>('link')?.links(forceLinks);
      }, { deep: true });

      animate();

      window.addEventListener('resize', () => {
        if (!camera || !renderer) return;
        camera.aspect = window.innerWidth / window.innerHeight;
        camera.updateProjectionMatrix();
        renderer.setSize(window.innerWidth, window.innerHeight);
      });

      console.log('Visualization system initialized');
    } catch (error) {
      console.error('Failed to initialize visualization:', error);
      throw error;
    }
  };

  // Start interaction mode (local force simulation)
  const startInteraction = () => {
    if (isInteracting.value) return;
    
    isInteracting.value = true;
    
    // Copy current positions to simulation
    const positions = binaryStore.getAllPositions;
    simulation.nodes().forEach((node: ForceNode, i: number) => {
      const posOffset = i * 3;
      node.x = positions[posOffset];
      node.y = positions[posOffset + 1];
      node.z = positions[posOffset + 2];
    });

    simulation
      .alpha(FORCE_SETTINGS.alpha)
      .restart();
  };

  // End interaction mode (return to server updates)
  const endInteraction = () => {
    if (!isInteracting.value) return;
    
    isInteracting.value = false;
    simulation.stop();
  };

  // Event handlers
  const handleNodeHover = (nodeId: string | null) => {
    hoveredNode.value = nodeId;
    if (state.value.scene) {
      state.value.scene.userData.needsRender = true;
    }
  };

  const handleNodeSelect = (nodeId: string | null) => {
    selectedNode.value = nodeId;
    if (state.value.scene) {
      state.value.scene.userData.needsRender = true;
    }
  };

  // Update nodes
  const updateNodes = (nodes: Node[]) => {
    if (!state.value.scene || isProcessingUpdate.value) return;

    isProcessingUpdate.value = true;
    try {
      const scene = state.value.scene;
      const currentIds = new Set(nodes.map(n => n.id));

      // Remove old nodes
      for (const [id, mesh] of meshCache.nodes.entries()) {
        if (!currentIds.has(id)) {
          scene.remove(mesh);
          mesh.geometry.dispose();
          (mesh.material as THREE.Material).dispose();
          meshCache.nodes.delete(id);
        }
      }

      // Add or update nodes
      nodes.forEach(node => {
        let mesh = meshCache.nodes.get(node.id);
        
        if (!mesh) {
          // Create new mesh
          mesh = createNodeMesh(node);
          scene.add(mesh);
          meshCache.nodes.set(node.id, mesh);
        } else {
          // Update existing mesh
          if (node.position) {
            mesh.position.set(
              node.position[0] / POSITION_SCALE,
              node.position[1] / POSITION_SCALE,
              node.position[2] / POSITION_SCALE
            );
          }
          if (node.size) {
            mesh.scale.setScalar(node.size * 0.02);
          }
          if (node.color) {
            (mesh.material as THREE.MeshStandardMaterial).color.set(node.color);
            (mesh.material as THREE.MeshStandardMaterial).emissive.set(node.color);
          }
        }
      });

      scene.userData.needsRender = true;
    } finally {
      isProcessingUpdate.value = false;
    }
  };

  // Update positions from binary data
  const updatePositions = (positions: Float32Array, velocities: Float32Array, nodeCount: number) => {
    if (!state.value.scene || !state.value.isInitialized || isProcessingUpdate.value || isInteracting.value) return;

    isProcessingUpdate.value = true;
    try {
      const nodes = simulation.nodes();
      for (let i = 0; i < nodeCount; i++) {
        const node = nodes[i] as ForceNode;
        if (!node) continue;

        const mesh = meshCache.nodes.get(node.id);
        if (!mesh) continue;

        const posOffset = i * 3;
        const velOffset = i * 3;

        mesh.position.set(
          positions[posOffset] / POSITION_SCALE,
          positions[posOffset + 1] / POSITION_SCALE,
          positions[posOffset + 2] / POSITION_SCALE
        );

        // Update node data
        node.x = positions[posOffset];
        node.y = positions[posOffset + 1];
        node.z = positions[posOffset + 2];
        node.vx = velocities[velOffset];
        node.vy = velocities[velOffset + 1];
        node.vz = velocities[velOffset + 2];
      }

      if (state.value.scene) {
        state.value.scene.userData.needsRender = true;
      }
    } finally {
      isProcessingUpdate.value = false;
    }
  };

  // Cleanup
  onBeforeUnmount(() => {
    if (animationFrameId !== null) {
      cancelAnimationFrame(animationFrameId);
    }

    if (controls) {
      controls.dispose();
    }

    simulation.stop();

    // Clean up meshes
    meshCache.nodes.forEach(mesh => {
      mesh.geometry.dispose();
      (mesh.material as THREE.Material).dispose();
    });
    meshCache.nodes.clear();

    if (state.value.renderer) {
      state.value.renderer.dispose();
      state.value.renderer.forceContextLoss();
    }
    
    state.value.canvas?.remove();
    state.value = {
      renderer: null,
      camera: null,
      scene: null,
      canvas: null,
      isInitialized: false,
      isXRSupported: false,
      isWebGL2: false,
      isGPUMode: false,
      fps: 0,
      lastFrameTime: 0
    };
  });

  return {
    state,
    initialize,
    updateNodes,
    updatePositions,
    updateLayoutPositions,
    startInteraction,
    endInteraction,
    handleNodeHover,
    handleNodeSelect,
    hoveredNode: computed(() => hoveredNode.value),
    selectedNode: computed(() => selectedNode.value),
    isGPUEnabled,
    isInteracting: computed(() => isInteracting.value)
  };
}
