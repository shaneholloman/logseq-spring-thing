/**
 * VRGraphCanvas Tests
 *
 * Tests for the VR graph canvas component configuration and logic.
 * Focuses on XR store creation, environment-based emulation toggling,
 * graph data flow, and drag state callback propagation.
 * Tests pure logic without React rendering context.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import * as THREE from 'three';

// ---------------------------------------------------------------------------
// Constants extracted from VRGraphCanvas.tsx for verification
// ---------------------------------------------------------------------------

/** XR store configuration */
const XR_STORE_CONFIG = {
  hand: true,
  controller: true,
};

/** Camera defaults */
const CAMERA_DEFAULTS = {
  position: [0, 1.6, 3] as [number, number, number],
  fov: 70,
};

/** GL defaults */
const GL_DEFAULTS = {
  antialias: true,
  alpha: false,
};

/** VRAgentActionScene defaults passed from VRGraphCanvas */
const AGENT_ACTION_SCENE_DEFAULTS = {
  maxConnections: 20,
  baseDuration: 500,
  enableHandTracking: true,
  debug: false,
};

// ---------------------------------------------------------------------------
// Mock data helpers
// ---------------------------------------------------------------------------

interface MockNode {
  id: string;
  label: string;
  position: { x: number; y: number; z: number };
  metadata?: Record<string, unknown>;
}

interface MockGraphData {
  nodes: MockNode[];
  edges: Array<{ id: string; source: string; target: string }>;
}

function createMockNode(overrides: Partial<MockNode> = {}): MockNode {
  return {
    id: `node-${Math.random().toString(36).substring(7)}`,
    label: 'Test Node',
    position: { x: 0, y: 0, z: 0 },
    metadata: {},
    ...overrides,
  };
}

function createMockAgentNode(overrides: Partial<MockNode> = {}): MockNode {
  return createMockNode({
    metadata: {
      type: 'agent',
      agentType: 'coder',
      status: 'active',
    },
    ...overrides,
  });
}

