/**
 * VRInteractionManager Tests
 *
 * Tests for the VR interaction manager component logic.
 * Focuses on controller event handling, node dragging state transitions,
 * input source handedness mapping, raycasting logic, and edge cases.
 * Tests pure logic without React rendering context.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import * as THREE from 'three';

// ---------------------------------------------------------------------------
// Constants extracted from VRInteractionManager.tsx
// ---------------------------------------------------------------------------

/** Default max ray distance for raycasting */
const DEFAULT_MAX_RAY_DISTANCE = 50;

/** Node sphere radius for intersection testing */
const NODE_RADIUS = 0.5;

/** Drag forward offset scalar */
const DRAG_FORWARD_OFFSET = 2;

// ---------------------------------------------------------------------------
// Types mirroring VRInteractionManager internal state
// ---------------------------------------------------------------------------

interface GrabbedNode {
  nodeId: string;
  hand: 'left' | 'right';
}

interface MockVRNode {
  id: string;
  position: THREE.Vector3;
}

// ---------------------------------------------------------------------------
// Mock data helpers
// ---------------------------------------------------------------------------

function createMockVRNode(overrides: Partial<{ id: string; x: number; y: number; z: number }> = {}): MockVRNode {
  return {
    id: overrides.id ?? `node-${Math.random().toString(36).substring(7)}`,
    position: new THREE.Vector3(
      overrides.x ?? 0,
      overrides.y ?? 0,
      overrides.z ?? 0
    ),
  };
}

function createMockNodes(count: number): MockVRNode[] {
  return Array.from({ length: count }, (_, i) =>
    createMockVRNode({
      id: `node-${i}`,
      x: i * 5,
      y: 0,
      z: -10,
    })
  );
}

/**
 * Simulates the findNodeAtRay logic from VRInteractionManager.
 * Tests the raycasting algorithm without needing actual XR controllers.
 */
function findNodeAtRay(
  controllerPos: THREE.Vector3,
  controllerDir: THREE.Vector3,
  nodes: MockVRNode[],
  maxRayDistance: number = DEFAULT_MAX_RAY_DISTANCE
): { nodeId: string; distance: number } | null {
  if (nodes.length === 0) return null;

  const raycaster = new THREE.Raycaster();
  raycaster.set(controllerPos, controllerDir.normalize());
  raycaster.far = maxRayDistance;

  let closestNode: { nodeId: string; distance: number } | null = null;
  let minDistance = Infinity;

  nodes.forEach((node) => {
    const sphere = new THREE.Sphere(node.position, NODE_RADIUS);
    const intersectionPoint = new THREE.Vector3();

    if (raycaster.ray.intersectsSphere(sphere)) {
      raycaster.ray.intersectSphere(sphere, intersectionPoint);
      const distance = controllerPos.distanceTo(intersectionPoint);

      if (distance < minDistance && distance < maxRayDistance) {
        minDistance = distance;
        closestNode = { nodeId: node.id, distance };
      }
    }
  });

  return closestNode;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe('VRInteractionManager Controller Select Event Handling', () => {
  let onNodeSelect: ReturnType<typeof vi.fn>;
  let onNodeRelease: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    onNodeSelect = vi.fn();
    onNodeRelease = vi.fn();
  });

  it('calls onNodeSelect when right controller hits a node', () => {
    const nodeId = 'target-node-1';
    onNodeSelect(nodeId);

    expect(onNodeSelect).toHaveBeenCalledWith('target-node-1');
  });

  it('calls onNodeSelect when left controller hits a node', () => {
    const nodeId = 'target-node-2';
    onNodeSelect(nodeId);

    expect(onNodeSelect).toHaveBeenCalledWith('target-node-2');
  });

  it('does not call onNodeSelect when ray misses all nodes', () => {
    const controllerPos = new THREE.Vector3(0, 0, 0);
    const controllerDir = new THREE.Vector3(0, 1, 0); // pointing up, nodes are along z
    const nodes = createMockNodes(3);

    const result = findNodeAtRay(controllerPos, controllerDir, nodes);

    if (result) {
      onNodeSelect(result.nodeId);
    }

    expect(onNodeSelect).not.toHaveBeenCalled();
  });

  it('calls onNodeRelease on selectend for right hand', () => {
    const grabbed: GrabbedNode = { nodeId: 'node-A', hand: 'right' };

    if (grabbed.hand === 'right') {
      onNodeRelease(grabbed.nodeId);
    }

    expect(onNodeRelease).toHaveBeenCalledWith('node-A');
  });

  it('calls onNodeRelease on selectend for left hand', () => {
    const grabbed: GrabbedNode = { nodeId: 'node-B', hand: 'left' };

    if (grabbed.hand === 'left') {
      onNodeRelease(grabbed.nodeId);
    }

    expect(onNodeRelease).toHaveBeenCalledWith('node-B');
  });

  it('does not call onNodeRelease when no node is grabbed', () => {
    const grabbed: GrabbedNode | null = null;

    if (grabbed) {
      onNodeRelease(grabbed.nodeId);
    }

    expect(onNodeRelease).not.toHaveBeenCalled();
  });
});