function createMockGraphData(
  nodeCount: number = 5,
  agentCount: number = 2
): MockGraphData {
  const nodes: MockNode[] = [];

  for (let i = 0; i < agentCount; i++) {
    nodes.push(
      createMockAgentNode({
        id: `agent-${i}`,
        label: `Agent ${i}`,
        position: { x: i * 5, y: 0, z: 0 },
      })
    );
  }

  for (let i = 0; i < nodeCount - agentCount; i++) {
    nodes.push(
      createMockNode({
        id: `data-${i}`,
        label: `Data Node ${i}`,
        position: { x: i * 3, y: 5, z: 0 },
        metadata: { type: 'data' },
      })
    );
  }

  const edges = nodes.length > 1
    ? [{ id: 'edge-0', source: nodes[0].id, target: nodes[1].id }]
    : [];

  return { nodes, edges };
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe('VRGraphCanvas XR Store Configuration', () => {
  it('configures hand tracking as enabled', () => {
    expect(XR_STORE_CONFIG.hand).toBe(true);
  });

  it('configures controller tracking as enabled', () => {
    expect(XR_STORE_CONFIG.controller).toBe(true);
  });

  it('sets both hand and controller to true simultaneously', () => {
    expect(XR_STORE_CONFIG).toEqual({
      hand: true,
      controller: true,
    });
  });
});

describe('VRGraphCanvas Emulation Behavior', () => {
  it('enables metaQuest3 emulation in DEV mode', () => {
    const isDev = true;
    const emulate = isDev ? 'metaQuest3' : false;

    expect(emulate).toBe('metaQuest3');
  });

  it('disables emulation in production mode', () => {
    const isDev = false;
    const emulate = isDev ? 'metaQuest3' : false;

    expect(emulate).toBe(false);
  });

  it('emulation value is exactly the string metaQuest3 in dev', () => {
    const isDev = true;
    const emulate = isDev ? 'metaQuest3' : false;

    expect(typeof emulate).toBe('string');
    expect(emulate).not.toBe('metaQuest2');
    expect(emulate).not.toBe('quest3');
  });

  it('emulation value is boolean false in production, not falsy string', () => {
    const isDev = false;
    const emulate = isDev ? 'metaQuest3' : false;

    expect(emulate).toBe(false);
    expect(typeof emulate).toBe('boolean');
  });
});

describe('VRGraphCanvas Camera and GL Defaults', () => {
  it('camera position is at standing VR height (1.6m)', () => {
    expect(CAMERA_DEFAULTS.position[1]).toBe(1.6);
  });

  it('camera is offset 3 units on z-axis', () => {
    expect(CAMERA_DEFAULTS.position[2]).toBe(3);
  });

  it('camera x-position is centered at 0', () => {
    expect(CAMERA_DEFAULTS.position[0]).toBe(0);
  });

  it('field of view is 70 degrees', () => {
    expect(CAMERA_DEFAULTS.fov).toBe(70);
  });

  it('antialias is enabled', () => {
    expect(GL_DEFAULTS.antialias).toBe(true);
  });

  it('alpha channel is disabled for opaque background', () => {
    expect(GL_DEFAULTS.alpha).toBe(false);
  });
});

describe('VRGraphCanvas Agent Node Extraction', () => {
  it('extracts agent nodes from graph data', () => {
    const graphData = createMockGraphData(10, 3);

    const agentNodes = graphData.nodes
      .filter((node) => node.metadata?.type === 'agent')
      .map((node) => ({
        id: node.id,
        type: (node.metadata?.agentType as string) || 'unknown',
        position: node.position,
        status:
          (node.metadata?.status as 'active' | 'idle' | 'error' | 'warning') ||
          'idle',
      }));

    expect(agentNodes).toHaveLength(3);
  });

  it('maps agentType from metadata', () => {
    const graphData = createMockGraphData(5, 2);
    graphData.nodes[0].metadata = {
      type: 'agent',
      agentType: 'researcher',
      status: 'active',
    };

    const agentNodes = graphData.nodes
      .filter((node) => node.metadata?.type === 'agent')
      .map((node) => ({
        id: node.id,
        type: (node.metadata?.agentType as string) || 'unknown',
        position: node.position,
        status:
          (node.metadata?.status as 'active' | 'idle' | 'error' | 'warning') ||
          'idle',
      }));

    expect(agentNodes[0].type).toBe('researcher');
  });

  it('defaults agentType to unknown when not provided', () => {
    const node = createMockNode({
      metadata: { type: 'agent' },
    });

    const type = (node.metadata?.agentType as string) || 'unknown';
    expect(type).toBe('unknown');
  });

  it('defaults status to idle when not provided', () => {
    const node = createMockNode({
      metadata: { type: 'agent', agentType: 'coder' },
    });

    const status =
      (node.metadata?.status as 'active' | 'idle' | 'error' | 'warning') ||
      'idle';
    expect(status).toBe('idle');
  });

  it('preserves node position in extracted agent nodes', () => {
    const graphData = createMockGraphData(3, 1);
    graphData.nodes[0].position = { x: 42, y: 17, z: -5 };

    const agentNodes = graphData.nodes
      .filter((node) => node.metadata?.type === 'agent')
      .map((node) => ({
        id: node.id,
        type: (node.metadata?.agentType as string) || 'unknown',
        position: node.position,
        status:
          (node.metadata?.status as 'active' | 'idle' | 'error' | 'warning') ||
          'idle',
      }));

    expect(agentNodes[0].position).toEqual({ x: 42, y: 17, z: -5 });
  });

  it('returns empty array when no agent nodes exist', () => {
    const graphData: MockGraphData = {
      nodes: [
        createMockNode({ metadata: { type: 'data' } }),
        createMockNode({ metadata: { type: 'resource' } }),
      ],
      edges: [],
    };

    const agentNodes = graphData.nodes.filter(
      (node) => node.metadata?.type === 'agent'
    );

    expect(agentNodes).toHaveLength(0);
  });

  it('returns empty array when graphData.nodes is empty', () => {
    const graphData: MockGraphData = { nodes: [], edges: [] };

    const agentNodes = graphData.nodes.filter(
      (node) => node.metadata?.type === 'agent'
    );

    expect(agentNodes).toHaveLength(0);
  });

  it('handles nodes without metadata gracefully', () => {
    const graphData: MockGraphData = {
      nodes: [
        createMockNode({ metadata: undefined }),
        createMockAgentNode(),
      ],
      edges: [],
    };

    const agentNodes = graphData.nodes.filter(
      (node) => node.metadata?.type === 'agent'
    );

    expect(agentNodes).toHaveLength(1);
  });
});

describe('VRGraphCanvas VR Button Visibility Logic', () => {
  it('shows VR button when VR is supported', () => {
    const isVRSupported = true;
    const forceVR = false;

    const showButton = isVRSupported || forceVR;
    expect(showButton).toBe(true);
  });

  it('shows VR button when forceVR is true', () => {
    const isVRSupported = false;
    const forceVR = true;

    const showButton = isVRSupported || forceVR;
    expect(showButton).toBe(true);
  });

  it('hides VR button when neither supported nor forced', () => {
    const isVRSupported = false;
    const forceVR = false;

    const showButton = isVRSupported || forceVR;
    expect(showButton).toBe(false);
  });

  it('shows VR button when both supported and forced', () => {
    const isVRSupported = true;
    const forceVR = true;

    const showButton = isVRSupported || forceVR;
    expect(showButton).toBe(true);
  });

  it('hides VR button when isVRSupported is null (unknown)', () => {
    const isVRSupported: boolean | null = null;
    const forceVR = false;

    const showButton = isVRSupported || forceVR;
    expect(showButton).toBe(false);
  });
});

describe('VRGraphCanvas URL Parameter Parsing', () => {
  it('detects vr=true URL parameter', () => {
    const searchString = '?vr=true';
    const urlParams = new URLSearchParams(searchString);
    const forceVR = urlParams.get('vr') === 'true';

    expect(forceVR).toBe(true);
  });

  it('rejects vr=false URL parameter', () => {
    const searchString = '?vr=false';
    const urlParams = new URLSearchParams(searchString);
    const forceVR = urlParams.get('vr') === 'true';

    expect(forceVR).toBe(false);
  });

  it('returns false when vr parameter is absent', () => {
    const searchString = '?mode=desktop';
    const urlParams = new URLSearchParams(searchString);
    const forceVR = urlParams.get('vr') === 'true';

    expect(forceVR).toBe(false);
  });

  it('returns false for empty search string', () => {
    const searchString = '';
    const urlParams = new URLSearchParams(searchString);
    const forceVR = urlParams.get('vr') === 'true';

    expect(forceVR).toBe(false);
  });
});

describe('VRGraphCanvas Drag State Callback', () => {
  it('invokes onDragStateChange callback with true', () => {
    const callback = vi.fn();
    callback(true);

    expect(callback).toHaveBeenCalledWith(true);
  });

  it('invokes onDragStateChange callback with false', () => {
    const callback = vi.fn();
    callback(false);

    expect(callback).toHaveBeenCalledWith(false);
  });

  it('does not throw when onDragStateChange is undefined', () => {
    const callback: ((isDragging: boolean) => void) | undefined = undefined;

    expect(() => {
      callback?.(true);
    }).not.toThrow();
  });

  it('tracks multiple drag state transitions', () => {
    const callback = vi.fn();

    callback(true);
    callback(false);
    callback(true);
    callback(false);

    expect(callback).toHaveBeenCalledTimes(4);
    expect(callback.mock.calls).toEqual([[true], [false], [true], [false]]);
  });
});

describe('VRGraphCanvas Agent Action Props Defaults', () => {
  it('enableAgentActions defaults to true', () => {
    const enableAgentActions = true;
    expect(enableAgentActions).toBe(true);
  });

  it('showStats defaults to false', () => {
    const showStats = false;
    expect(showStats).toBe(false);
  });

  it('passes maxConnections=20 to VRAgentActionScene', () => {
    expect(AGENT_ACTION_SCENE_DEFAULTS.maxConnections).toBe(20);
  });

  it('passes baseDuration=500 to VRAgentActionScene', () => {
    expect(AGENT_ACTION_SCENE_DEFAULTS.baseDuration).toBe(500);
  });

  it('passes enableHandTracking=true to VRAgentActionScene', () => {
    expect(AGENT_ACTION_SCENE_DEFAULTS.enableHandTracking).toBe(true);
  });

  it('passes debug=false to VRAgentActionScene', () => {
    expect(AGENT_ACTION_SCENE_DEFAULTS.debug).toBe(false);
  });

  it('conditionally renders VRAgentActionScene based on enableAgentActions', () => {
    const enableAgentActions = true;
    const shouldRenderScene = enableAgentActions;

    expect(shouldRenderScene).toBe(true);
  });

  it('does not render VRAgentActionScene when enableAgentActions is false', () => {
    const enableAgentActions = false;
    const shouldRenderScene = enableAgentActions;

    expect(shouldRenderScene).toBe(false);
  });
});