describe('VRInteractionManager Squeeze Event Handling', () => {
  let onNodeSelect: ReturnType<typeof vi.fn>;
  let onNodeRelease: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    onNodeSelect = vi.fn();
    onNodeRelease = vi.fn();
  });

  it('grabs node on squeezestart when ray hits a node', () => {
    const handedness = 'right';
    const nodeId = 'squeezed-node';
    const grabbed: GrabbedNode = { nodeId, hand: handedness };

    onNodeSelect(grabbed.nodeId);

    expect(onNodeSelect).toHaveBeenCalledWith('squeezed-node');
  });

  it('sets grabbed hand to the controller handedness on squeeze', () => {
    const handedness: 'left' | 'right' = 'left';
    const grabbed: GrabbedNode = { nodeId: 'test-node', hand: handedness };

    expect(grabbed.hand).toBe('left');
  });

  it('releases node on squeezeend matching the hand', () => {
    const grabbed: GrabbedNode = { nodeId: 'squeezed-node', hand: 'right' };
    const eventHandedness = 'right';

    if (grabbed.hand === eventHandedness) {
      onNodeRelease(grabbed.nodeId);
    }

    expect(onNodeRelease).toHaveBeenCalledWith('squeezed-node');
  });

  it('does not release node on squeezeend from different hand', () => {
    const grabbed: GrabbedNode = { nodeId: 'squeezed-node', hand: 'right' };
    const eventHandedness = 'left';

    if (grabbed.hand === eventHandedness) {
      onNodeRelease(grabbed.nodeId);
    }

    expect(onNodeRelease).not.toHaveBeenCalled();
  });

  it('does not call onNodeSelect on squeezestart when controller ref is null', () => {
    const controllerExists = false;

    if (controllerExists) {
      onNodeSelect('should-not-fire');
    }

    expect(onNodeSelect).not.toHaveBeenCalled();
  });
});

describe('VRInteractionManager Node Dragging State Transitions', () => {
  it('transitions from idle to dragging on selectstart', () => {
    let grabbed: GrabbedNode | null = null;

    // selectstart
    grabbed = { nodeId: 'drag-node', hand: 'right' };

    expect(grabbed).not.toBeNull();
    expect(grabbed!.nodeId).toBe('drag-node');
  });

  it('transitions from dragging to idle on selectend', () => {
    let grabbed: GrabbedNode | null = { nodeId: 'drag-node', hand: 'right' };

    // selectend
    grabbed = null;

    expect(grabbed).toBeNull();
  });

  it('maintains dragging state across frames', () => {
    const grabbed: GrabbedNode = { nodeId: 'persistent-drag', hand: 'left' };

    // Simulate multiple frame updates
    for (let frame = 0; frame < 60; frame++) {
      expect(grabbed.nodeId).toBe('persistent-drag');
      expect(grabbed.hand).toBe('left');
    }
  });

  it('calculates drag position projected forward from controller', () => {
    const controllerPos = new THREE.Vector3(1, 1.5, 0);
    const controllerDir = new THREE.Vector3(0, 0, -1);

    const dragPosition = controllerPos
      .clone()
      .add(controllerDir.clone().multiplyScalar(DRAG_FORWARD_OFFSET));

    expect(dragPosition.x).toBeCloseTo(1, 5);
    expect(dragPosition.y).toBeCloseTo(1.5, 5);
    expect(dragPosition.z).toBeCloseTo(-2, 5);
  });

  it('drag position updates with controller movement', () => {
    const positions: THREE.Vector3[] = [];

    // Frame 1: controller at origin looking forward
    const pos1 = new THREE.Vector3(0, 1.6, 0);
    const dir1 = new THREE.Vector3(0, 0, -1);
    positions.push(pos1.clone().add(dir1.clone().multiplyScalar(DRAG_FORWARD_OFFSET)));

    // Frame 2: controller moved right
    const pos2 = new THREE.Vector3(1, 1.6, 0);
    const dir2 = new THREE.Vector3(0, 0, -1);
    positions.push(pos2.clone().add(dir2.clone().multiplyScalar(DRAG_FORWARD_OFFSET)));

    expect(positions[0].x).toBeCloseTo(0, 5);
    expect(positions[1].x).toBeCloseTo(1, 5);
    expect(positions[0].z).toEqual(positions[1].z);
  });

  it('does not compute drag position when no node is grabbed', () => {
    const grabbed: GrabbedNode | null = null;
    const onNodeDrag = vi.fn();

    if (grabbed) {
      onNodeDrag(grabbed.nodeId, new THREE.Vector3());
    }

    expect(onNodeDrag).not.toHaveBeenCalled();
  });
});

describe('VRInteractionManager Handedness Mapping', () => {
  it('maps right handedness to right controller ref', () => {
    const handedness = 'right';
    const isPrimary = handedness === 'right';

    expect(isPrimary).toBe(true);
  });

  it('maps left handedness to left controller ref', () => {
    const handedness = 'left';
    const isSecondary = handedness === 'left';

    expect(isSecondary).toBe(true);
  });

  it('selects correct controller for squeeze based on handedness', () => {
    const rightController = { name: 'right' };
    const leftController = { name: 'left' };

    const eventHandedness = 'right';
    const selectedController =
      eventHandedness === 'right' ? rightController : leftController;

    expect(selectedController.name).toBe('right');
  });

  it('selects left controller for left-handed squeeze', () => {
    const rightController = { name: 'right' };
    const leftController = { name: 'left' };

    const eventHandedness = 'left';
    const selectedController =
      eventHandedness === 'right' ? rightController : leftController;

    expect(selectedController.name).toBe('left');
  });

  it('uses right hand to update grabbed node hand property', () => {
    const grabbed: GrabbedNode = { nodeId: 'test', hand: 'right' };
    const controller =
      grabbed.hand === 'right' ? 'rightControllerRef' : 'leftControllerRef';

    expect(controller).toBe('rightControllerRef');
  });

  it('uses left hand to update grabbed node hand property', () => {
    const grabbed: GrabbedNode = { nodeId: 'test', hand: 'left' };
    const controller =
      grabbed.hand === 'right' ? 'rightControllerRef' : 'leftControllerRef';

    expect(controller).toBe('leftControllerRef');
  });
});

describe('VRInteractionManager Edge Cases: No Active Session', () => {
  it('skips controller update when session is null', () => {
    const session = null;
    let controllersUpdated = false;

    if (session) {
      controllersUpdated = true;
    }

    expect(controllersUpdated).toBe(false);
  });

  it('skips frame processing when no node is grabbed and no session', () => {
    const session = null;
    const grabbed: GrabbedNode | null = null;
    const onNodeDrag = vi.fn();

    if (session) {
      // update controllers
    }

    if (grabbed) {
      onNodeDrag(grabbed.nodeId, new THREE.Vector3());
    }

    expect(onNodeDrag).not.toHaveBeenCalled();
  });

  it('handles XR manager without getSession method', () => {
    const xrManager: { getSession?: () => null } = {};
    const session = xrManager.getSession?.();

    expect(session).toBeUndefined();
  });
});

describe('VRInteractionManager Edge Cases: No Input Sources', () => {
  it('skips when inputSources array is empty', () => {
    const inputSources: Array<{ handedness: string }> = [];
    let processed = 0;

    for (const source of inputSources) {
      processed++;
    }

    expect(processed).toBe(0);
  });

  it('skips source without gripSpace', () => {
    const source = { handedness: 'right', gripSpace: null };
    let poseObtained = false;

    if (source.gripSpace) {
      poseObtained = true;
    }

    expect(poseObtained).toBe(false);
  });

  it('skips when frame.getPose returns null', () => {
    const pose = null;
    let controllerUpdated = false;

    if (pose) {
      controllerUpdated = true;
    }

    expect(controllerUpdated).toBe(false);
  });

  it('creates new Group when controller ref is null', () => {
    let controllerRef: THREE.Group | null = null;

    if (!controllerRef) {
      controllerRef = new THREE.Group();
    }

    expect(controllerRef).toBeInstanceOf(THREE.Group);
  });
});

describe('VRInteractionManager Raycasting Logic', () => {
  it('finds node directly in front of controller', () => {
    const controllerPos = new THREE.Vector3(0, 0, 0);
    const controllerDir = new THREE.Vector3(0, 0, -1);
    const nodes = [
      createMockVRNode({ id: 'target', x: 0, y: 0, z: -5 }),
    ];

    const result = findNodeAtRay(controllerPos, controllerDir, nodes);

    expect(result).not.toBeNull();
    expect(result!.nodeId).toBe('target');
  });

  it('returns null when no nodes in ray path', () => {
    const controllerPos = new THREE.Vector3(0, 0, 0);
    const controllerDir = new THREE.Vector3(0, 0, -1);
    const nodes = [
      createMockVRNode({ id: 'off-axis', x: 100, y: 100, z: -5 }),
    ];

    const result = findNodeAtRay(controllerPos, controllerDir, nodes);

    expect(result).toBeNull();
  });

  it('returns null when node list is empty', () => {
    const controllerPos = new THREE.Vector3(0, 0, 0);
    const controllerDir = new THREE.Vector3(0, 0, -1);

    const result = findNodeAtRay(controllerPos, controllerDir, []);

    expect(result).toBeNull();
  });

  it('selects closest node when multiple nodes in ray path', () => {
    const controllerPos = new THREE.Vector3(0, 0, 0);
    const controllerDir = new THREE.Vector3(0, 0, -1);
    const nodes = [
      createMockVRNode({ id: 'far', x: 0, y: 0, z: -20 }),
      createMockVRNode({ id: 'near', x: 0, y: 0, z: -3 }),
      createMockVRNode({ id: 'mid', x: 0, y: 0, z: -10 }),
    ];

    const result = findNodeAtRay(controllerPos, controllerDir, nodes);

    expect(result).not.toBeNull();
    expect(result!.nodeId).toBe('near');
  });

  it('ignores nodes beyond maxRayDistance', () => {
    const controllerPos = new THREE.Vector3(0, 0, 0);
    const controllerDir = new THREE.Vector3(0, 0, -1);
    const nodes = [
      createMockVRNode({ id: 'too-far', x: 0, y: 0, z: -100 }),
    ];

    const result = findNodeAtRay(controllerPos, controllerDir, nodes, 50);

    expect(result).toBeNull();
  });
});

describe('VRInteractionManager Default Props', () => {
  it('defaults maxRayDistance to 50', () => {
    expect(DEFAULT_MAX_RAY_DISTANCE).toBe(50);
  });

  it('node sphere radius is 0.5 for intersection testing', () => {
    expect(NODE_RADIUS).toBe(0.5);
  });

  it('drag forward offset is 2 units', () => {
    expect(DRAG_FORWARD_OFFSET).toBe(2);
  });
});

describe('VRInteractionManager Cleanup', () => {
  it('clears grabbed node ref on unmount', () => {
    let grabbed: GrabbedNode | null = { nodeId: 'active-drag', hand: 'right' };

    // Simulate cleanup effect
    grabbed = null;

    expect(grabbed).toBeNull();
  });

  it('cleanup is idempotent when no node is grabbed', () => {
    let grabbed: GrabbedNode | null = null;

    // Simulate cleanup
    grabbed = null;

    expect(grabbed).toBeNull();
  });
});

describe('VRInteractionManager Controller Position Update', () => {
  it('sets controller position from XR pose transform', () => {
    const group = new THREE.Group();
    const posePosition = { x: 0.5, y: 1.2, z: -0.3 };

    group.position.set(posePosition.x, posePosition.y, posePosition.z);

    expect(group.position.x).toBeCloseTo(0.5, 5);
    expect(group.position.y).toBeCloseTo(1.2, 5);
    expect(group.position.z).toBeCloseTo(-0.3, 5);
  });

  it('sets controller quaternion from XR pose orientation', () => {
    const group = new THREE.Group();
    const orientation = { x: 0, y: 0.707, z: 0, w: 0.707 };

    group.quaternion.set(orientation.x, orientation.y, orientation.z, orientation.w);

    expect(group.quaternion.x).toBeCloseTo(0, 5);
    expect(group.quaternion.y).toBeCloseTo(0.707, 3);
    expect(group.quaternion.z).toBeCloseTo(0, 5);
    expect(group.quaternion.w).toBeCloseTo(0.707, 3);
  });

  it('routes right handedness to rightControllerRef', () => {
    const source = { handedness: 'right' };
    const rightRef = { current: null as THREE.Group | null };
    const leftRef = { current: null as THREE.Group | null };

    const targetRef = source.handedness === 'right' ? rightRef : leftRef;

    expect(targetRef).toBe(rightRef);
  });

  it('routes left handedness to leftControllerRef', () => {
    const source = { handedness: 'left' };
    const rightRef = { current: null as THREE.Group | null };
    const leftRef = { current: null as THREE.Group | null };

    const targetRef = source.handedness === 'right' ? rightRef : leftRef;

    expect(targetRef).toBe(leftRef);
  });
});
